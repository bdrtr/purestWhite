//! The PNG decoder for chapter images: our own, decoding as PNGdec 1.1.6 (the library it replaced)
//! did, which `test/png_decoder` pins. Rows come out one at a time through a callback, as raw
//! samples with the palette and transparency, for `PngToFramebufferConverter` to dither.
//!
//! The chapter decoder, like PNGdec, supports bit depths up to 8 and no interlacing, checks no CRC
//! but IHDR's and not the Adler-32 sum, and reads the file through a 2048-byte buffer whose
//! bookkeeping decides how odd chunk layouts come out (an empty IDAT, for one, leaves it reading
//! stale bytes as the next chunk). It departs from PNGdec only where PNGdec misbehaved:
//! - A file cut inside image data returns [`Error::FileEnded`]. PNGdec measured a chunk's length
//!   from the start of its read buffer, not from the chunk, so it missed a cut near a chunk's end
//!   and waited forever for the rest. A zlib stream that ends inside a row short of the height
//!   also returns [`Error::FileEnded`], at once; PNGdec read on to the end of the file and
//!   returned the same there.
//! - Bytes PNGdec read or wrote past its buffers are dropped or read as 0: a palette or tRNS chunk
//!   longer than the palette, chunk data past the read buffer.
//! - On corrupt data, a row complete when the damage is found is still drawn (PNGdec dropped it
//!   when the bad data came right after the row's last byte in the same inflate call), and a zlib
//!   header asking for a preset dictionary is corrupt (PNGdec stalled on it and returned at the
//!   end of the file).
//!
//! Covers are read by [`cover`], as `PngToBmpConverter`'s own decoder read them, through the same
//! inflate and unfiltering.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

pub mod cover;
mod gray;

use alloc::alloc::{Layout, alloc_zeroed};
use alloc::boxed::Box;
use alloc::vec::Vec;
use bembeyaz_inflate::{Feed, Format, Inflater, WINDOW_SIZE, Window};
use core::mem::MaybeUninit;
use core::ops::Range;

/// `open` refuses an image whose rows take this many bytes or more: the `PNG_MAX_BUFFERED_PIXELS`
/// value platformio.ini set for PNGdec.
pub const MAX_PITCH: i32 = 16416;

/// The size of the read buffer, which decides how chunks are walked.
const FILE_BUF_SIZE: usize = 2048;
/// A row's palette: 256 RGB triples, then 256 alpha values.
pub const PALETTE_SIZE: usize = 1024;
/// Where the palette's alpha values start.
const ALPHA: usize = 768;
/// Bytes a PLTE chunk may fill: the whole palette, alpha included, as far as PNGdec's copy reached.
const PALETTE_LEN: i64 = 1024;
/// Alpha values a tRNS chunk may fill.
const ALPHA_LEN: i64 = 256;

pub mod pixel {
    pub const GRAYSCALE: u8 = 0;
    pub const TRUECOLOR: u8 = 2;
    pub const INDEXED: u8 = 3;
    pub const GRAY_ALPHA: u8 = 4;
    pub const TRUECOLOR_ALPHA: u8 = 6;
}

const PLTE: u32 = u32::from_be_bytes(*b"PLTE");
const TRNS: u32 = u32::from_be_bytes(*b"tRNS");
const IDAT: u32 = u32::from_be_bytes(*b"IDAT");
const IHDR: u32 = u32::from_be_bytes(*b"IHDR");

/// Why `open` or `decode` failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `decode` without a successful `open`.
    NotOpen,
    /// The file, or its image data, ended before the image did.
    FileEnded,
    /// The image data is corrupt.
    Corrupt,
    /// Out of memory.
    Memory,
    /// Interlaced, or more than 8 bits per sample.
    Unsupported,
    /// Not a PNG file, or a broken header.
    InvalidFile,
    /// Rows too long to decode (see [`MAX_PITCH`]).
    TooBig,
    /// The row callback asked to stop.
    QuitEarly,
}

/// The file being decoded.
pub trait Source {
    /// Reads up to `buf.len()` bytes at `pos` and returns how many it read: fewer at the end of
    /// the file, 0 past it or on an error.
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> usize;
}

