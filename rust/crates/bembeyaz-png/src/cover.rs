//! The PNG reader for covers: our own, reading as `PngToBmpConverter`'s own decoder did, which
//! `test/png_to_bmp` pins. Rows come out one at a time as 8-bit gray, for the converter to scale,
//! dither and write as a BMP.
//!
//! It makes the same reads and seeks, in the same order, as that decoder made on `HalFile` (where
//! a seek past the end fails and leaves the position), checks no CRC, walks the chunks to the
//! first IDAT taking PLTE on the way, and decodes the image data as one zlib stream across the
//! IDAT chunks, skipping any other chunk between them. Unlike the chapter decoder it takes 16-bit
//! samples (their high bytes), checks the Adler-32 sum when the read of the last row reaches it,
//! stops at a filter type it does not know, reads an index past the palette as entry 0 and
//! ignores alpha and tRNS. Gray is 25% red, 50% green and 25% blue.

use super::{IDAT, IHDR, PLTE, Source, Workspace, pixel, unfilter_into, zeroed_box};
use alloc::vec::Vec;
use bembeyaz_inflate::{Format, Inflater, Input, Window};

const SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const IEND: u32 = u32::from_be_bytes(*b"IEND");
/// The converter's limits.
const MAX_WIDTH: u32 = 2048;
const MAX_HEIGHT: u32 = 3072;
const MAX_ROW_BYTES: u32 = 16384;
/// The most image data handed to the decoder at a time.
const READ_SIZE: usize = 2048;

/// Why `open` or `decode` failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `decode` without a successful `open`, or a second time.
    NotOpen,
    /// The file ended inside the header.
    HeaderEnded,
    /// No PNG signature.
    NotPng,
    /// The first chunk is not IHDR.
    NoHeader,
    /// Compression or filter method other than 0, interlacing, an unknown color type, or a bit
    /// depth PNG does not allow for the color type.
    Unsupported,
    /// No pixels, wider than 2048 or taller than 3072, or rows of more than 16384 bytes.
    BadSize,
    /// No IDAT chunk before IEND or the end of the file.
    NoImageData,
    /// Out of memory.
    Memory,
    /// The image data ended or is corrupt (a wrong Adler-32 sum included, when the read of the
    /// last row reaches it); the row being read is not drawn.
    Corrupt,
    /// A row with a filter type other than 0 to 4.
    UnknownFilter,
    /// The row callback asked to stop.
    QuitEarly,
}

/// The file as the converter read it: reads move the position on, and a seek moves it only to a
/// place inside the file.
struct File {
    pos: u32,
    size: u32,
}

impl File {
    fn read(&mut self, source: &mut impl Source, buf: &mut [u8]) -> usize {
        let n = source.read_at(self.pos, buf).min(buf.len());
        self.pos = self
            .pos
            .saturating_add(u32::try_from(n).unwrap_or(u32::MAX));
        n
    }

    fn read_be32(&mut self, source: &mut impl Source) -> Option<u32> {
        let mut word = [0; 4];
        (self.read(source, &mut word) == 4).then_some(u32::from_be_bytes(word))
    }

    fn seek_cur(&mut self, offset: u32) -> bool {
        let to = u64::from(self.pos) + u64::from(offset);
        match u32::try_from(to) {
            Ok(to) if to <= self.size => {
                self.pos = to;
                true
            }
            _ => false,
        }
    }
}

/// A cover being read. About 2.3 KB: keep it off the stack ([`Cover::init`]).
pub struct Cover {
    /// While `open` walks the chunks, 256 RGB triples from PLTE; then the image data being read.
    buf: [u8; READ_SIZE],
    /// The gray of each palette index; an index past the palette gets entry 0's gray.
    palette_gray: [u8; 256],
    pos: u32,
    size: u32,
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    /// Bytes apart that a pixel's filter looks back.
    step: u8,
    row_bytes: u32,
    /// Image data left in the current IDAT chunk.
    chunk_left: u32,
    opened: bool,
}

/// The image data across IDAT chunks, read through the cover's buffer as the converter's fill
/// callback did.
struct ImageData<'a, S> {
    source: &'a mut S,
    file: File,
    buf: &'a mut [u8; READ_SIZE],
    chunk_left: &'a mut u32,
    /// The unread bytes in `buf`.
    start: usize,
    end: usize,
    ended: bool,
}

