extern crate std;

use super::*;
use miniz_oxide::deflate::{compress_to_vec, compress_to_vec_zlib};
use std::boxed::Box;
use std::vec;
use std::vec::Vec;

/// Text with repeats at every distance up to the full window, so the decoder
/// reaches back across window wraps.
fn sample(len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut seed = 0x1234_5678_u32;
    while out.len() < len {
        seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let back = (seed >> 8) as usize % WINDOW_SIZE.min(out.len() + 1);
        if back > 3 && seed & 1 == 0 {
            let start = out.len() - back;
            for i in 0..(seed >> 24) as usize % 200 {
                out.push(out[start + i % back]);
            }
        } else {
            out.extend_from_slice(b"the quick brown fox ");
            out.push(b'a' + ((seed >> 16) % 26) as u8);
        }
    }
    out.truncate(len);
    out
}

/// Hands out the input `chunk` bytes at a time, like a file read loop.
struct Chunked<'a> {
    data: &'a [u8],
    chunk: usize,
    buffered: usize,
}

impl<'a> Chunked<'a> {
    fn new(data: &'a [u8], chunk: usize) -> Self {
        Self {
            data,
            chunk,
            buffered: 0,
        }
    }
}

impl Input for Chunked<'_> {
    fn fill_buf(&mut self) -> &[u8] {
        if self.buffered == 0 {
            self.buffered = self.chunk.min(self.data.len());
        }
        &self.data[..self.buffered]
    }

    fn consume(&mut self, n: usize) {
        assert!(n <= self.buffered);
        self.data = &self.data[n..];
        self.buffered -= n;
    }
}

fn window() -> Box<[u8; WINDOW_SIZE]> {
    vec![0xA5; WINDOW_SIZE]
        .into_boxed_slice()
        .try_into()
        .unwrap()
}

fn one_shot(
    compressed: &[u8],
    format: Format,
    in_chunk: usize,
    out_len: usize,
) -> Result<Vec<u8>, Error> {
    let mut inflater = Inflater::new(format);
    let mut out = vec![0; out_len];
    let mut input = Chunked::new(compressed, in_chunk);
    let (n, result) = inflater.decompress_into(&mut input, &mut out, 0);
    result?;
    out.truncate(n);
    Ok(out)
}

fn streamed(
    compressed: &[u8],
    format: Format,
    in_chunk: usize,
    out_chunk: usize,
) -> Result<Vec<u8>, Error> {
    let mut buf = window();
    let mut window = Window::new(&mut buf);
    let mut inflater = Inflater::new(format);
    let mut input = Chunked::new(compressed, in_chunk);
    let mut out = Vec::new();
    let mut piece = vec![0; out_chunk];
    loop {
        let (n, result) = window.read(&mut inflater, &mut input, &mut piece);
        out.extend_from_slice(&piece[..n]);
        if result? == Status::Done {
            return Ok(out);
        }
        assert_eq!(n, out_chunk, "More means the output is full");
    }
}

/// Run under Miri (`cargo +nightly miri test -p bembeyaz-inflate init_`) to
/// check that the zeroed state `init` writes over garbage is a valid decoder.
#[test]
fn init_over_garbage_decodes() {
    let data = sample(300);
    for (format, compressed) in [
        (Format::Deflate, compress_to_vec(&data, 6)),
        (Format::Zlib, compress_to_vec_zlib(&data, 6)),
    ] {
        // Garbage first, as in a reused framebuffer.
        let mut slot = Box::new(MaybeUninit::<Inflater>::uninit());
        unsafe {
            slot.as_mut_ptr()
                .cast::<u8>()
                .write_bytes(0xA5, size_of::<Inflater>());
        };
        let inflater = Inflater::init(&mut slot, format);
        let mut out = vec![0; data.len() + 1];
        let mut input = compressed.as_slice();
        assert_eq!(
            inflater.decompress_into(&mut input, &mut out, 0),
            (data.len(), Ok(Status::Done))
        );
        assert_eq!(out[..data.len()], data);
    }
}