/// One decoded row, before any conversion.
pub struct Row<'a> {
    pub y: i32,
    pub width: i32,
    /// The row's samples (`pixels.len()` is the pitch), packed most significant bit first below
    /// 8 bits.
    pub pixels: &'a [u8],
    /// 256 RGB triples, then 256 alpha values.
    pub palette: &'a [u8; PALETTE_SIZE],
    /// The tRNS color key: a gray sample, or 0xRRGGBB, each the low byte of its 16-bit value.
    pub transparent: u32,
    pub pixel_type: u8,
    pub bpp: u8,
    pub has_alpha: bool,
}

/// An image being decoded. About 3 KB: keep it off the stack ([`Png::init`]).
pub struct Png {
    buf: [u8; FILE_BUF_SIZE],
    palette: [u8; PALETTE_SIZE],
    size: u32,
    width: i32,
    height: i32,
    pitch: i32,
    transparent: u32,
    bpp: u8,
    pixel_type: u8,
    has_alpha: bool,
    opened: bool,
}

/// The inflate state, its window and nothing else that has to be zeroed.
struct Workspace {
    inflater: MaybeUninit<Inflater>,
    window: [u8; WINDOW_SIZE],
}

/// # Safety
/// All-zero bytes must be a valid `T`, and `T` not zero-sized.
unsafe fn zeroed_box<T>() -> Option<Box<T>> {
    // SAFETY: per the contract; the block has `T`'s own layout.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)] // the layout is `T`'s own
        let block = alloc_zeroed(Layout::new::<T>()).cast::<T>();
        if block.is_null() {
            None
        } else {
            Some(Box::from_raw(block))
        }
    }
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

fn be32(bytes: &[u8], at: usize) -> i32 {
    let mut word = [0; 4];
    if let Some(src) = bytes.get(at..at + 4) {
        word.copy_from_slice(src);
    }
    i32::from_be_bytes(word)
}

/// `bytes[at..at + len]`, cut at the end of `bytes` (PNGdec read on past it).
fn span(bytes: &[u8], at: i64, len: i64) -> &[u8] {
    let start = usize::try_from(at).unwrap_or(usize::MAX).min(bytes.len());
    let len = usize::try_from(len).unwrap_or(0);
    let rest = bytes.get(start..).unwrap_or_default();
    rest.get(..len.min(rest.len())).unwrap_or_default()
}

fn read(source: &mut impl Source, pos: i64, buf: &mut [u8]) -> i64 {
    let Ok(pos) = u32::try_from(pos) else {
        return 0;
    };
    let n = source.read_at(pos, buf).min(buf.len());
    i64::try_from(n).unwrap_or(0)
}

/// Undoes a row's filter for the raw bytes `raw` that continue the row at byte `x0` (after the
/// filter byte), writing them to `row[x0..]`. `row[..x0]` holds the row so far and `up` the row
/// above. Pixels are `step` bytes apart (for the chapter decoder, as PNGdec counted them).
///
/// The chapter decoder hands it raw bytes straight from the inflate window, so unfiltering is the
/// copy out of it; the cover reader hands it whole rows.
fn unfilter_into(filter: u8, raw: &[u8], row: &mut [u8], up: &[u8], x0: usize, step: usize) {
    let end = (x0 + raw.len()).min(row.len()).min(up.len());
    let raw = raw.get(..end.saturating_sub(x0)).unwrap_or_default();
    match filter {
        1 | 3 | 4 => {}
        2 => {
            let (Some(dst), Some(up)) = (row.get_mut(x0..end), up.get(x0..end)) else {
                return;
            };
            let (words, dst_rest) = dst.as_chunks_mut::<4>();
            let (raw_words, raw_rest) = raw.as_chunks::<4>();
            let (up_words, up_rest) = up.as_chunks::<4>();
            for ((v, r), u) in words.iter_mut().zip(raw_words).zip(up_words) {
                v[0] = r[0].wrapping_add(u[0]);
                v[1] = r[1].wrapping_add(u[1]);
                v[2] = r[2].wrapping_add(u[2]);
                v[3] = r[3].wrapping_add(u[3]);
            }
            for ((v, &r), &u) in dst_rest.iter_mut().zip(raw_rest).zip(up_rest) {
                *v = r.wrapping_add(u);
            }
            return;
        }
        // None, and the unknown types PNGdec left alone.
        _ => {
            if let Some(dst) = row.get_mut(x0..end) {
                dst.copy_from_slice(raw);
            }
            return;
        }
    }
    // Byte by byte up to the first whole pixel that has one to its left, then whole pixels,
    // then what is left.
    let start = x0.max(step).next_multiple_of(step.max(1)).min(end);
    unfilter_bytes(filter, raw, row, up, x0, x0..start, step);
    let x = match step {
        1 => unfilter_1(filter, raw, row, up, x0, start, end),
        2 => unfilter_2(filter, raw, row, up, x0, start, end),
        3 => unfilter_3(filter, raw, row, up, x0, start, end),
        4 => unfilter_4(filter, raw, row, up, x0, start, end),
        6 | 8 => unfilter_wide(filter, raw, row, up, x0, start, end, step),
        _ => start,
    };
    unfilter_bytes(filter, raw, row, up, x0, x..end, step);
}

