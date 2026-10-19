//! This decoder against the one it replaced (`reference`), on the same calls: generated streams
//! of every kind, then truncated, corrupted and random ones. Every call must return the same
//! count, result and bytes. `INFLATE_DIFF_CASES` sets the number of cases (default 3000).

extern crate std;

use super::*;
use crate::reference::{RefInflater, RefWindow};
use core::fmt::Write as _;
use miniz_oxide::deflate::{compress_to_vec, compress_to_vec_zlib};
use std::boxed::Box;
use std::string::String;
use std::vec;
use std::vec::Vec;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Data of several kinds: random bytes, text with repeats near and far, runs, and mixtures.
fn data(rng: &mut Rng) -> Vec<u8> {
    let len = match rng.below(4) {
        0 => rng.below(64),
        1 => rng.below(2000),
        2 => rng.below(40_000),
        _ => 32_000 + rng.below(40_000),
    };
    let mut out = Vec::with_capacity(len);
    let kind = rng.below(4);
    while out.len() < len {
        match (kind, rng.below(4)) {
            (0, _) | (3, 0) => out.push(rng.next() as u8),
            (1, _) | (3, 1) => {
                let back = 1 + rng.below(out.len().clamp(1, WINDOW_SIZE));
                if back <= out.len() && rng.below(2) == 0 {
                    let start = out.len() - back;
                    for i in 0..3 + rng.below(300) {
                        out.push(out[start + i % back]);
                    }
                } else {
                    out.extend_from_slice(b"lorem ipsum dolor sit amet ");
                }
            }
            _ => {
                let byte = rng.next() as u8;
                out.extend(std::iter::repeat_n(byte, 1 + rng.below(500)));
            }
        }
    }
    out.truncate(len);
    out
}

/// A stream to decode: a valid one, or one damaged in some way.
fn stream(rng: &mut Rng, format: Format) -> Vec<u8> {
    if rng.below(20) == 0 {
        // Random bytes: mostly header and table errors.
        return (0..rng.below(200)).map(|_| rng.next() as u8).collect();
    }
    let data = data(rng);
    let level = rng.below(11) as u8;
    let mut bytes = match format {
        Format::Deflate => compress_to_vec(&data, level),
        Format::Zlib => compress_to_vec_zlib(&data, level),
    };
    match rng.below(6) {
        0 if !bytes.is_empty() => bytes.truncate(rng.below(bytes.len())),
        1 | 2 if !bytes.is_empty() => {
            for _ in 0..=rng.below(3) {
                let at = rng.below(bytes.len());
                bytes[at] = match rng.below(3) {
                    0 => bytes[at] ^ (1 << rng.below(8)),
                    1 => rng.next() as u8,
                    _ => 0,
                };
            }
        }
        3 => bytes.extend((0..rng.below(20)).map(|_| rng.next() as u8)),
        _ => {}
    }
    bytes
}

/// Hands out the input `chunk` bytes at a time.
struct Chunked<'a> {
    data: &'a [u8],
    chunk: usize,
    buffered: usize,
}

impl Input for Chunked<'_> {
    fn fill_buf(&mut self) -> &[u8] {
        if self.buffered == 0 {
            self.buffered = self.chunk.min(self.data.len());
        }
        &self.data[..self.buffered]
    }
    fn consume(&mut self, n: usize) {
        self.data = &self.data[n..];
        self.buffered -= n;
    }
}