impl<S: Source> ImageData<'_, S> {
    /// The next IDAT chunk's header, skipping other chunks; false at IEND or the end of the file.
    fn next_chunk(&mut self) -> bool {
        loop {
            let Some(len) = self.file.read_be32(self.source) else {
                return false;
            };
            let mut kind = [0; 4];
            if self.file.read(self.source, &mut kind) != 4 {
                return false;
            }
            let kind = u32::from_be_bytes(kind);
            if kind == IDAT {
                *self.chunk_left = len;
                return true;
            }
            // The data and the CRC: len + 4 wraps in u32, as the converter's sum did.
            if !self.file.seek_cur(len.wrapping_add(4)) || kind == IEND {
                return false;
            }
        }
    }

    fn refill(&mut self) {
        while *self.chunk_left == 0 {
            // Past the previous chunk's CRC.
            if !self.file.seek_cur(4) || !self.next_chunk() {
                self.ended = true;
                return;
            }
        }
        let want = usize::try_from(*self.chunk_left).map_or(READ_SIZE, |left| left.min(READ_SIZE));
        let n = self
            .file
            .read(self.source, self.buf.get_mut(..want).unwrap_or_default());
        if n == 0 {
            self.ended = true;
            return;
        }
        *self.chunk_left -= u32::try_from(n).unwrap_or(0);
        self.start = 0;
        self.end = n;
    }
}

impl<S: Source> Input for ImageData<'_, S> {
    fn fill_buf(&mut self) -> &[u8] {
        if self.start == self.end && !self.ended {
            self.refill();
        }
        self.buf.get(self.start..self.end).unwrap_or_default()
    }

    fn consume(&mut self, n: usize) {
        self.start = self.start.saturating_add(n).min(self.end);
    }
}

/// The bit depths PNG allows for each color type, which the conversion to gray reads rows as.
fn bit_depth_allowed(color_type: u8, bit_depth: u8) -> bool {
    match color_type {
        pixel::GRAYSCALE => matches!(bit_depth, 1 | 2 | 4 | 8 | 16),
        pixel::INDEXED => matches!(bit_depth, 1 | 2 | 4 | 8),
        _ => matches!(bit_depth, 8 | 16),
    }
}

/// 25% red, 50% green and 25% blue: `(25 r + 50 g + 25 b) / 100`, which is `(r + 2 g + b) / 4`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "a weighted mean of bytes is a byte"
)]
fn luma(r: u8, g: u8, b: u8) -> u8 {
    ((u32::from(r) + 2 * u32::from(g) + u32::from(b)) >> 2) as u8
}

impl Cover {
    /// Makes `slot` a zeroed cover in place.
    pub fn init(slot: &mut core::mem::MaybeUninit<Self>) -> &mut Self {
        // SAFETY: integers, byte arrays and bools, all valid as zero.
        unsafe {
            slot.as_mut_ptr().write_bytes(0, 1);
            slot.assume_init_mut()
        }
    }

    /// Reads the header of the `size`-byte file in `source` and walks to its image data, taking
    /// the palette on the way, as the converter did.
    ///
    /// # Errors
    /// Why the file was refused. Width and height are set once IHDR is read, even when it is then
    /// refused.
    pub fn open(&mut self, source: &mut impl Source, size: u32) -> Result<(), Error> {
        self.buf.fill(0);
        self.width = 0;
        self.height = 0;
        self.chunk_left = 0;
        self.opened = false;
        let mut file = File { pos: 0, size };
        let result = self.read_header(source, &mut file);
        self.pos = file.pos;
        self.size = size;
        self.opened = result.is_ok();
        result
    }