#[test]
fn round_trips_through_both_modes() {
    for len in [0, 1, 300, 40_000, 200_000] {
        let data = sample(len);
        for (format, compressed) in [
            (Format::Deflate, compress_to_vec(&data, 6)),
            (Format::Deflate, compress_to_vec(&data, 0)),
            (Format::Zlib, compress_to_vec_zlib(&data, 9)),
        ] {
            for in_chunk in [1, 13, 1024, usize::MAX] {
                assert_eq!(
                    one_shot(&compressed, format, in_chunk, len).unwrap(),
                    data,
                    "one-shot {len}"
                );
                for out_chunk in [1, 127, WINDOW_SIZE + 1, 100_000] {
                    if in_chunk == 1 && out_chunk == 1 && len > 40_000 {
                        continue; // slow, and covered by the smaller sizes
                    }
                    assert_eq!(
                        streamed(&compressed, format, in_chunk, out_chunk).unwrap(),
                        data,
                        "streamed {len} in {in_chunk} out {out_chunk}"
                    );
                }
            }
        }
    }
}

#[test]
fn one_shot_resumes_in_the_same_buffer() {
    let data = sample(50_000);
    let compressed = compress_to_vec(&data, 6);
    let mut inflater = Inflater::new(Format::Deflate);
    let mut out = vec![0; data.len()];
    let mut input = compressed.as_slice();
    let mut pos = 0;
    for step in [1, 999, 20_000, 28_999, 1] {
        let (n, result) = inflater.decompress_into(&mut input, &mut out[..pos + step], pos);
        let status = result.unwrap();
        assert_eq!(n, step);
        pos += n;
        assert_eq!(
            status,
            if pos == data.len() {
                Status::Done
            } else {
                Status::More
            }
        );
    }
    assert_eq!(out, data);
}

#[test]
fn one_shot_stops_when_the_buffer_is_full() {
    let data = sample(5000);
    let compressed = compress_to_vec(&data, 6);
    // An entry whose real size exceeds the size in its header.
    assert_eq!(
        one_shot(&compressed, Format::Deflate, 64, 4000).unwrap(),
        data[..4000]
    );
    // And a buffer larger than the output.
    assert_eq!(
        one_shot(&compressed, Format::Deflate, 64, 6000).unwrap(),
        data
    );
}

#[test]
fn rejects_a_position_past_the_buffer() {
    let mut inflater = Inflater::new(Format::Deflate);
    let mut out = [0; 4];
    assert_eq!(
        inflater.decompress_into(&mut &[][..], &mut out, 5),
        (0, Err(Error::BadOutput))
    );
}

#[test]
fn reports_truncation() {
    let data = sample(10_000);
    let compressed = compress_to_vec(&data, 6);
    for len in [0, 1, compressed.len() / 2, compressed.len() - 1] {
        let truncated = &compressed[..len];
        // One byte of room, so the decoder has to look for the end of the stream.
        assert_eq!(
            one_shot(truncated, Format::Deflate, 100, data.len() + 1),
            Err(Error::Truncated),
            "{len}"
        );
        assert_eq!(
            streamed(truncated, Format::Deflate, 100, 512),
            Err(Error::Truncated),
            "{len}"
        );
    }
    // A zlib stream without its Adler-32 trailer.
    let zlib = compress_to_vec_zlib(&data, 6);
    let truncated = &zlib[..zlib.len() - 2];
    assert_eq!(
        streamed(truncated, Format::Zlib, 100, 512),
        Err(Error::Truncated)
    );
}

#[test]
fn checks_the_zlib_checksum() {
    let data = sample(3000);
    let mut zlib = compress_to_vec_zlib(&data, 6);
    *zlib.last_mut().unwrap() ^= 1;
    assert_eq!(
        streamed(&zlib, Format::Zlib, 100, 512),
        Err(Error::Checksum)
    );
    assert_eq!(
        one_shot(&zlib, Format::Zlib, 100, data.len()),
        Err(Error::Checksum)
    );
    // Raw DEFLATE has no checksum, and a zlib header is not raw DEFLATE.
    assert_eq!(
        streamed(&compress_to_vec(&data, 6), Format::Zlib, 100, 512),
        Err(Error::Corrupt)
    );
}

#[test]
fn ignores_data_after_the_stream() {
    let data = sample(3000);
    let mut compressed = compress_to_vec(&data, 6);
    compressed.extend_from_slice(b"trailing bytes");
    assert_eq!(
        streamed(&compressed, Format::Deflate, 7, 512).unwrap(),
        data
    );
    assert_eq!(
        one_shot(&compressed, Format::Deflate, 7, data.len()).unwrap(),
        data
    );
}

