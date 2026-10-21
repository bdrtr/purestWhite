//! BMP reading for lib/GfxRenderer's `Bitmap`: the headers and palette of an uncompressed BMP
//! (1, 2, 4, 8, 24 or 32 bits per pixel, bottom-up or top-down, at most 2048 x 3072), and each
//! row's pixels as 8-bit luminance. Dithering and packing to the display's gray levels stay with
//! the caller.
//!
//! [`parse`] reads the file in the order `Bitmap::parseHeaders` did, so a damaged file fails the
//! same way: bytes past the end read as 0, and fields read before an error are left set. A palette
//! entry cut short reads its missing bytes as 0.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub const MAX_WIDTH: i32 = 2048;
pub const MAX_HEIGHT: i32 = 3072;

/// What the headers say. `bb_bmp` in C.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Bmp {
    pub width: i32,
    pub height: i32,
    /// Bytes per row in the file, padded to 4.
    pub row_bytes: i32,
    /// Where the pixels start.
    pub data_offset: u32,
    pub colors_used: u32,
    pub bits_per_pixel: u16,
    pub top_down: bool,
    /// The file has a palette and every entry is within 21 of one of the display's gray levels
    /// (0, 85, 170, 255), so the image needs no dithering; always so at 1 and 2 bits per pixel.
    pub native_palette: bool,
    /// The luminance of each palette index; entries past the palette hold their own index.
    pub palette_lum: [u8; 256],
}

impl Default for Bmp {
    fn default() -> Self {
        Bmp {
            width: 0,
            height: 0,
            row_bytes: 0,
            data_offset: 0,
            colors_used: 0,
            bits_per_pixel: 0,
            top_down: false,
            native_palette: false,
            palette_lum: [0; 256],
        }
    }
}

/// Why a file is refused: `BmpReaderError`'s numbers.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    SeekStartFailed = 2,
    NotBmp = 3,
    DibTooSmall = 4,
    BadPlanes = 5,
    UnsupportedBpp = 6,
    UnsupportedCompression = 7,
    BadDimensions = 8,
    ImageTooLarge = 9,
    PaletteTooLarge = 10,
    SeekPixelDataFailed = 11,
}

/// The file.
pub trait Source {
    /// Fills the start of `buf` and says how many bytes it read; 0 at the end or on an error.
    fn read(&mut self, buf: &mut [u8]) -> usize;
    /// Moves `count` bytes forward; a failure leaves the position.
    fn skip(&mut self, count: u32);
    fn seek(&mut self, position: u32) -> bool;
}

fn read_bytes<const N: usize>(source: &mut impl Source) -> [u8; N] {
    // Missing bytes read as 0.
    let mut bytes = [0; N];
    let mut filled = 0;
    while let Some(rest) = bytes.get_mut(filled..)
        && !rest.is_empty()
    {
        let count = source.read(rest).min(rest.len());
        if count == 0 {
            break;
        }
        filled += count;
    }
    bytes
}

fn read_u16(source: &mut impl Source) -> u16 {
    u16::from_le_bytes(read_bytes(source))
}

fn read_u32(source: &mut impl Source) -> u32 {
    u32::from_le_bytes(read_bytes(source))
}

#[allow(clippy::cast_possible_truncation)]
fn luminance(red: u8, green: u8, blue: u8) -> u8 {
    ((77 * u32::from(red) + 150 * u32::from(green) + 29 * u32::from(blue)) >> 8) as u8
}