    fn read_header(&mut self, source: &mut impl Source, file: &mut File) -> Result<(), Error> {
        let mut signature = [0; 8];
        if file.read(source, &mut signature) != 8 || signature != SIGNATURE {
            return Err(Error::NotPng);
        }
        file.read_be32(source).ok_or(Error::HeaderEnded)?;
        let mut kind = [0; 4];
        if file.read(source, &mut kind) != 4 || u32::from_be_bytes(kind) != IHDR {
            return Err(Error::NoHeader);
        }
        let width = file.read_be32(source).ok_or(Error::HeaderEnded)?;
        let height = file.read_be32(source).ok_or(Error::HeaderEnded)?;
        let mut rest = [0; 5];
        if file.read(source, &mut rest) != 5 {
            return Err(Error::HeaderEnded);
        }
        // Skip the CRC; if the file ends here, go on from where it is.
        file.seek_cur(4);
        let [bit_depth, color_type, compression, filter, interlace] = rest;
        self.width = width;
        self.height = height;
        self.bit_depth = bit_depth;
        self.color_type = color_type;
        if compression != 0 || filter != 0 || interlace != 0 {
            return Err(Error::Unsupported);
        }
        if width == 0 || height == 0 || width > MAX_WIDTH || height > MAX_HEIGHT {
            return Err(Error::BadSize);
        }
        let sixteen = bit_depth == 16;
        let packed = (width * u32::from(bit_depth)).div_ceil(8);
        let (step, row_bytes) = match color_type {
            pixel::GRAYSCALE if sixteen => (2, width * 2),
            pixel::GRAYSCALE | pixel::INDEXED => (1, packed),
            pixel::TRUECOLOR => {
                let step = if sixteen { 6 } else { 3 };
                (step, width * step)
            }
            pixel::GRAY_ALPHA => {
                let step = if sixteen { 4 } else { 2 };
                (step, width * step)
            }
            pixel::TRUECOLOR_ALPHA => {
                let step = if sixteen { 8 } else { 4 };
                (step, width * step)
            }
            _ => return Err(Error::Unsupported),
        };
        if !bit_depth_allowed(color_type, bit_depth) {
            return Err(Error::Unsupported);
        }
        if row_bytes > MAX_ROW_BYTES {
            return Err(Error::BadSize);
        }
        #[allow(clippy::cast_possible_truncation, reason = "at most 8")]
        let step = step as u8;
        self.step = step;
        self.row_bytes = row_bytes;

        // The chunks up to the first IDAT. PLTE bytes go into `buf` (zeroed by `open`) and stay as
        // the PLTE chunks leave them.
        let mut palette_size = 0;
        while let Some(len) = file.read_be32(source) {
            let mut kind = [0; 4];
            if file.read(source, &mut kind) != 4 {
                break;
            }
            match u32::from_be_bytes(kind) {
                PLTE => {
                    let entries = (len / 3).min(256);
                    palette_size = usize::try_from(entries).unwrap_or(256);
                    let bytes = entries * 3;
                    let dst = self.buf.get_mut(..palette_size * 3).unwrap_or_default();
                    file.read(source, dst);
                    if len > bytes {
                        file.seek_cur(len - bytes);
                    }
                    file.seek_cur(4);
                }
                IDAT => {
                    self.chunk_left = len;
                    self.palette_gray =
                        palette_gray(self.buf.get(..768).unwrap_or_default(), palette_size);
                    return Ok(());
                }
                IEND => break,
                _ => {
                    file.seek_cur(len.wrapping_add(4));
                }
            }
        }
        Err(Error::NoImageData)
    }

    #[must_use]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Decodes the image opened from `source` and hands each row to `draw` as `width` gray bytes,
    /// top to bottom; `draw` returns false to stop. Allocates the inflate state and window
    /// (about 37 KB), three rows and a gray row for the call.
    ///
    /// # Errors
    /// [`Error::Corrupt`], [`Error::UnknownFilter`], [`Error::QuitEarly`], [`Error::Memory`], or
    /// [`Error::NotOpen`].
    pub fn decode(
        &mut self,
        source: &mut impl Source,
        mut draw: impl FnMut(&[u8]) -> bool,
    ) -> Result<(), Error> {
        if !self.opened {
            return Err(Error::NotOpen);
        }
        self.opened = false;
        let row_bytes = usize::try_from(self.row_bytes).map_err(|_| Error::Memory)?;
        let width = usize::try_from(self.width).map_err(|_| Error::Memory)?;
        // Two rows (this one and the one above), the row as read and the gray row: separate
        // blocks, so that small ones stay in internal RAM (above 4096 bytes malloc puts them in
        // PSRAM).
        let zeroed = |len: usize| -> Result<Vec<u8>, Error> {
            let mut block = Vec::new();
            block.try_reserve_exact(len).map_err(|_| Error::Memory)?;
            block.resize(len, 0);
            Ok(block)
        };
        let mut rows = zeroed(2 * row_bytes)?;
        let mut raw = zeroed(row_bytes)?;
        let mut gray = zeroed(width)?;
        // SAFETY: a byte array and a `MaybeUninit`, both valid as zero.
        let mut workspace = unsafe { zeroed_box::<Workspace>() }.ok_or(Error::Memory)?;
        let Workspace { inflater, window } = &mut *workspace;
        let inflater = Inflater::init(inflater, Format::Zlib);
        let mut window = Window::new(window);
        let (mut cur, mut prev) = rows.split_at_mut(row_bytes);
        let samples = Samples {
            color_type: self.color_type,
            bit_depth: self.bit_depth,
            palette_gray: &self.palette_gray,
        };
        let mut data = ImageData {
            source,
            file: File {
                pos: self.pos,
                size: self.size,
            },
            buf: &mut self.buf,
            chunk_left: &mut self.chunk_left,
            start: 0,
            end: 0,
            ended: false,
        };
        let step = usize::from(self.step);
        for _ in 0..self.height {
            let mut filter = [0];
            let (n, result) = window.read(inflater, &mut data, &mut filter);
            if n != 1 || result.is_err() {
                return Err(Error::Corrupt);
            }
            let (n, result) = window.read(inflater, &mut data, &mut raw);
            if n != row_bytes || result.is_err() {
                return Err(Error::Corrupt);
            }
            let [filter] = filter;
            if filter > 4 {
                return Err(Error::UnknownFilter);
            }
            unfilter_into(filter, &raw, cur, prev, 0, step);
            samples.to_gray(cur, &mut gray);
            if !draw(&gray) {
                return Err(Error::QuitEarly);
            }
            core::mem::swap(&mut cur, &mut prev);
        }
        Ok(())
    }
}