#[test]
fn keeps_reporting_done() {
    let data = sample(100);
    let compressed = compress_to_vec(&data, 6);
    let mut buf = window();
    let mut window = Window::new(&mut buf);
    let mut inflater = Inflater::new(Format::Deflate);
    let mut input = compressed.as_slice();
    let mut out = [0; 100];
    assert_eq!(
        window.read(&mut inflater, &mut input, &mut out),
        (100, Ok(Status::Done))
    );
    assert_eq!(
        window.read(&mut inflater, &mut input, &mut out),
        (0, Ok(Status::Done))
    );
}

#[test]
fn reports_done_once_pending_output_is_taken() {
    let data = sample(100);
    let compressed = compress_to_vec(&data, 6);
    let mut buf = window();
    let mut window = Window::new(&mut buf);
    let mut inflater = Inflater::new(Format::Deflate);
    let mut input = compressed.as_slice();
    let mut out = [0; 60];
    assert_eq!(
        window.read(&mut inflater, &mut input, &mut out),
        (60, Ok(Status::More))
    );
    assert_eq!(
        window.read(&mut inflater, &mut input, &mut out),
        (40, Ok(Status::Done))
    );
    assert_eq!(out[..40], data[60..]);
}

#[test]
fn survives_truncation_and_corruption() {
    fn exercise(compressed: &[u8], data: &[u8]) {
        for format in [Format::Deflate, Format::Zlib] {
            // Whatever comes out before an error is still bounded by the buffer.
            if let Ok(out) = one_shot(compressed, format, 17, data.len()) {
                assert!(out.len() <= data.len());
            }
            let mut buf = window();
            let mut window = Window::new(&mut buf);
            let mut inflater = Inflater::new(format);
            let mut input = Chunked::new(compressed, 17);
            let mut piece = [0; 777];
            // Corrupt data can still decode to a lot of output; cap the work.
            for _ in 0..200 {
                let (n, result) = window.read(&mut inflater, &mut input, &mut piece);
                assert!(n <= piece.len());
                if result != Ok(Status::More) {
                    break;
                }
            }
        }
    }

    let data = sample(4000);
    for compressed in [compress_to_vec(&data, 6), compress_to_vec_zlib(&data, 1)] {
        for len in 0..compressed.len() {
            exercise(&compressed[..len], &data);
        }
        let mut bytes = compressed.clone();
        for i in 0..bytes.len() {
            let original = bytes[i];
            for value in [0x00, 0xFF, original ^ 0x01, original ^ 0x80] {
                bytes[i] = value;
                exercise(&bytes, &data);
            }
            bytes[i] = original;
        }
    }
}

/// Also run under Miri: the output buffer is never read before it is written.
#[test]
fn reads_into_uninitialised_memory() {
    let data = sample(2000);
    let compressed = compress_to_vec(&data, 6);
    let mut buf = window();
    let mut window = Window::new(&mut buf);
    let mut inflater = Inflater::new(Format::Deflate);
    let mut input = compressed.as_slice();
    let mut out = vec![MaybeUninit::<u8>::uninit(); 1500];
    assert_eq!(
        window.read_uninit(&mut inflater, &mut input, &mut out),
        (1500, Ok(Status::More))
    );
    let out: Vec<u8> = out.iter().map(|b| unsafe { b.assume_init() }).collect();
    assert_eq!(out, data[..1500]);
}

/// Output decoded before an error still reaches the caller, so a reader that
/// only needs the start of a damaged entry (an image header) gets it.
#[test]
fn hands_out_the_output_before_an_error() {
    let data = sample(100_000);
    let compressed = compress_to_vec(&data, 6);
    let truncated = &compressed[..compressed.len() / 2];

    let mut inflater = Inflater::new(Format::Deflate);
    let mut out = vec![0; data.len()];
    let (n, result) = inflater.decompress_into(&mut &truncated[..], &mut out, 0);
    assert_eq!(result, Err(Error::Truncated));
    assert!(n > WINDOW_SIZE, "{n}");
    assert_eq!(out[..n], data[..n]);

    // Streaming: every read before the failing one is full, and the failing
    // one still counts what it copied, up to everything decoded.
    for piece_len in [1024, 50_000] {
        let mut buf = window();
        let mut window = Window::new(&mut buf);
        let mut inflater = Inflater::new(Format::Deflate);
        let mut input = Chunked::new(truncated, 100);
        let mut piece = vec![0; piece_len];
        let mut total = 0;
        loop {
            let (n, result) = window.read(&mut inflater, &mut input, &mut piece);
            assert_eq!(piece[..n], data[total..total + n]);
            total += n;
            match result {
                Ok(status) => assert_eq!(status, Status::More),
                Err(error) => {
                    assert_eq!(error, Error::Truncated);
                    break;
                }
            }
        }
        assert_eq!(total, n, "streamed in {piece_len}");
    }
}