/// Reads the headers and palette into `out` and leaves the file at the pixels.
///
/// # Errors
///
/// Why the file cannot be shown; the fields read before that are left in `out`.
#[allow(clippy::cast_possible_wrap)]
pub fn parse(source: &mut impl Source, out: &mut Bmp) -> Result<(), Error> {
    if !source.seek(0) {
        return Err(Error::SeekStartFailed);
    }
    if read_u16(source) != 0x4D42 {
        return Err(Error::NotBmp);
    }
    source.skip(8);
    out.data_offset = read_u32(source);
    if read_u32(source) < 40 {
        return Err(Error::DibTooSmall);
    }
    out.width = read_u32(source) as i32;
    let raw_height = read_u32(source) as i32;
    out.top_down = raw_height < 0;
    out.height = if out.top_down {
        raw_height.wrapping_neg()
    } else {
        raw_height
    };
    let planes = read_u16(source);
    out.bits_per_pixel = read_u16(source);
    let compression = read_u32(source);
    let bpp = out.bits_per_pixel;
    if planes != 1 {
        return Err(Error::BadPlanes);
    }
    if !matches!(bpp, 1 | 2 | 4 | 8 | 24 | 32) {
        return Err(Error::UnsupportedBpp);
    }
    // BI_RGB, or BI_BITFIELDS for 32-bit BGRA masks.
    if !(compression == 0 || (bpp == 32 && compression == 3)) {
        return Err(Error::UnsupportedCompression);
    }
    // Image size and resolution.
    source.skip(12);
    out.colors_used = read_u32(source);
    if out.colors_used == 0 && bpp <= 8 {
        out.colors_used = 1 << bpp;
    }
    if out.colors_used > 256 {
        return Err(Error::PaletteTooLarge);
    }
    // Important colors.
    source.skip(4);
    if out.width <= 0 || out.height <= 0 {
        return Err(Error::BadDimensions);
    }
    if out.width > MAX_WIDTH || out.height > MAX_HEIGHT {
        return Err(Error::ImageTooLarge);
    }
    out.row_bytes = (out.width * i32::from(bpp) + 31) / 32 * 4;

    for (i, lum) in (0u8..=255).zip(out.palette_lum.iter_mut()) {
        *lum = i;
    }
    let colors = usize::try_from(out.colors_used).unwrap_or(0);
    for lum in out.palette_lum.iter_mut().take(colors) {
        let [blue, green, red, _] = read_bytes::<4>(source);
        *lum = luminance(red, green, blue);
    }
    if !source.seek(out.data_offset) {
        return Err(Error::SeekPixelDataFailed);
    }
    out.native_palette = bpp <= 2
        || (colors > 0
            && out.palette_lum.iter().take(colors).all(|&lum| {
                let (lum, level) = (u16::from(lum), u16::from(lum >> 6) * 85);
                lum <= level + 21 && lum + 21 >= level
            }));
    Ok(())
}

impl Bmp {
    /// The luminance of pixels `first..first + out.len()` of a row read from the file, as if bytes
    /// past `row` were 0. False for a depth [`parse`] refuses.
    #[must_use]
    pub fn row_luminance(&self, row: &[u8], first: usize, out: &mut [u8]) -> bool {
        let palette = |i: u8| self.palette_lum.get(usize::from(i)).copied().unwrap_or(0);
        let byte = |i: usize| row.get(i).copied().unwrap_or(0);
        match self.bits_per_pixel {
            32 => true_color::<4>(row, first, out),
            24 => true_color::<3>(row, first, out),
            8 => {
                for (x, lum) in (first..).zip(out.iter_mut()) {
                    *lum = palette(byte(x));
                }
            }
            4 => {
                for (x, lum) in (first..).zip(out.iter_mut()) {
                    let pair = byte(x >> 1);
                    *lum = palette(if x & 1 == 1 { pair & 0x0F } else { pair >> 4 });
                }
            }
            2 => {
                for (x, lum) in (first..).zip(out.iter_mut()) {
                    *lum = palette((byte(x >> 2) >> (6 - (x & 3) * 2)) & 0x03);
                }
            }
            1 => {
                for (x, lum) in (first..).zip(out.iter_mut()) {
                    *lum = palette(u8::from(byte(x >> 3) & (0x80 >> (x & 7)) != 0));
                }
            }
            _ => return false,
        }
        true
    }
}

/// Blue, green, red (and alpha) pixels of `N` bytes.
fn true_color<const N: usize>(row: &[u8], first: usize, out: &mut [u8]) {
    let pixels = row.get(first.saturating_mul(N)..).unwrap_or_default();
    let (whole, part) = pixels.as_chunks::<N>();
    let mut cut = [0; N];
    if let Some(start) = cut.get_mut(..part.len()) {
        start.copy_from_slice(part);
    }
    let mut lums = out.iter_mut();
    for (pixel, lum) in whole.iter().chain(core::iter::once(&cut)).zip(&mut lums) {
        let [blue, green, red, ..] = pixel.as_slice() else {
            continue;
        };
        *lum = luminance(*red, *green, *blue);
    }
    for lum in lums {
        *lum = 0;
    }
}

#[cfg(test)]
mod tests;
