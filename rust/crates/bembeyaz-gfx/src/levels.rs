//! Rows of 2-bit gray levels (0 black to 3 white, four to a byte, most significant first: the
//! chapter image cache's format) drawn onto the canvas. Orientation is resolved once per row and
//! only the columns that land on the panel and in the strip are visited, each one bit step from the
//! last. The clip does not apply: callers keep images on the screen.

use super::{Canvas, Orientation, Plane};

/// How levels become ink in a plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ink {
    pub plane: Plane,
    /// Gray planes hold absolute levels (both inks written) rather than differences.
    pub absolute: bool,
}

impl Ink {
    /// Level `level`'s ink: black, white, or none (white in black and white is left as it is).
    #[must_use]
    pub fn of(self, level: u8) -> Option<bool> {
        let msb = match self.plane {
            Plane::Bw => return (level < 3).then_some(true),
            Plane::GrayLsb => false,
            Plane::GrayMsb => true,
        };
        // BitmapHelpers.h's grayPlanePixel, still used in C++: change both.
        let mid = if msb { 2 } else { 1 };
        if self.absolute {
            Some(!(level == 3 || level == mid))
        } else {
            let write = if msb {
                level == 1 || level == 2
            } else {
                level == 1
            };
            write.then_some(false)
        }
    }
}

/// The columns `c` (of `0..width`) for which `base + c * step` is in `lo..hi`, `step` being -1, 0
/// or 1.
fn columns(base: i64, step: i64, lo: i64, hi: i64, width: i64) -> (i64, i64) {
    match step {
        0 if (lo..hi).contains(&base) => (0, width),
        0 => (0, 0),
        1 => ((lo - base).max(0), (hi - base).min(width)),
        _ => ((base - hi + 1).max(0), (base - lo + 1).min(width)),
    }
}

impl Canvas<'_> {
    /// Draws the first `width` levels of `row` (packed, four to a byte) along logical row `y`
    /// from column `x`, as `ink` says.
    pub fn level_row(&mut self, row: &[u8], width: i32, x: i32, y: i32, ink: Ink) {
        if width <= 0 {
            return;
        }
        let (px, py) = self.to_panel(x, y);
        // One logical step right, on the panel.
        let (sx, sy) = match self.orientation {
            Orientation::Portrait => (0, -1),
            Orientation::LandscapeClockwise => (-1, 0),
            Orientation::PortraitInverted => (0, 1),
            Orientation::LandscapeCounterClockwise => (1, 0),
        };
        let width = i64::from(width);
        let (px, py) = (i64::from(px), i64::from(py));
        let origin = i64::from(self.origin_y);
        let (x0, x1) = columns(px, sx, 0, i64::from(self.panel_width), width);
        let (y0, y1) = columns(py, sy, origin, origin + i64::from(self.rows), width);
        let (first, end) = (x0.max(y0), x1.min(y1));
        if first >= end {
            return;
        }
        let inks = [ink.of(0), ink.of(1), ink.of(2), ink.of(3)];
        let Ok(stride_bits) = i64::try_from(self.stride.saturating_mul(8)) else {
            return;
        };
        // The bit of column `first`, and the bits one column moves.
        let start = (py + first * sy - origin) * stride_bits + px + first * sx;
        let step = sy * stride_bits + sx;
        let (Ok(mut at), Ok(first), Ok(end)) = (
            usize::try_from(start),
            usize::try_from(first),
            usize::try_from(end),
        ) else {
            return;
        };
        // All columns here are on the panel and in the strip: `at` stays in the rows.
        #[allow(clippy::cast_possible_truncation)]
        let step = step as isize;
        for column in first..end {
            let byte = row.get(column / 4).copied().unwrap_or(0xFF);
            #[allow(clippy::cast_possible_truncation)]
            let level = (byte >> (6 - (column % 4) as u32 * 2)) & 3;
            if let Some(Some(black)) = inks.get(usize::from(level))
                && let Some(b) = self.buf.get_mut(at >> 3)
            {
                let mask = 0x80u8 >> (at & 7);
                if *black {
                    *b &= !mask;
                } else {
                    *b |= mask;
                }
            }
            at = at.wrapping_add_signed(step);
        }
    }
}