/// The decoder runs ahead of the reader. A read that ends before the damage
/// succeeds; the error comes with the read that reaches it, and stays.
#[test]
fn an_error_waits_for_the_output_before_it() {
    // A stored block that is not the last, then a block of the reserved type
    // 11: the step that decodes the good bytes also meets the damage.
    let data = sample(2000);
    let len = u16::try_from(data.len()).unwrap();
    let mut stream = vec![0x00];
    stream.extend_from_slice(&len.to_le_bytes());
    stream.extend_from_slice(&(!len).to_le_bytes());
    stream.extend_from_slice(&data);
    stream.push(0x07);

    let mut input = stream.as_slice();
    let mut buf = window();
    let mut window = Window::new(&mut buf);
    let mut inflater = Inflater::new(Format::Deflate);
    let mut out = [0; 100];
    assert_eq!(
        window.read(&mut inflater, &mut input, &mut out),
        (100, Ok(Status::More))
    );
    assert_eq!(out[..], data[..100]);

    let mut total = 100;
    loop {
        let (n, result) = window.read(&mut inflater, &mut input, &mut out);
        assert_eq!(out[..n], data[total..total + n]);
        total += n;
        if let Err(error) = result {
            assert_eq!(error, Error::Corrupt);
            break;
        }
    }
    assert_eq!(total, data.len());
    assert_eq!(
        window.read(&mut inflater, &mut input, &mut out),
        (0, Err(Error::Corrupt))
    );
}

/// DEFLATE bits, least significant first; Huffman codes most significant first.
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, value: u32, n: u32) {
        self.acc |= value << self.n;
        self.n += n;
        while self.n >= 8 {
            self.out.push(self.acc.to_le_bytes()[0]);
            self.acc >>= 8;
            self.n -= 8;
        }
    }

    fn code(&mut self, code: u32, n: u32) {
        self.put(code.reverse_bits() >> (32 - n), n);
    }

    fn finish(mut self) -> Vec<u8> {
        if self.n > 0 {
            self.out.push(self.acc.to_le_bytes()[0]);
        }
        self.out
    }
}

/// A fixed-code block: the literal 'a', then 3 bytes `distance` back.
fn match_after_one_byte(distance_code: u32) -> Vec<u8> {
    let mut bits = Bits::default();
    bits.put(1, 1); // last block
    bits.put(1, 2); // fixed codes
    bits.code(0x30 + u32::from(b'a'), 8);
    bits.code(1, 7); // length 3
    bits.code(distance_code, 5); // no extra bits for codes 0-3
    bits.code(0, 7); // end of block
    bits.finish()
}

/// A distance before the start of the stream is corrupt in a ring as in a plain buffer, as zlib
/// has it, instead of copying what the ring held before.
#[test]
fn refuses_a_distance_before_the_start() {
    let good = match_after_one_byte(0); // distance 1
    assert_eq!(
        one_shot(&good, Format::Deflate, 1, 10),
        Ok(b"aaaa".to_vec())
    );
    assert_eq!(streamed(&good, Format::Deflate, 1, 3), Ok(b"aaaa".to_vec()));

    let bad = match_after_one_byte(1); // distance 2
    assert_eq!(one_shot(&bad, Format::Deflate, 1, 10), Err(Error::Corrupt));
    for in_chunk in [1, bad.len()] {
        let mut buf = window();
        let mut window = Window::new(&mut buf);
        let mut inflater = Inflater::new(Format::Deflate);
        let mut input = Chunked::new(&bad, in_chunk);
        let mut out = [0; 10];
        assert_eq!(
            window.read(&mut inflater, &mut input, &mut out),
            (1, Err(Error::Corrupt))
        );
        assert_eq!(out[0], b'a');
    }

    // Past the first window the whole ring is history.
    let data = sample(WINDOW_SIZE + 1000);
    let compressed = compress_to_vec(&data, 6);
    assert_eq!(streamed(&compressed, Format::Deflate, 4096, 1000), Ok(data));
}
