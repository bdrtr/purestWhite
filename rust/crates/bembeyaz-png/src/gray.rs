//! A decoded row as gray (0 black to 255 white) for drawing: each pixel's luma and alpha, the alpha
//! kept apart for images drawn over the screen or else composited onto white. A tRNS color key
//! makes its pixels clear.

use super::{ALPHA, Row, pixel};

/// The drawing converter's luma: 77, 150 and 29 parts in 256.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    #[allow(clippy::cast_possible_truncation, reason = "at most 255")]
    let luma = ((u32::from(r) * 77 + u32::from(g) * 150 + u32::from(b) * 29) >> 8) as u8;
    luma
}

impl Row<'_> {
    /// Writes the gray of the row's pixels (as many as `gray` holds, at most the row's width) into
    /// `gray`, and their alpha into `alpha`; without `alpha`, the gray is composited onto white.
    /// Samples past the row read as 0; unknown pixel types are mid gray.
    pub fn to_gray(&self, gray: &mut [u8], alpha: Option<&mut [u8]>) {
        let width = usize::try_from(self.width).unwrap_or(0).min(gray.len());
        let gray = gray.get_mut(..width).unwrap_or_default();
        let depth = match self.bpp {
            1 | 2 | 4 => self.bpp,
            _ => 8,
        };
        let key = self.transparent;
        match self.pixel_type {
            pixel::GRAYSCALE => {
                if depth == 8 && !self.has_alpha && alpha.is_none() {
                    // Already gray and opaque.
                    for (out, &v) in gray.iter_mut().zip(self.pixels) {
                        *out = v;
                    }
                    return;
                }
                store(gray, alpha, |x| {
                    let sample = self.sample(x, depth);
                    #[allow(clippy::cast_possible_truncation, reason = "the key's low byte")]
                    let clear = self.has_alpha && sample == key as u8;
                    (expand(sample, depth), if clear { 0 } else { 255 })
                });
            }
            pixel::TRUECOLOR => {
                let (pixels, _) = self.pixels.as_chunks::<3>();
                store(gray, alpha, |x| {
                    let [r, g, b] = pixels.get(x).copied().unwrap_or_default();
                    let color = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
                    let clear = self.has_alpha && color == key;
                    (luma(r, g, b), if clear { 0 } else { 255 })
                });
            }
            pixel::INDEXED => {
                let (entries, _) = self.palette.as_chunks::<3>();
                store(gray, alpha, |x| {
                    let index = usize::from(self.sample(x, depth));
                    let [r, g, b] = entries.get(index).copied().unwrap_or_default();
                    let a = if self.has_alpha {
                        self.palette.get(ALPHA + index).copied().unwrap_or(0)
                    } else {
                        255
                    };
                    (luma(r, g, b), a)
                });
            }
            pixel::GRAY_ALPHA => {
                let (pixels, _) = self.pixels.as_chunks::<2>();
                store(gray, alpha, |x| {
                    let [v, a] = pixels.get(x).copied().unwrap_or_default();
                    (v, a)
                });
            }
            pixel::TRUECOLOR_ALPHA => {
                let (pixels, _) = self.pixels.as_chunks::<4>();
                store(gray, alpha, |x| {
                    let [r, g, b, a] = pixels.get(x).copied().unwrap_or_default();
                    (luma(r, g, b), a)
                });
            }
            _ => store(gray, alpha, |_| (128, 255)),
        }
    }

    /// Sample `x` of a row packed at `depth` (1, 2, 4 or 8) bits, most significant bit first.
    fn sample(&self, x: usize, depth: u8) -> u8 {
        let bit = x.wrapping_mul(usize::from(depth));
        #[allow(clippy::cast_possible_truncation, reason = "below 8")]
        let shift = (8 - u32::from(depth)).wrapping_sub((bit & 7) as u32) & 7;
        let mask = u8::try_from((1u16 << depth) - 1).unwrap_or(u8::MAX);
        self.pixels
            .get(bit >> 3)
            .map_or(0, |&byte| (byte >> shift) & mask)
    }
}

/// A `depth`-bit sample scaled to 0 to 255: times 255 over its largest value, which divides 255.
fn expand(sample: u8, depth: u8) -> u8 {
    let step = match depth {
        1 => 255,
        2 => 85,
        4 => 17,
        _ => 1,
    };
    sample.wrapping_mul(step)
}

/// Writes each pixel's `(gray, alpha)` from `pixel` into `gray` and `alpha`, or without `alpha`
/// its gray composited onto white.
fn store(gray: &mut [u8], alpha: Option<&mut [u8]>, pixel: impl Fn(usize) -> (u8, u8)) {
    match alpha {
        Some(alpha) => {
            for (x, (g, a)) in gray.iter_mut().zip(alpha.iter_mut()).enumerate() {
                (*g, *a) = pixel(x);
            }
        }
        None => {
            for (x, g) in gray.iter_mut().enumerate() {
                let (v, a) = pixel(x);
                let (v, a) = (u32::from(v), u32::from(a));
                #[allow(clippy::cast_possible_truncation, reason = "a mix of two grays")]
                let mixed = ((v * a + 255 * (255 - a)) / 255) as u8;
                *g = mixed;
            }
        }
    }
}
