extern crate std;

use super::*;
use std::boxed::Box;
use std::vec;
use std::vec::Vec;

struct Mem<'a>(&'a [u8]);

impl Source for Mem<'_> {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> usize {
        let rest = self.0.get(pos as usize..).unwrap_or_default();
        let n = rest.len().min(buf.len());
        buf[..n].copy_from_slice(&rest[..n]);
        n
    }
}

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

macro_rules! corpus {
    ($name:literal) => {
        include_bytes!(concat!("../testdata/corpus/", $name))
    };
}

fn new_jpeg() -> Box<MaybeUninit<Jpeg>> {
    Box::new_uninit()
}

struct Decoded {
    opened: Result<(), Error>,
    width: i32,
    height: i32,
    /// (x, y, width, height, width used) of each block, and the pixels in use.
    blocks: Vec<(i32, i32, i32, i32, i32)>,
    pixels: Vec<u8>,
    result: Option<Result<(), Error>>,
}

/// Opens `file` and, when the converters would decode it, decodes it at `size`.
fn decode(file: &[u8], size: Scale) -> Decoded {
    let mut slot = new_jpeg();
    let jpeg = Jpeg::init(&mut slot);
    let opened = jpeg.open(&mut Mem(file), i32::try_from(file.len()).unwrap());
    let mut decoded = Decoded {
        opened,
        width: jpeg.width(),
        height: jpeg.height(),
        blocks: Vec::new(),
        pixels: Vec::new(),
        result: None,
    };
    let (w, h) = (i64::from(jpeg.width()), i64::from(jpeg.height()));
    if opened.is_err() || w <= 0 || h <= 0 || w * h > 8_388_608 {
        return decoded;
    }
    let (mut blocks, mut pixels) = (Vec::new(), Vec::new());
    let result = jpeg.decode(&mut Mem(file), size, |block| {
        blocks.push((
            block.x,
            block.y,
            block.width,
            block.height,
            block.width_used,
        ));
        for row in 0..block.height {
            for x in 0..block.width_used {
                pixels.push(block.pixels[(row * block.width + x) as usize]);
            }
        }
        true
    });
    decoded.blocks = blocks;
    decoded.pixels = pixels;
    decoded.result = Some(result);
    decoded
}

/// The blocks tile the (scaled) image once, row by row.
fn assert_tiles(d: &Decoded, shift: i32) {
    let (w, h) = (
        (d.width + (1 << shift) - 1) >> shift,
        (d.height + (1 << shift) - 1) >> shift,
    );
    let covered: i64 = d
        .blocks
        .iter()
        .map(|&(_, _, _, height, used)| i64::from(height) * i64::from(used))
        .sum();
    assert_eq!(covered, i64::from(w) * i64::from(h));
}

#[test]
fn decodes_every_layout_at_every_scale() {
    let files: [&[u8]; 8] = [
        corpus!("gray_100x60.jpg"),
        corpus!("rgb444_100x60.jpg"),
        corpus!("rgb422_100x60.jpg"),
        corpus!("rgb420_100x60.jpg"),
        corpus!("rgb420_319x201.jpg"),
        corpus!("gray_restart3.jpg"),
        corpus!("rgb420_optimized.jpg"),
        corpus!("wide_rgb422.jpg"),
    ];
    for file in files {
        for (size, shift) in [
            (Scale::Full, 0),
            (Scale::Half, 1),
            (Scale::Quarter, 2),
            (Scale::Eighth, 3),
        ] {
            let d = decode(file, size);
            assert_eq!(d.opened, Ok(()));
            assert_eq!(d.result, Some(Ok(())), "{size:?}");
            assert_tiles(&d, shift);
        }
    }
}

/// A progressive image asked for full size decodes its first scan at 1/8 size.
#[test]
fn progressive_is_an_eighth() {
    let file = corpus!("prog_rgb420_319x201.jpg");
    let full = decode(file, Scale::Full);
    let eighth = decode(file, Scale::Eighth);
    assert_eq!(full.result, Some(Ok(())));
    assert_eq!(full.blocks, eighth.blocks);
    assert_eq!(full.pixels, eighth.pixels);
    assert_tiles(&full, 3);
}

/// A mid-gray image comes out mid-gray, smooth gradients smooth: the IDCT and the clamp hold.
#[test]
fn pixels_follow_the_picture() {
    // gray_100x60 is a left-to-right gradient plus shapes: the left edge is darker than the right.
    let d = decode(corpus!("gray_100x60.jpg"), Scale::Full);
    let row: Vec<u8> = d.pixels[..100].to_vec();
    let left: u32 = row[..10].iter().map(|&p| u32::from(p)).sum();
    let right: u32 = row[90..].iter().map(|&p| u32::from(p)).sum();
    assert!(left < right, "{row:?}");
}