/// [`unfilter_into`] byte by byte over `row[span]`, for the ends of a piece and for steps no valid
/// image has (PNGdec's arithmetic gives them for some broken headers).
fn unfilter_bytes(
    filter: u8,
    raw: &[u8],
    row: &mut [u8],
    up: &[u8],
    x0: usize,
    span: Range<usize>,
    step: usize,
) {
    for x in span {
        let u = up.get(x).copied().unwrap_or(0);
        let (left, up_left) = match x.checked_sub(step) {
            Some(i) => (
                row.get(i).copied().unwrap_or(0),
                up.get(i).copied().unwrap_or(0),
            ),
            None => (0, 0),
        };
        let predicted = match filter {
            1 => left,
            3 => u8::midpoint(left, u),
            _ => paeth(left, u, up_left),
        };
        let r = raw.get(x - x0).copied().unwrap_or(0);
        if let Some(v) = row.get_mut(x) {
            *v = r.wrapping_add(predicted);
        }
    }
}

/// [`unfilter_into`] over the whole `N`-byte pixels of `row[start..end]` (`start` a multiple of
/// `N`, at least `N`), one function per size with each pixel's bytes written out: at
/// `opt-level = "s"` LLVM keeps a loop over a pixel's bytes, and on the Xtensa every pass of a loop
/// costs a taken branch and a counter. Returns where the pixels end.
macro_rules! unfilter_pixels {
    ($name:ident, $n:literal, [$($i:literal),+]) => {
        fn $name(
            filter: u8,
            raw: &[u8],
            row: &mut [u8],
            up: &[u8],
            x0: usize,
            start: usize,
            end: usize,
        ) -> usize {
            // A piece that ends before the second pixel has none of these.
            if start < $n || start >= end {
                return start;
            }
            let count = (end - start) / $n;
            let stop = start + count * $n;
            let (Some(before), Some(up_before), Some(raw), Some(up_px)) = (
                row.get(start - $n..start),
                up.get(start - $n..start),
                raw.get(start - x0..stop - x0),
                up.get(start..stop),
            ) else {
                return start;
            };
            // The pixels to the left and up-left, carried along in registers.
            let mut left = [0u8; $n];
            left.copy_from_slice(before);
            let mut up_left = [0u8; $n];
            up_left.copy_from_slice(up_before);
            let Some(dst) = row.get_mut(start..stop) else {
                return start;
            };
            let (dst, _) = dst.as_chunks_mut::<$n>();
            let (raw, _) = raw.as_chunks::<$n>();
            let (ups, _) = up_px.as_chunks::<$n>();
            match filter {
                1 => {
                    for (v, r) in dst.iter_mut().zip(raw) {
                        $( left[$i] = r[$i].wrapping_add(left[$i]); v[$i] = left[$i]; )+
                    }
                }
                3 => {
                    for ((v, r), u) in dst.iter_mut().zip(raw).zip(ups) {
                        $( left[$i] = r[$i].wrapping_add(u8::midpoint(left[$i], u[$i])); v[$i] = left[$i]; )+
                    }
                }
                _ => {
                    for ((v, r), u) in dst.iter_mut().zip(raw).zip(ups) {
                        $(
                            left[$i] = r[$i].wrapping_add(paeth(left[$i], u[$i], up_left[$i]));
                            v[$i] = left[$i];
                            up_left[$i] = u[$i];
                        )+
                    }
                }
            }
            stop
        }
    };
}

unfilter_pixels!(unfilter_1, 1, [0]);
unfilter_pixels!(unfilter_2, 2, [0, 1]);
unfilter_pixels!(unfilter_3, 3, [0, 1, 2]);
unfilter_pixels!(unfilter_4, 4, [0, 1, 2, 3]);