/// The gray of each palette index from `rgb`, the palette as the PLTE chunks left it; an index
/// past the palette gets entry 0's gray.
fn palette_gray(rgb: &[u8], palette_size: usize) -> [u8; 256] {
    let (entries, _) = rgb.as_chunks::<3>();
    let first = entries.first().map_or(0, |&[r, g, b]| luma(r, g, b));
    let mut gray = [first; 256];
    for (out, &[r, g, b]) in gray.iter_mut().zip(entries).take(palette_size) {
        *out = luma(r, g, b);
    }
    gray
}

/// How a row's samples read as gray.
struct Samples<'a> {
    color_type: u8,
    bit_depth: u8,
    palette_gray: &'a [u8; 256],
}

/// Each `N`-byte pixel's gray from its bytes.
fn convert<const N: usize>(src: &[u8], gray: &mut [u8], to_gray: impl Fn(&[u8; N]) -> u8) {
    let (pixels, _) = src.as_chunks::<N>();
    for (out, pixel) in gray.iter_mut().zip(pixels) {
        *out = to_gray(pixel);
    }
}

impl Samples<'_> {
    /// The converter's conversion of a row of samples to gray.
    fn to_gray(&self, src: &[u8], gray: &mut [u8]) {
        let sixteen = self.bit_depth == 16;
        match (self.color_type, sixteen) {
            (pixel::GRAYSCALE, true) => convert(src, gray, |&[high, _]| high),
            (pixel::GRAYSCALE, false) if self.bit_depth == 8 => convert(src, gray, |&[v]| v),
            (pixel::GRAYSCALE, false) => {
                let max = (1u16 << self.bit_depth) - 1;
                for (x, out) in gray.iter_mut().enumerate() {
                    #[allow(clippy::cast_possible_truncation, reason = "a sample scaled to 255")]
                    let level = (u16::from(self.sample(src, x)) * 255 / max) as u8;
                    *out = level;
                }
            }
            (pixel::INDEXED, _) if self.bit_depth == 8 => {
                for (out, &index) in gray.iter_mut().zip(src) {
                    *out = self
                        .palette_gray
                        .get(usize::from(index))
                        .copied()
                        .unwrap_or(0);
                }
            }
            (pixel::INDEXED, _) => {
                for (x, out) in gray.iter_mut().enumerate() {
                    let index = usize::from(self.sample(src, x));
                    *out = self.palette_gray.get(index).copied().unwrap_or(0);
                }
            }
            (pixel::TRUECOLOR, false) => convert(src, gray, |&[r, g, b]| luma(r, g, b)),
            (pixel::TRUECOLOR, true) => convert(src, gray, |&[r, _, g, _, b, _]| luma(r, g, b)),
            (pixel::GRAY_ALPHA, false) => convert(src, gray, |&[v, _]| v),
            (pixel::GRAY_ALPHA, true) => convert(src, gray, |&[v, _, _, _]| v),
            (_, false) => convert(src, gray, |&[r, g, b, _]| luma(r, g, b)),
            (_, true) => convert(src, gray, |&[r, _, g, _, b, _, _, _]| luma(r, g, b)),
        }
    }

    /// Sample `x` of a row packed at 1, 2, 4 or 8 bits, most significant bit first.
    fn sample(&self, src: &[u8], x: usize) -> u8 {
        let depth = usize::from(self.bit_depth);
        let per_byte = 8 / depth;
        let shift = (per_byte - 1 - x % per_byte) * depth;
        let mask = u8::try_from((1u16 << depth) - 1).unwrap_or(u8::MAX);
        src.get(x / per_byte)
            .map_or(0, |&byte| (byte >> shift) & mask)
    }
}