#[test]
fn stopping_early_still_succeeds() {
    let file = corpus!("large_gray.jpg");
    let mut slot = new_jpeg();
    let jpeg = Jpeg::init(&mut slot);
    assert_eq!(jpeg.open(&mut Mem(file), file.len() as i32), Ok(()));
    let mut calls = 0;
    let result = jpeg.decode(&mut Mem(file), Scale::Full, |_| {
        calls += 1;
        false
    });
    assert_eq!(result, Ok(()));
    assert_eq!(calls, 1);
}

#[test]
fn decodes_once_after_an_open() {
    let file = corpus!("gray_100x60.jpg");
    let mut slot = new_jpeg();
    let jpeg = Jpeg::init(&mut slot);
    let draw = |_: &Block<'_>| true;
    assert_eq!(
        jpeg.decode(&mut Mem(file), Scale::Full, draw),
        Err(Error::NotOpen)
    );
    assert_eq!(
        jpeg.open(&mut Mem(&file[..100]), 100),
        Err(Error::InvalidFile)
    );
    assert_eq!(
        jpeg.decode(&mut Mem(file), Scale::Full, draw),
        Err(Error::NotOpen)
    );
    assert_eq!(jpeg.open(&mut Mem(file), file.len() as i32), Ok(()));
    assert_eq!(jpeg.decode(&mut Mem(file), Scale::Full, draw), Ok(()));
    assert_eq!(
        jpeg.decode(&mut Mem(file), Scale::Full, draw),
        Err(Error::NotOpen)
    );
}

#[test]
fn refuses_what_jpegdec_refused() {
    let opened = |file: &[u8]| decode(file, Scale::Full).opened;
    assert_eq!(opened(&[]), Err(Error::InvalidFile));
    assert_eq!(opened(corpus!("bad_soi.jpg")), Err(Error::InvalidFile));
    // Under 256 bytes JPEGDEC took a file for broken.
    assert_eq!(
        opened(&corpus!("gray_100x60.jpg")[..255]),
        Err(Error::InvalidFile)
    );
    assert_eq!(
        opened(corpus!("lossless_sof3.jpg")),
        Err(Error::Unsupported)
    );
    assert_eq!(
        opened(corpus!("dht_counts_overflow.jpg")),
        Err(Error::Decode)
    );
    let d = decode(corpus!("scan_all_ff.jpg"), Scale::Full);
    assert_eq!(d.result, Some(Err(Error::Decode)));
}

/// The departures from JPEGDEC's undefined behaviour: a sampling factor without an MCU layout is
/// an error (JPEGDEC divided by zero), and a frame of five components keeps four.
#[test]
fn survives_what_jpegdec_crashed_on() {
    let good = corpus!("rgb420_100x60.jpg");
    let sof = good.windows(2).position(|w| w == [0xff, 0xc0]).unwrap();
    let mut bad = good.to_vec();
    bad[sof + 11] = 0x31; // the luma sampling factor
    let d = decode(&bad, Scale::Full);
    assert_eq!(d.result, Some(Err(Error::Unsupported)));
    let mut bad = good.to_vec();
    bad[sof + 9] = 5; // five components
    let _ = decode(&bad, Scale::Full);
}

/// A progressive file whose only Huffman table is DC table 2, which lies where JPEGDEC kept the
/// AC tables: its scan (coefficients 1 to 63) looks AC codes up in DC table 2's entries, whose
/// length bytes read as 33 or 49 bits, past the 32-bit bit buffer (JPEGDEC's shift amount wrapped
/// there; no panic in a debug build).
fn progressive_with_dc_table_2(bits: usize, approx: u8) -> Vec<u8> {
    let mut f = vec![0xff, 0xd8];
    f.extend([0xff, 0xdb, 0, 67, 0]);
    f.extend([1; 64]);
    f.extend([0xff, 0xc2, 0, 11, 8, 0, 16, 0, 16, 1, 1, 0x11, 0]);
    let mut counts = [0u8; 16];
    counts[bits - 1] = 2;
    f.extend([0xff, 0xc4, 0, 21, 0x02]);
    f.extend(counts);
    f.extend([1, 1]);
    // The scan: component 1 with DC table 2, coefficients 1 to 63.
    f.extend([0xff, 0xda, 0, 8, 1, 1, 0x20, 1, 63, approx]);
    f.resize(400, 0);
    f.extend([0xff, 0xd9]);
    f
}

#[test]
fn survives_a_progressive_code_past_the_bit_buffer() {
    // A refinement scan: one more bit after a 33-bit code.
    let d = decode(&progressive_with_dc_table_2(2, 0x10), Scale::Full);
    assert_eq!(d.opened, Ok(()));
    assert!(d.result.is_some());
    // A first scan: the next code after a 49-bit one and its extra bit.
    let d = decode(&progressive_with_dc_table_2(3, 0x00), Scale::Full);
    assert_eq!(d.opened, Ok(()));
    assert!(d.result.is_some());
}

/// A source that fails the test after 1000 reads: an open that loops reads without end.
struct Limited<'a>(&'a [u8], u32);

