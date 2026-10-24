//! Glyphs drawn at another size, pixel by pixel: shrunk (superscripts, subscripts and small caps)
//! by sampling the source block each pixel covers, and grown (drop caps) by bilinear
//! interpolation.

use super::{Bitmap, Canvas};

/// Bounds far beyond any font glyph that keep the arithmetic below in i32.
const MAX_SIDE: i32 = 1 << 12;
// Chapters store drop cap scales as at most 4096 (16 times).
const MAX_SCALE: i32 = 1 << 12;

impl Bitmap<'_> {
    fn in_bounds(&self) -> bool {
        (0..=MAX_SIDE).contains(&self.width) && (0..=MAX_SIDE).contains(&self.height)
    }

    /// Pixel (`x`, `y`)'s coverage: 0 (white) to 3 (black), 1-bit ink as 3; 0 outside the bitmap.
    fn coverage(&self, x: i32, y: i32) -> i32 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0;
        }
        // Both are inside the bitmap, so not negative.
        #[allow(clippy::cast_sign_loss)]
        let at = (y as usize)
            .wrapping_mul(self.width as usize)
            .wrapping_add(x as usize);
        if self.two_bit {
            #[allow(clippy::cast_possible_truncation)]
            let shift = (3 - (at & 3) as u32) * 2;
            i32::from((self.bits.get(at >> 2).copied().unwrap_or(0) >> shift) & 3)
        } else {
            #[allow(clippy::cast_possible_truncation)]
            let shift = 7 - (at & 7) as u32;
            if (self.bits.get(at >> 3).copied().unwrap_or(0) >> shift) & 1 != 0 {
                3
            } else {
                0
            }
        }
    }
}

impl Canvas<'_> {
    /// Draws `glyph` shrunk by `num`/`den` (meant for at most 1) with its top left at (`x`, `y`), in black
    /// or white. Each pixel covers a block of source pixels: at one half any ink there draws (2-bit
    /// glyphs: dark gray or more, or two light pixels), so thin strokes survive; milder reductions
    /// need majority coverage so stems don't thicken. Returns how many pixels were off the panel;
    /// negative sizes, sizes over 4096 and factors outside 1 to 4096 draw nothing.
    pub fn reduced_glyph(
        &mut self,
        glyph: &Bitmap,
        (num, den): (i32, i32),
        (x, y): (i32, i32),
        black: bool,
    ) -> u32 {
        if !glyph.in_bounds() || !(1..=MAX_SIDE).contains(&num) || !(1..=MAX_SIDE).contains(&den) {
            return 0;
        }
        // Rounded up so edge columns aren't lost.
        let width = (glyph.width * num + den - 1) / den;
        let height = (glyph.height * num + den - 1) / den;
        let halving = num * 2 == den;
        let mut off = 0u32;
        for dy in 0..height {
            let y0 = dy * den / num;
            let y1 = glyph
                .height
                .min((y0 + 1).max(((dy + 1) * den + num - 1) / num));
            for dx in 0..width {
                let x0 = dx * den / num;
                let x1 = glyph
                    .width
                    .min((x0 + 1).max(((dx + 1) * den + num - 1) / num));
                let (mut sum, mut darkest, mut samples) = (0, 0, 0);
                for sy in y0..y1 {
                    for sx in x0..x1 {
                        let c = glyph.coverage(sx, sy);
                        sum += c;
                        darkest = darkest.max(c);
                        samples += 1;
                    }
                }
                let ink = if halving {
                    if glyph.two_bit {
                        darkest >= 2 || sum >= 2
                    } else {
                        darkest != 0
                    }
                } else {
                    samples > 0 && sum * 2 >= samples * 3
                };
                if ink && !self.pixel(x.wrapping_add(dx), y.wrapping_add(dy), black) {
                    off = off.wrapping_add(1);
                }
            }
        }
        off
    }

    /// Draws `glyph` scaled by `scale256` / 256 in black with its top left at (`x`, `y`): each
    /// pixel's centre is mapped back onto the glyph and inked where the bilinear coverage of its
    /// four neighbours reaches half of black. Returns how many pixels were off the panel; sizes
    /// over 4096, or a scale under 1 or over 4096 (16 times), draw nothing.
    pub fn resampled_glyph(&mut self, glyph: &Bitmap, scale256: i32, (x, y): (i32, i32)) -> u32 {
        if !glyph.in_bounds() || !(1..=MAX_SCALE).contains(&scale256) {
            return 0;
        }
        let width = (glyph.width * scale256 + 255) / 256;
        let height = (glyph.height * scale256 + 255) / 256;
        // Source position of each pixel centre, 16.16 fixed point.
        let step = (256 << 16) / scale256;
        let origin = step / 2 - (1 << 15);
        let mut off = 0u32;
        for dy in 0..height {
            let sy = origin + dy * step;
            let (y0, fy) = (sy >> 16, (sy >> 8) & 0xFF);
            for dx in 0..width {
                let sx = origin + dx * step;
                let (x0, fx) = (sx >> 16, (sx >> 8) & 0xFF);
                let upper = glyph.coverage(x0, y0) * (256 - fx) + glyph.coverage(x0 + 1, y0) * fx;
                let lower =
                    glyph.coverage(x0, y0 + 1) * (256 - fx) + glyph.coverage(x0 + 1, y0 + 1) * fx;
                if (upper * (256 - fy) + lower * fy) * 2 >= 3 * 256 * 256
                    && !self.pixel(x.wrapping_add(dx), y.wrapping_add(dy), true)
                {
                    off = off.wrapping_add(1);
                }
            }
        }
        off
    }
}