/// Every call of a one-shot decode with an output buffer of `size`, read `step` at a time.
fn one_shot_calls(
    bytes: &[u8],
    format: Format,
    chunk: usize,
    size: usize,
    step: usize,
    reference: bool,
) -> String {
    let mut log = String::new();
    let mut out = vec![0u8; size];
    let mut input = Chunked {
        data: bytes,
        chunk,
        buffered: 0,
    };
    let mut ours = Inflater::new(format);
    let mut theirs = RefInflater::new(format);
    let mut pos = 0;
    for _ in 0..1000 {
        let end = (pos + step).min(size);
        let buf = &mut out[..end];
        let (n, result) = if reference {
            theirs.decompress_into(&mut input, buf, pos)
        } else {
            ours.decompress_into(&mut input, buf, pos)
        };
        let _ = writeln!(log, "{n} {result:?} {:?}", &out[pos..pos + n]);
        pos += n;
        if result != Ok(Status::More) || pos == size {
            break;
        }
    }
    log
}

/// Every read of a streamed decode, `piece` bytes at a time.
fn streamed_calls(
    bytes: &[u8],
    format: Format,
    chunk: usize,
    piece: usize,
    reference: bool,
) -> String {
    let mut log = String::new();
    let mut buf: Box<[u8; WINDOW_SIZE]> = vec![0u8; WINDOW_SIZE]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let mut out = vec![0u8; piece];
    let mut input = Chunked {
        data: bytes,
        chunk,
        buffered: 0,
    };
    let mut ours = Inflater::new(format);
    let mut theirs = RefInflater::new(format);
    let mut our_window = Window::new(&mut buf);
    let mut buf2: Box<[u8; WINDOW_SIZE]> = vec![0u8; WINDOW_SIZE]
        .into_boxed_slice()
        .try_into()
        .unwrap();
    let mut their_window = RefWindow::new(&mut buf2);
    // Corrupt data can decode to a lot of output; cap the work.
    for _ in 0..300 {
        let (n, result) = if reference {
            their_window.read(&mut theirs, &mut input, &mut out)
        } else {
            our_window.read(&mut ours, &mut input, &mut out)
        };
        let _ = writeln!(log, "{n} {result:?} {:?}", &out[..n]);
        if result != Ok(Status::More) {
            break;
        }
    }
    log
}