impl Source for Limited<'_> {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> usize {
        self.1 += 1;
        assert!(self.1 < 1000, "the open loops");
        Mem(self.0).read_at(pos, buf)
    }
}

fn open_limited(file: &[u8]) -> Result<(), Error> {
    let mut slot = new_jpeg();
    let jpeg = Jpeg::init(&mut slot);
    jpeg.open(&mut Limited(file, 0), i32::try_from(file.len()).unwrap())
}

/// A segment longer than the read buffer is skipped with a seek.
#[test]
fn skips_a_long_segment() {
    let file = corpus!("gray_100x60.jpg");
    let mut long = vec![0xff, 0xd8, 0xff, 0xe2, 0xff, 0xff];
    long.resize(2 + 0x10001, 0x11);
    long.extend(&file[2..]);
    assert_eq!(open_limited(&long), Ok(()));
    let (a, b) = (decode(file, Scale::Full), decode(&long, Scale::Full));
    assert_eq!(b.result, Some(Ok(())));
    assert_eq!((a.blocks, a.pixels), (b.blocks, b.pixels));
}

/// After a segment that runs past the end of the file the seek fails and the parse reads on from
/// the end of its buffer, as JPEGDEC's did: here through the zeros to the image's own headers.
#[test]
fn reads_on_after_a_segment_past_the_end() {
    let file = corpus!("gray_100x60.jpg");
    let mut f = vec![0xff, 0xd8, 0xff, 0xe2, 0xff, 0xff];
    f.resize(2 + 0x10001, 0);
    f.extend([0xff, 0xe1, 0xff, 0xff]);
    // A gap at which JPEGDEC's resync (three bytes a step over non-markers) meets the next marker.
    f.resize(f.len() + 3001, 0);
    f.extend(&file[2..]);
    assert_eq!(open_limited(&f), Ok(()));
    let (a, b) = (decode(file, Scale::Full), decode(&f, Scale::Full));
    assert_eq!(b.result, Some(Ok(())));
    assert_eq!((a.blocks, a.pixels), (b.blocks, b.pixels));
}

/// Once nothing more can be read after a failed seek, the open fails. JPEGDEC parsed its stale
/// buffer again, its file position running round its `int` until a seek landed in the file, here
/// on a scan header.
#[test]
fn a_segment_past_the_end_fails() {
    let mut f = vec![0xff, 0xd8, 0xff, 0xfe, 0xff, 0xff];
    while f.len() < 65539 {
        f.extend([0xff, 0xda]);
    }
    f.truncate(65539);
    f.extend([0xff, 0xe0, 0xff, 0xff]);
    assert_eq!(open_limited(&f), Err(Error::Decode));
}

/// A comment jumps to an APP0 segment within 2048 bytes of the end, 65536 bytes long with its
/// marker: past the end. Every later jump read the same stale segment, so JPEGDEC's position ran
/// round 2^32 back to the APP0 segment, the one point of its class in the file, and again without
/// end.
#[test]
fn a_segment_looping_past_the_end_fails() {
    let (size, app0) = (66_000, 65_000);
    let mut f = vec![0xff, 0xd8, 0xff, 0xfe];
    f.extend(u16::try_from(app0 - 4).unwrap().to_be_bytes());
    f.resize(app0, 0);
    f.extend([0xff, 0xe0, 0xff, 0xfe]);
    f.resize(size, 0);
    assert_eq!(open_limited(&f), Err(Error::Decode));
}

/// Damage anywhere ends in a result, never a panic or a hang, with blocks inside their buffer.
#[test]
fn survives_random_damage() {
    let seeds: [&[u8]; 6] = [
        corpus!("gray_100x60.jpg"),
        corpus!("rgb420_100x60.jpg"),
        corpus!("rgb422_33x17.jpg"),
        corpus!("prog_rgb420_100x60.jpg"),
        corpus!("rgb420_restart3.jpg"),
        corpus!("cmyk_64x40.jpg"),
    ];
    let mut rng = Rng(7);
    for seed in seeds {
        for round in 0..400 {
            let mut bytes = seed.to_vec();
            for _ in 0..=rng.below(4) {
                let at = rng.below(bytes.len());
                match rng.below(3) {
                    0 => bytes[at] ^= 1 << rng.below(8),
                    1 => bytes[at] = rng.next() as u8,
                    _ => bytes.truncate(at.max(1)),
                }
            }
            let size = [Scale::Full, Scale::Half, Scale::Quarter, Scale::Eighth][round % 4];
            let d = decode(&bytes, size);
            for &(_, _, width, height, used) in &d.blocks {
                assert!(
                    width * height <= PIXELS_SIZE as i32 || used <= 0,
                    "{width}x{height}"
                );
            }
        }
    }
}

#[test]
fn the_state_is_about_18_kb() {
    let size = core::mem::size_of::<Jpeg>();
    assert!((17_000..19_500).contains(&size), "{size}");
    let _ = vec![0u8; 1];
}
