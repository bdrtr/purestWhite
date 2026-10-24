//! Gray levels (0 black to 255 white) to the panel's 2-bit levels (0 black to 3 white) or to 1 bit
//! (0 black, 1 white), for BMP rows (lib/GfxRenderer's `Bitmap`, the PNG and JPEG cover
//! converters): by thresholds, or by error diffusion whose rows of carried error the caller owns.
//! Images as they decode are resampled and dithered by position in [`block_row`] and [`line_row`];
//! covers are scaled before dithering by [`area_add`] and [`area_pixels`], or [`linear_row`] and
//! [`blend_rows`].
//!
//! Nothing here panics: pixels past the output are dropped, and errors past the error rows are not
//! carried.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

/// How gray becomes levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    /// 2-bit: fixed thresholds tuned to the panel, no diffusion.
    Threshold,
    /// 2-bit: the top two bits, for palettes that already hold the panel's levels.
    Native,
    /// 2-bit: Atkinson diffusion (6/8 of the error, to six neighbours over two rows).
    Atkinson,
    /// 2-bit: Floyd-Steinberg diffusion, mirrored on odd rows.
    FloydSteinberg,
    /// 1-bit: Atkinson diffusion.
    Atkinson1,
}

impl Method {
    /// Bits per output pixel.
    #[must_use]
    pub fn bits(self) -> usize {
        match self {
            Method::Atkinson1 => 1,
            _ => 2,
        }
    }

    /// Error values a row of `width` pixels needs carried (0: none).
    #[must_use]
    pub fn error_len(self, width: usize) -> usize {
        match self {
            Method::Atkinson | Method::Atkinson1 => width.saturating_add(4).saturating_mul(3),
            Method::FloydSteinberg => width.saturating_add(2).saturating_mul(2),
            _ => 0,
        }
    }
}

// lib/GfxRenderer's brightness, contrast and gamma adjustments, all off as there.
const USE_BRIGHTNESS: bool = false;
const BRIGHTNESS_BOOST: i32 = 10;
const GAMMA_CORRECTION: bool = false;
const CONTRAST_PERCENT: i32 = 115;
const USE_NOISE_DITHERING: bool = false;

/// `gray` after the brightness, contrast and gamma adjustments.
#[must_use]
pub fn adjust(gray: u8) -> u8 {
    if !USE_BRIGHTNESS {
        return gray;
    }
    let contrast = ((i32::from(gray) - 128) * CONTRAST_PERCENT / 100 + 128).clamp(0, 255);
    let bright = (contrast + BRIGHTNESS_BOOST).clamp(0, 255);
    let out = if GAMMA_CORRECTION && bright > 0 {
        // Two Newton steps of sqrt(gray * 255): brightens dark and mid tones.
        let product = bright * 255;
        let mut x = bright;
        x = (x + product / x) >> 1;
        x = (x + product / x) >> 1;
        x.min(255)
    } else {
        bright
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out = out as u8;
    out
}

/// A pseudo-random threshold (0 to 255) for pixel (`x`, `y`) that does not alias when scaled.
fn noise(x: i32, y: i32) -> i32 {
    #[allow(clippy::cast_sign_loss)]
    let mut hash = (x as u32)
        .wrapping_mul(374_761_393)
        .wrapping_add((y as u32).wrapping_mul(668_265_263));
    hash = (hash ^ (hash >> 13)).wrapping_mul(1_274_126_177);
    #[allow(clippy::cast_possible_wrap)]
    let threshold = (hash >> 24) as i32;
    threshold
}

/// 2-bit level by fixed thresholds tuned to the panel (or noise, when that is on).
fn threshold(gray: u8, x: i32, y: i32) -> u8 {
    let gray = i32::from(gray);
    if USE_NOISE_DITHERING {
        let (scaled, t) = (gray * 3, noise(x, y));
        return if scaled < 255 {
            u8::from(scaled + t >= 255)
        } else if scaled < 510 {
            if scaled - 255 + t >= 255 { 2 } else { 1 }
        } else if scaled - 510 + t >= 255 {
            3
        } else {
            2
        };
    }
    match gray {
        ..45 => 0,
        45..70 => 1,
        70..140 => 2,
        _ => 3,
    }
}

/// The 2-bit level of `gray` and the gray value that level shows, for the diffusing methods.
fn level(gray: i32, original_thresholds: bool) -> (u8, i32) {
    if original_thresholds {
        match gray {
            ..43 => (0, 0),
            43..128 => (1, 85),
            128..213 => (2, 170),
            _ => (3, 255),
        }
    } else {
        // Legacy panel tuning: slightly darker midtones.
        match gray {
            ..30 => (0, 15),
            30..55 => (1, 35),
            55..150 => (2, 90),
            _ => (3, 210),
        }
    }
}

/// Error diffusion state for one image, carried from row to row. The error rows belong to the
/// caller; with none (too short), the diffusing methods quantize without carrying error.
pub struct Ditherer<'a> {
    pub method: Method,
    pub original_thresholds: bool,
    /// [`Method::error_len`] values for the row width.
    pub errors: &'a mut [i16],
    /// Rows finished since the start (or the last [`Ditherer::reset`]).
    pub rows_done: &'a mut u32,
}