#[test]
fn matches_the_decoder_it_replaced() {
    let cases: usize = std::env::var("INFLATE_DIFF_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3000);
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..cases {
        let format = if rng.below(2) == 0 {
            Format::Deflate
        } else {
            Format::Zlib
        };
        let bytes = stream(&mut rng, format);
        let chunk = [1, 7, 64, 4096, usize::MAX][rng.below(5)];
        let size = rng.below(80_000);
        let step = 1 + rng.below(size.max(1));
        assert_eq!(
            one_shot_calls(&bytes, format, chunk, size, step, false),
            one_shot_calls(&bytes, format, chunk, size, step, true),
            "case {case}: one-shot {format:?}, chunk {chunk}, size {size}, step {step}, stream {bytes:02x?}"
        );
        let piece = [1, 100, 777, 40_000][rng.below(4)];
        let ours = streamed_calls(&bytes, format, chunk, piece, false);
        let theirs = streamed_calls(&bytes, format, chunk, piece, true);
        if ours != theirs {
            assert_refers_before_its_start(&bytes, format, &ours, &theirs);
        }
    }
}

/// The one departure from the reference: in a ring, a distance before the start of the stream is
/// corrupt here, as it is for both decoders in a plain buffer. Our streamed reads must then give
/// what the reference gave up to there, and what a one-shot decode gives.
fn assert_refers_before_its_start(bytes: &[u8], format: Format, ours: &str, theirs: &str) {
    let output = |log: &str| -> Vec<u8> {
        log.lines()
            .flat_map(|line| {
                let list = &line[line.find('[').unwrap() + 1..line.len() - 1];
                list.split(", ")
                    .filter(|v| !v.is_empty())
                    .map(|v| v.parse::<u8>().unwrap())
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    let streamed = output(ours);
    assert!(
        ours.lines().last().unwrap().contains("Err(Corrupt)"),
        "ours {ours}\ntheirs {theirs}"
    );
    assert!(
        output(theirs).starts_with(&streamed),
        "ours {ours}\ntheirs {theirs}"
    );
    let size = streamed.len() + 1000;
    let one_shot = one_shot_calls(bytes, format, usize::MAX, size, size, false);
    assert_eq!(
        output(&one_shot),
        streamed,
        "one-shot {one_shot}\nours {ours}"
    );
    assert!(one_shot.contains("Err(Corrupt)"), "one-shot {one_shot}");
}

/// Throughput against the decoder it replaced: `cargo test --release -p bembeyaz-inflate
/// throughput -- --ignored --nocapture`.
#[test]
#[ignore = "a measurement, not a check"]
fn throughput() {
    // INFLATE_BENCH_FILE: a real text instead of the generated data.
    let text = std::env::var("INFLATE_BENCH_FILE").map_or_else(
        |_| {
            let mut rng = Rng(7);
            let mut text = Vec::new();
            while text.len() < 4 << 20 {
                text.extend_from_slice(&data(&mut rng));
            }
            text
        },
        |path| std::fs::read(path).unwrap(),
    );
    for level in [1u8, 6, 9] {
        let compressed = compress_to_vec(&text, level);
        let mut out = vec![0u8; text.len()];
        for (name, reference) in [("ours", false), ("miniz_oxide", true)] {
            let start = std::time::Instant::now();
            for _ in 0..5 {
                let mut input = compressed.as_slice();
                let (n, result) = if reference {
                    RefInflater::new(Format::Deflate).decompress_into(&mut input, &mut out, 0)
                } else {
                    Inflater::new(Format::Deflate).decompress_into(&mut input, &mut out, 0)
                };
                assert_eq!((n, result), (text.len(), Ok(Status::Done)));
            }
            let one_shot = start.elapsed() / 5;
            let start = std::time::Instant::now();
            for _ in 0..5 {
                let mut buf: Box<[u8; WINDOW_SIZE]> = vec![0u8; WINDOW_SIZE]
                    .into_boxed_slice()
                    .try_into()
                    .unwrap();
                let mut piece = [0u8; 4096];
                let mut input = Chunked {
                    data: &compressed,
                    chunk: 2048,
                    buffered: 0,
                };
                let mut total = 0;
                if reference {
                    let mut w = RefWindow::new(&mut buf);
                    let mut i = RefInflater::new(Format::Deflate);
                    while let (n, Ok(Status::More)) = w.read(&mut i, &mut input, &mut piece) {
                        total += n;
                    }
                } else {
                    let mut w = Window::new(&mut buf);
                    let mut i = Inflater::new(Format::Deflate);
                    while let (n, Ok(Status::More)) = w.read(&mut i, &mut input, &mut piece) {
                        total += n;
                    }
                }
                assert!(total > 0);
            }
            let streamed = start.elapsed() / 5;
            std::eprintln!(
                "level {level} {name:12} one-shot {:6.1} MB/s streamed {:6.1} MB/s",
                text.len() as f64 / one_shot.as_secs_f64() / 1e6,
                text.len() as f64 / streamed.as_secs_f64() / 1e6
            );
        }
    }
    std::eprintln!(
        "state: ours {} B, miniz_oxide {} B",
        size_of::<Inflater>(),
        size_of::<RefInflater>()
    );
}

/// Streams whose output ends just past a multiple of the window and whose Adler-32 trailer is
/// cut: the input runs out while the ring is full, so when bit-buffer bytes go back to the input
/// decides which read reports the truncation.
#[test]
fn truncated_trailer_past_a_window() {
    let mut rng = Rng(11);
    for len in (WINDOW_SIZE - 40..WINDOW_SIZE + 400).step_by(37) {
        let data = data(&mut rng);
        let data: Vec<u8> = data.iter().copied().cycle().take(len).collect();
        for level in [1u8, 6] {
            let full = compress_to_vec_zlib(&data, level);
            for cut in 0..=5 {
                let bytes = &full[..full.len() - cut.min(full.len())];
                for chunk in [1, usize::MAX] {
                    for piece in [1, 64, 777, len] {
                        assert_eq!(
                            streamed_calls(bytes, Format::Zlib, chunk, piece, false),
                            streamed_calls(bytes, Format::Zlib, chunk, piece, true),
                            "len {len} level {level} cut {cut} chunk {chunk} piece {piece}"
                        );
                    }
                    for (size, step) in [(len, len), (len, 100), (len + 5, 7)] {
                        assert_eq!(
                            one_shot_calls(bytes, Format::Zlib, chunk, size, step, false),
                            one_shot_calls(bytes, Format::Zlib, chunk, size, step, true),
                            "len {len} level {level} cut {cut} chunk {chunk} size {size} step {step}"
                        );
                    }
                }
            }
        }
    }
}

/// An all-zero literal/length code at the very end of the input. The old decoder took it and then
/// reported a truncation; it has no end-of-block code, so it is corrupt now, as zlib has it.
#[test]
fn empty_code_at_the_end_of_the_input() {
    let mut bits = 0u64;
    let mut n = 0;
    let mut put = |value: u64, width: u32| {
        bits |= value << n;
        n += width;
    };
    put(1, 1); // BFINAL
    put(2, 2); // dynamic
    put(0, 5); // HLIT 257
    put(0, 5); // HDIST 1
    put(0, 4); // HCLEN 4: lengths of 16, 17, 18, 0
    for len in [0, 0, 1, 1] {
        put(len, 3);
    }
    // Code-length code: 0 is '0', 18 is '1'. 138 + 117 + 3 zero lengths = 257 + 1.
    put(1, 1);
    put(127, 7);
    put(1, 1);
    put(106, 7);
    put(0, 3);
    assert_eq!(n, 48);
    let stream = bits.to_le_bytes()[..6].to_vec();
    for chunk in [1, usize::MAX] {
        for piece in [1, 300] {
            assert_eq!(
                streamed_calls(&stream, Format::Deflate, chunk, piece, false),
                "0 Err(Corrupt) []\n",
                "chunk {chunk} piece {piece}"
            );
            assert_eq!(
                streamed_calls(&stream, Format::Deflate, chunk, piece, true),
                "0 Err(Truncated) []\n",
                "chunk {chunk} piece {piece}"
            );
        }
        assert_eq!(
            one_shot_calls(&stream, Format::Deflate, chunk, 300, 300, false),
            "0 Err(Corrupt) []\n"
        );
    }
}

/// Literal-heavy streams cut in their last bytes, with the output reaching the end of the window
/// (or of a one-shot buffer) right before the last literal: that literal is read when the output
/// is full, and which read reports the cut depends on whether its bits were taken then.
#[test]
fn literal_after_a_full_output() {
    let mut rng = Rng(23);
    for extra in 1..=8 {
        let len = WINDOW_SIZE + extra;
        let data: Vec<u8> = (0..len).map(|_| (rng.next() % 40) as u8 + b'0').collect();
        for level in [1u8, 6] {
            let full = compress_to_vec(&data, level);
            for cut in 1..=12 {
                let bytes = &full[..full.len() - cut];
                for chunk in [1, usize::MAX] {
                    for piece in [WINDOW_SIZE + 1, len] {
                        assert_eq!(
                            streamed_calls(bytes, Format::Deflate, chunk, piece, false),
                            streamed_calls(bytes, Format::Deflate, chunk, piece, true),
                            "len {len} level {level} cut {cut} chunk {chunk} piece {piece}"
                        );
                    }
                    assert_eq!(
                        one_shot_calls(bytes, Format::Deflate, chunk, len, WINDOW_SIZE, false),
                        one_shot_calls(bytes, Format::Deflate, chunk, len, WINDOW_SIZE, true),
                        "len {len} level {level} cut {cut} chunk {chunk} one-shot"
                    );
                }
            }
        }
    }
}