/// [`unfilter_into`] over the whole pixels of `row[start..end]` for 6- and 8-byte pixels (16-bit RGB
/// and RGBA covers): as `unfilter_pixels!`, with the pixel's bytes in a loop. Returns where the
/// pixels end.
#[allow(
    clippy::too_many_arguments,
    reason = "the unfilter_pixels functions' arguments, and the step"
)]
fn unfilter_wide(
    filter: u8,
    raw: &[u8],
    row: &mut [u8],
    up: &[u8],
    x0: usize,
    start: usize,
    end: usize,
    step: usize,
) -> usize {
    let (mut left, mut up_left) = ([0u8; 8], [0u8; 8]);
    if start < step || start >= end {
        return start;
    }
    let last = start + (end - start) / step * step;
    let (Some(left), Some(up_left), Some(before), Some(up_before)) = (
        left.get_mut(..step),
        up_left.get_mut(..step),
        row.get(start - step..start),
        up.get(start - step..start),
    ) else {
        return start;
    };
    left.copy_from_slice(before);
    up_left.copy_from_slice(up_before);
    let (Some(dst), Some(raw), Some(ups)) = (
        row.get_mut(start..last),
        raw.get(start - x0..last - x0),
        up.get(start..last),
    ) else {
        return start;
    };
    let pixels = dst
        .chunks_mut(step)
        .zip(raw.chunks(step))
        .zip(ups.chunks(step));
    match filter {
        1 => {
            for ((v, r), _) in pixels {
                for ((v, &r), l) in v.iter_mut().zip(r).zip(left.iter_mut()) {
                    *l = r.wrapping_add(*l);
                    *v = *l;
                }
            }
        }
        3 => {
            for ((v, r), u) in pixels {
                for (((v, &r), &u), l) in v.iter_mut().zip(r).zip(u).zip(left.iter_mut()) {
                    *l = r.wrapping_add(u8::midpoint(*l, u));
                    *v = *l;
                }
            }
        }
        _ => {
            for ((v, r), u) in pixels {
                for ((((v, &r), &u), l), ul) in v
                    .iter_mut()
                    .zip(r)
                    .zip(u)
                    .zip(left.iter_mut())
                    .zip(up_left.iter_mut())
                {
                    *l = r.wrapping_add(paeth(*l, u, *ul));
                    *v = *l;
                    *ul = u;
                }
            }
        }
    }
    last
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let (ia, ib, ic) = (i16::from(a), i16::from(b), i16::from(c));
    let pa = (ib - ic).abs();
    let pb = (ia - ic).abs();
    let pc = (ia + ib - 2 * ic).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

impl Png {
    /// A zeroed image, allocated in place; `None` when out of memory.
    #[cfg(test)]
    #[must_use]
    pub fn new_boxed() -> Option<Box<Self>> {
        // SAFETY: integers, byte arrays and bools, all valid as zero.
        unsafe { zeroed_box() }
    }

    /// Makes `slot` a zeroed image in place.
    pub fn init(slot: &mut MaybeUninit<Self>) -> &mut Self {
        // SAFETY: integers, byte arrays and bools, all valid as zero.
        unsafe {
            slot.as_mut_ptr().write_bytes(0, 1);
            slot.assume_init_mut()
        }
    }

    /// Reads the header of the `size`-byte file in `source`, as PNGdec's `open` did: a file whose
    /// IHDR is not the first chunk opens with every field 0.
    ///
    /// # Errors
    /// [`Error::InvalidFile`], or [`Error::Unsupported`] or [`Error::TooBig`] with the fields set.
    pub fn open(&mut self, source: &mut impl Source, size: u32) -> Result<(), Error> {
        self.buf.fill(0);
        self.palette.fill(0);
        self.size = size;
        self.width = 0;
        self.height = 0;
        self.pitch = 0;
        self.transparent = 0;
        self.bpp = 0;
        self.pixel_type = 0;
        self.has_alpha = false;
        self.opened = false;

        let header = self.buf.get_mut(..33).unwrap_or_default();
        if read(source, 0, header) < 33 || be32(&self.buf, 0) != 0x8950_4E47_u32.cast_signed() {
            return Err(Error::InvalidFile);
        }
        if be32(&self.buf, 12).cast_unsigned() == IHDR {
            let crc = crc32(self.buf.get(12..29).unwrap_or_default());
            if be32(&self.buf, 8) != 13 || crc.cast_signed() != be32(&self.buf, 29) {
                return Err(Error::InvalidFile);
            }
            let [bpp, pixel_type, _, _, interlace] =
                [24, 25, 26, 27, 28].map(|i| self.buf.get(i).copied().unwrap_or(0));
            self.width = be32(&self.buf, 16);
            self.height = be32(&self.buf, 20);
            self.bpp = bpp;
            self.pixel_type = pixel_type;
            if interlace != 0 || bpp > 8 {
                return Err(Error::Unsupported);
            }
            let channels = match pixel_type {
                pixel::GRAYSCALE | pixel::INDEXED => 1,
                pixel::TRUECOLOR => 3,
                pixel::GRAY_ALPHA => 2,
                pixel::TRUECOLOR_ALPHA => 4,
                _ => 0,
            };
            self.has_alpha = matches!(pixel_type, pixel::GRAY_ALPHA | pixel::TRUECOLOR_ALPHA);
            if channels > 0 {
                // PNGdec's int arithmetic, overflow and all.
                self.pitch = (channels * i32::from(bpp))
                    .wrapping_mul(self.width)
                    .wrapping_add(7)
                    / 8;
            }
        }
        if self.pitch >= MAX_PITCH {
            return Err(Error::TooBig);
        }
        self.opened = true;
        Ok(())
    }

    #[must_use]
    pub fn width(&self) -> i32 {
        self.width
    }

    #[must_use]
    pub fn height(&self) -> i32 {
        self.height
    }

    /// Bits per sample.
    #[must_use]
    pub fn bpp(&self) -> u8 {
        self.bpp
    }

    /// One of [`pixel`].
    #[must_use]
    pub fn pixel_type(&self) -> u8 {
        self.pixel_type
    }

    /// An alpha channel, or (once decoding has met a tRNS chunk) transparency.
    #[must_use]
    pub fn has_alpha(&self) -> bool {
        self.has_alpha
    }

    /// Decodes the image opened from `source` and hands each row to `draw`, top to bottom;
    /// `draw` returns false to stop. Rows the data holds past the height may follow, as PNGdec
    /// sent them. Allocates the inflate window (32 KiB) and two rows for the call.
    ///
    /// # Errors
    /// [`Error::FileEnded`], [`Error::Corrupt`], [`Error::QuitEarly`], [`Error::Memory`],
    /// [`Error::TooBig`] for a row size PNGdec's arithmetic made negative, or [`Error::NotOpen`].
    #[allow(
        clippy::too_many_lines,
        reason = "PNGdec's DecodePNG in one piece, to read against it"
    )]
    pub fn decode(
        &mut self,
        source: &mut impl Source,
        mut draw: impl FnMut(&Row<'_>) -> bool,
    ) -> Result<(), Error> {
        if !self.opened {
            return Err(Error::NotOpen);
        }
        let line_len = usize::try_from(self.pitch).map_err(|_| Error::TooBig)? + 1;
        let mut lines = Vec::new();
        lines
            .try_reserve_exact(2 * line_len)
            .map_err(|_| Error::Memory)?;
        lines.resize(2 * line_len, 0);
        // SAFETY: a byte array and a `MaybeUninit`, both valid as zero.
        let mut workspace = unsafe { zeroed_box::<Workspace>() }.ok_or(Error::Memory)?;
        let Workspace { inflater, window } = &mut *workspace;
        let inflater = Inflater::init(inflater, Format::ZlibUnchecked);
        let mut window = Window::new(window);
        let (mut cur, mut prev) = lines.split_at_mut(line_len);
        // Bytes apart that a pixel's filter looks back, as PNGdec counted them.
        let step = if self.pitch <= self.width {
            1
        } else {
            self.pitch / self.width
        };
        let step = usize::try_from(step).unwrap_or(1).max(1);
        // Bytes of the current row decoded so far.
        let mut filled = 0;

        // PNGdec's walk: `buf` holds the `bytes_read` file bytes that end at `file_offset`, and
        // `offset` is the position in it. Inside IDAT data `bytes_read` counts what is left.
        let mut offset: i64 = 0;
        let mut file_offset: i64 = 8;
        let mut bytes_read: i64 = 0;
        let mut y = 0;
        let mut done = false;
        let mut result = Ok(());
        while !done && y < self.height {
            if offset > bytes_read - 8 {
                file_offset += offset - bytes_read;
                bytes_read = read(source, file_offset, &mut self.buf);
                if bytes_read < 8 {
                    return Err(Error::FileEnded);
                }
                file_offset += bytes_read;
                offset = 0;
            }
            let at = usize::try_from(offset).unwrap_or(usize::MAX);
            let mut len = i64::from(be32(&self.buf, at));
            if len < 0 || len + (file_offset - bytes_read) > i64::from(self.size) {
                return Err(Error::FileEnded);
            }
            let marker = be32(&self.buf, at.saturating_add(4)).cast_unsigned();
            offset += 8;
            match marker {
                PLTE => {
                    self.palette.get_mut(ALPHA..).unwrap_or_default().fill(0xFF);
                    let colors = span(&self.buf, offset, len.min(PALETTE_LEN));
                    if let Some(dst) = self.palette.get_mut(..colors.len()) {
                        dst.copy_from_slice(colors);
                    }
                }
                TRNS => {
                    let at = |i: i64| {
                        u32::from(span(&self.buf, offset + i, 1).first().copied().unwrap_or(0))
                    };
                    if self.pixel_type == pixel::INDEXED {
                        let alpha = span(&self.buf, offset, len.min(ALPHA_LEN));
                        if let Some(dst) = self.palette.get_mut(ALPHA..ALPHA + alpha.len()) {
                            dst.copy_from_slice(alpha);
                        }
                        self.has_alpha = true;
                    } else if len == 2 {
                        self.transparent = at(1);
                        self.has_alpha = true;
                    } else if len == 6 {
                        self.transparent = at(5) | (at(3) << 8) | (at(1) << 16);
                        self.has_alpha = true;
                    }
                }
                IDAT => {
                    while len > 0 {
                        if offset >= bytes_read {
                            let want = usize::try_from(len)
                                .unwrap_or(FILE_BUF_SIZE)
                                .min(FILE_BUF_SIZE);
                            bytes_read = read(
                                source,
                                file_offset,
                                self.buf.get_mut(..want).unwrap_or_default(),
                            );
                            if bytes_read == 0 {
                                return Err(Error::FileEnded);
                            }
                            file_offset += bytes_read;
                            offset = 0;
                        } else {
                            bytes_read -= offset;
                        }
                        let take = bytes_read.min(len);
                        let mut input = span(&self.buf, offset, take);
                        offset += take;
                        bytes_read -= take;
                        len -= take;
                        loop {
                            let want = line_len - filled;
                            let (_, fed) = window.feed_with(inflater, &mut input, want, |piece| {
                                let mut piece = piece;
                                if filled == 0
                                    && let Some((&filter, rest)) = piece.split_first()
                                {
                                    if let Some(slot) = cur.first_mut() {
                                        *slot = filter;
                                    }
                                    piece = rest;
                                    filled = 1;
                                }
                                let filter = cur.first().copied().unwrap_or(0);
                                let (_, row) = cur.split_at_mut(1);
                                let up = prev.get(1..).unwrap_or_default();
                                unfilter_into(filter, piece, row, up, filled - 1, step);
                                filled += piece.len();
                            });
                            let ended = match fed {
                                Ok(Feed::OutputFull) => None,
                                Ok(Feed::NeedInput) => break,
                                Ok(Feed::Done) => Some(Ok(())),
                                Err(_) => Some(Err(Error::Corrupt)),
                            };
                            if filled == line_len {
                                let row = Row {
                                    y,
                                    width: self.width,
                                    pixels: cur.get(1..).unwrap_or_default(),
                                    palette: &self.palette,
                                    transparent: self.transparent,
                                    pixel_type: self.pixel_type,
                                    bpp: self.bpp,
                                    has_alpha: self.has_alpha,
                                };
                                if !draw(&row) {
                                    return Err(Error::QuitEarly);
                                }
                                y += 1;
                                core::mem::swap(&mut cur, &mut prev);
                                filled = 0;
                            } else if ended == Some(Ok(())) && y < self.height {
                                // The stream ended before the last row: PNGdec walked on to the end
                                // of the file and returned there. One that ends after the last row
                                // (a partial extra row, or the trailer in a later piece) succeeded
                                // in PNGdec, and does here.
                                return Err(Error::FileEnded);
                            }
                            if let Some(end) = ended {
                                // PNGdec read the rest of the chunk and stopped.
                                result = end;
                                len = 0;
                                done = true;
                                break;
                            }
                        }
                    }
                    if bytes_read != 0 {
                        bytes_read += offset;
                    } else {
                        offset = 0;
                    }
                }
                _ => {}
            }
            offset += len + 4;
        }
        result
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "tests"
)]
mod tests;