impl Ditherer<'_> {
    /// The error rows, the current one first, as they have rotated with the rows finished;
    /// none when there are not enough values for them.
    fn rows(&mut self) -> Option<[&mut [i16]; 3]> {
        let count: u32 = match self.method {
            Method::Atkinson | Method::Atkinson1 => 3,
            Method::FloydSteinberg => 2,
            _ => return None,
        };
        let len = self.errors.len() / count as usize;
        if len == 0 {
            return None;
        }
        let mut chunks = self.errors.chunks_exact_mut(len);
        let (a, b) = (chunks.next()?, chunks.next()?);
        let c = if count == 3 { chunks.next()? } else { &mut [] };
        Some(match (self.rows_done.wrapping_rem(count), count) {
            (0, _) => [a, b, c],
            (1, 2) => [b, a, c],
            (1, _) => [b, c, a],
            _ => [c, a, b],
        })
    }

    /// Writes the levels of `gray` (pixels `x0` on of row `y`, adjusted here) into `out`, a row
    /// packed most significant bit first at the method's bits per pixel; other pixels' bits are
    /// left as they are.
    pub fn pixels(&mut self, gray: &[u8], x0: usize, y: i32, out: &mut [u8]) {
        let original = self.original_thresholds;
        let odd_row = *self.rows_done & 1 != 0;
        let method = self.method;
        let rows = self.rows();
        match (method, rows) {
            (Method::Atkinson, Some([r0, r1, r2])) => {
                atkinson::<2>(gray, x0, out, [r0, r1, r2], |a| level(a, original));
            }
            (Method::Atkinson1, Some([r0, r1, r2])) => {
                let one = |a| if a < 128 { (0, 0) } else { (1, 255) };
                atkinson::<1>(gray, x0, out, [r0, r1, r2], one);
            }
            (Method::FloydSteinberg, Some([cur, next, _])) => {
                floyd_steinberg(gray, x0, out, [cur, next], odd_row, original);
            }
            (Method::Atkinson | Method::FloydSteinberg, None) => {
                plain::<2>(gray, x0, out, |g, _| level(i32::from(g), original).0);
            }
            (Method::Atkinson1, None) => plain::<1>(gray, x0, out, |g, _| u8::from(g >= 128)),
            (Method::Threshold, _) => plain::<2>(gray, x0, out, |g, x| threshold(g, x, y)),
            (Method::Native, _) => plain::<2>(gray, x0, out, |g, _| g >> 6),
        }
    }

    /// Moves on to the next row: its carried error becomes current, and a cleared row follows.
    /// `floyd_steinberg` relies on the finished row being cleared here (it does not keep what odd
    /// rows send back to the left).
    pub fn next_row(&mut self) {
        if let Some([done, ..]) = self.rows() {
            done.fill(0);
        }
        *self.rows_done = self.rows_done.wrapping_add(1);
    }

    /// Starts over: no carried error, and row 0 again.
    pub fn reset(&mut self) {
        self.errors.fill(0);
        *self.rows_done = 0;
    }
}

/// Sets pixel `x`'s `BITS` bits of the packed row `out` to `value`.
#[allow(clippy::inline_always)]
#[inline(always)]
fn put<const BITS: usize>(out: &mut [u8], x: usize, value: u8) {
    let per_byte = 8 / BITS;
    #[allow(clippy::cast_possible_truncation)]
    let shift = (8 - BITS - (x % per_byte) * BITS) as u32;
    let mask = ((1u8 << BITS) - 1) << shift;
    if let Some(byte) = out.get_mut(x / per_byte) {
        *byte = (*byte & !mask) | ((value << shift) & mask);
    }
}

/// `count` pixels from `x0`, each level only from its own gray and position.
fn plain<const BITS: usize>(gray: &[u8], x0: usize, out: &mut [u8], level: impl Fn(u8, i32) -> u8) {
    for (i, &g) in gray.iter().enumerate() {
        let x = x0.saturating_add(i);
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        put::<BITS>(out, x, level(adjust(g), x as i32));
    }
}

/// Adds `error` to an error value, wrapping as the old C++ int16 did.
#[allow(clippy::inline_always)]
#[inline(always)]
fn add(e: &mut i16, error: i32) {
    #[allow(clippy::cast_possible_truncation)]
    let sum = (i32::from(*e) + error) as i16;
    *e = sum;
}

/// Atkinson diffusion at `BITS` bits per pixel: rows `r` from the current one; `quantize` gives an
/// adjusted gray's level and the gray it shows. 1/8 of the error goes to each of six neighbours:
/// two to the right, three below and one two rows down.
///
/// The loop walks the rows with the pixels instead of indexing them (no bounds checks): what goes
/// to the right waits in `ahead` until that pixel comes, what goes below is added once the column
/// has all of it, and what is still waiting at the end goes back into the rows for the next piece
/// of the row. Carried values stay far inside i16 (under 1000), so adding sums once matches the
/// old code's adding each part.
fn atkinson<const BITS: usize>(
    gray: &[u8],
    x0: usize,
    out: &mut [u8],
    [r0, r1, r2]: [&mut [i16]; 3],
    quantize: impl Fn(i32) -> (u8, i32),
) {
    let n = gray.len();
    let end = x0.saturating_add(n);
    // Error carried into x + 2 (two pixels ahead and on), x + 1 below, x + 2 two rows down.
    let here = r0.get_mut(x0.saturating_add(2)..end.saturating_add(4));
    let below = r1.get_mut(x0.saturating_add(1)..end.saturating_add(3));
    let two_down = r2.get_mut(x0.saturating_add(2)..end.saturating_add(2));
    let (Some((here, here_rest)), Some((below, below_rest)), Some(two_down)) = (
        here.and_then(|h| h.split_at_mut_checked(n)),
        below.and_then(|b| b.split_at_mut_checked(n)),
        two_down,
    ) else {
        // Pixels past the row: levels only, nothing carried.
        return plain::<BITS>(gray, x0, out, |g, _| quantize(i32::from(g)).0);
    };
    // Errors waiting for pixel x + 2 and x + 3 of this row, and for x + 2 and x + 3 below.
    let (mut ahead, mut ahead2, mut under, mut under2) = (0, 0, 0, 0);
    let pixels = gray
        .iter()
        .zip(here.iter())
        .zip(below.iter_mut())
        .zip(two_down.iter_mut());
    for (x, (((&g, &carried), below), two_down)) in (x0..).zip(pixels) {
        let adjusted = (i32::from(adjust(g)) + i32::from(carried) + ahead).clamp(0, 255);
        let (value, shown) = quantize(adjusted);
        put::<BITS>(out, x, value);
        let error = (adjusted - shown) >> 3;
        (ahead, ahead2) = (ahead2 + error, error);
        add(below, under + error);
        (under, under2) = (under2 + error, error);
        add(two_down, error);
    }
    if let [a, b] = here_rest {
        add(a, ahead);
        add(b, ahead2);
    }
    if let [a, b] = below_rest {
        add(a, under);
        add(b, under2);
    }
}

/// Floyd-Steinberg diffusion on rows `cur` and `next`: 7/16 to the right, and 3/16, 5/16, 1/16
/// below from the left; mirrored on odd rows. Walked like `atkinson`. On odd rows the 7/16 goes to
/// the pixel already done on the left, which nothing reads again (the row is cleared next), so it
/// is not kept.
fn floyd_steinberg(
    gray: &[u8],
    x0: usize,
    out: &mut [u8],
    [cur, next]: [&mut [i16]; 2],
    odd_row: bool,
    original: bool,
) {
    let n = gray.len();
    let end = x0.saturating_add(n);
    // Error carried into x + 1 of this row and x of the next.
    let here = cur.get_mut(x0.saturating_add(1)..end.saturating_add(2));
    let below = next.get_mut(x0..end.saturating_add(2));
    let (Some((here, here_rest)), Some((below, below_rest))) = (
        here.and_then(|h| h.split_at_mut_checked(n)),
        below.and_then(|b| b.split_at_mut_checked(n)),
    ) else {
        // Pixels past the row: levels only, nothing carried.
        return plain::<2>(gray, x0, out, |g, _| level(i32::from(g), original).0);
    };
    // Below, from the left: 3/16, 5/16, 1/16, or mirrored.
    let (w0, w2) = if odd_row { (1, 3) } else { (3, 1) };
    // Error waiting for the next pixel of this row, and for x and x + 1 below.
    let (mut ahead, mut under, mut under2) = (0, 0, 0);
    let pixels = gray.iter().zip(here.iter()).zip(below.iter_mut());
    for (x, ((&g, &carried), below)) in (x0..).zip(pixels) {
        let adjusted = (i32::from(adjust(g)) + i32::from(carried) + ahead).clamp(0, 255);
        let (value, shown) = level(adjusted, original);
        put::<2>(out, x, value);
        let error = adjusted - shown;
        ahead = if odd_row { 0 } else { (error * 7) >> 4 };
        add(below, under + ((error * w0) >> 4));
        (under, under2) = (under2 + ((error * 5) >> 4), (error * w2) >> 4);
    }
    if let [a] = here_rest {
        add(a, ahead);
    }
    if let [a, b] = below_rest {
        add(a, under);
        add(b, under2);
    }
}

mod sample;
mod scale;
pub use sample::{GrayBlock, Line, Quantize, Scale, block_row, line_row, put_levels};
pub use scale::{area_add, area_pixels, blend_rows, linear_row};

#[cfg(test)]
mod tests;
