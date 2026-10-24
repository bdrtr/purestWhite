//! Unscaled font glyphs: clipping, orientation and framebuffer addressing are resolved once per
//! glyph, as in the old `GlyphBitmap.h`; then each pixel is one step: to the next bit along a
//! panel row, or (portrait text) to the same bit of the next row's byte.

use super::{Canvas, Orientation, Plane};

/// A packed glyph bitmap: rows back to back, most significant bit first, 1 or 2 bits per pixel.
#[derive(Clone, Copy)]
pub struct Bitmap<'a> {
    pub bits: &'a [u8],
    pub width: i32,
    pub height: i32,
    pub two_bit: bool,
}

/// Where glyph pixel (0, 0) lands and the step for one move along the glyph's x and y axes, all in
/// one coordinate space. The axes must be an orthogonal unit rotation: each is +/-1 on exactly one
/// component, and they differ in which; any other frame draws nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub x: i32,
    pub y: i32,
    pub dx_x: i32,
    pub dx_y: i32,
    pub dy_x: i32,
    pub dy_y: i32,
}

/// A half-open rectangle in glyph pixels.
#[derive(Clone, Copy)]
struct Span {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Span {
    fn is_empty(&self) -> bool {
        self.left >= self.right || self.top >= self.bottom
    }
}

/// Narrows `start..end` on one glyph axis so that `base + i * step` stays in `lower..upper`.
fn clip_axis(base: i32, step: i32, lower: i32, upper: i32, start: &mut i32, end: &mut i32) {
    if step > 0 {
        *start = (*start).max(lower.wrapping_sub(base));
        *end = (*end).min(upper.wrapping_sub(base));
    } else {
        *start = (*start).max(base.wrapping_sub(upper).wrapping_add(1));
        *end = (*end).min(base.wrapping_sub(lower).wrapping_add(1));
    }
}

/// Narrows `span` so every pixel left maps into `left..right` x `top..bottom` of the space
/// `frame` is in; which glyph axis runs along that space's x depends on the rotation.
fn clip_to_rect(frame: &Frame, left: i32, top: i32, right: i32, bottom: i32, span: &mut Span) {
    if frame.dx_x != 0 {
        clip_axis(
            frame.x,
            frame.dx_x,
            left,
            right,
            &mut span.left,
            &mut span.right,
        );
        clip_axis(
            frame.y,
            frame.dy_y,
            top,
            bottom,
            &mut span.top,
            &mut span.bottom,
        );
    } else {
        clip_axis(
            frame.x,
            frame.dy_x,
            left,
            right,
            &mut span.top,
            &mut span.bottom,
        );
        clip_axis(
            frame.y,
            frame.dx_y,
            top,
            bottom,
            &mut span.left,
            &mut span.right,
        );
    }
}

/// Calls `pixel` with the value of each of `count` pixels from pixel `start` on (2 bits per pixel
/// when `TWO_BIT`, else 1, most significant first), reading each byte once; whole 2-bit bytes
/// are unrolled so every shift is a constant.
#[allow(clippy::inline_always)]
#[inline(always)]
fn each_pixel<const TWO_BIT: bool>(
    bits: &[u8],
    start: usize,
    count: usize,
    mut pixel: impl FnMut(u8),
) {
    let (per_byte, size) = if TWO_BIT { (4, 2) } else { (8, 1) };
    let mut byte = start / per_byte;
    let mut left = count;
    // A partial first byte.
    let slot = start % per_byte;
    if slot != 0 {
        let take = (per_byte - slot).min(left);
        // slot * size is under 8.
        #[allow(clippy::cast_possible_truncation)]
        let mut packed = bits.get(byte).copied().unwrap_or(0) << (slot * size) as u32;
        for _ in 0..take {
            pixel(packed >> (8 - size));
            packed <<= size;
        }
        left -= take;
        byte += 1;
    }
    while left >= per_byte {
        let packed = bits.get(byte).copied().unwrap_or(0);
        if TWO_BIT {
            pixel(packed >> 6);
            pixel((packed >> 4) & 3);
            pixel((packed >> 2) & 3);
            pixel(packed & 3);
        } else {
            // 1-bit glyphs are rare (2-bit fonts are the default): not unrolled.
            let mut packed = packed;
            for _ in 0..8 {
                pixel(packed >> 7);
                packed <<= 1;
            }
        }
        left -= per_byte;
        byte += 1;
    }
    // A partial last byte.
    let mut packed = bits.get(byte).copied().unwrap_or(0);
    for _ in 0..left {
        pixel(packed >> (8 - size));
        packed <<= size;
    }
}

/// Whether a frame's axes are an orthogonal unit rotation, as `Canvas::glyph` needs.
fn orthogonal(frame: &Frame) -> bool {
    let unit = |a: i32, b: i32| matches!((a, b), (0, 1 | -1) | (1 | -1, 0));
    unit(frame.dx_x, frame.dx_y)
        && unit(frame.dy_x, frame.dy_y)
        && (frame.dx_x == 0) != (frame.dy_x == 0)
}

/// Whether a source value is drawn: `LEVELS` has bit n set for value n.
#[allow(clippy::inline_always)]
#[inline(always)]
fn drawn<const LEVELS: u8>(ink: u8) -> bool {
    match LEVELS {
        // Any ink: black and white, and 1-bit glyphs.
        0x0e | 0x02 => ink != 0,
        // Light and dark gray: the MSB plane.
        0x06 => ink.wrapping_sub(1) < 2,
        // Dark gray: the LSB plane.
        0x04 => ink == 2,
        _ => (LEVELS >> ink) & 1 != 0,
    }
}

impl Canvas<'_> {
    /// Draws `glyph`, whose pixel (0, 0) and axes `frame` places in logical coordinates, clipped
    /// to the clip, the panel and the strip.
    ///
    /// 2-bit values are 0 white, 1 light gray, 2 dark gray, 3 black; 1-bit value 1 is ink.
    /// In black and white every non-white value is drawn, black or white as `black` says; the
    /// gray planes only set bits (light and dark gray for the MSB plane, dark gray for the LSB
    /// plane). 1-bit glyphs draw their ink as `black` says in every plane.
    pub fn glyph(&mut self, glyph: &Bitmap, frame: &Frame, plane: Plane, black: bool) {
        let Bitmap {
            bits,
            width,
            height,
            two_bit,
        } = *glyph;
        if !orthogonal(frame) {
            return;
        }
        // The glyph, narrowed to the logical clip before rotating.
        let mut span = Span {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        let c = self.clip;
        clip_to_rect(frame, c.left, c.top, c.right, c.bottom, &mut span);

        // One orthogonal transform for text rotation and panel orientation: the origin and both
        // glyph axes, on the panel.
        let (x, y) = self.to_panel(frame.x, frame.y);
        let rotate = |a: i32, b: i32| match self.orientation {
            Orientation::Portrait => (b, a.wrapping_neg()),
            Orientation::LandscapeClockwise => (a.wrapping_neg(), b.wrapping_neg()),
            Orientation::PortraitInverted => (b.wrapping_neg(), a),
            Orientation::LandscapeCounterClockwise => (a, b),
        };
        let (along, down) = (
            rotate(frame.dx_x, frame.dx_y),
            rotate(frame.dy_x, frame.dy_y),
        );
        let panel = Frame {
            x,
            y,
            dx_x: along.0,
            dx_y: along.1,
            dy_x: down.0,
            dy_y: down.1,
        };

        // The span starts as the glyph and only narrows: now to the panel width and the rows held.
        let end_row = self.origin_y.wrapping_add(self.rows);
        clip_to_rect(
            &panel,
            0,
            self.origin_y,
            self.panel_width,
            end_row,
            &mut span,
        );
        if span.is_empty() {
            return;
        }

        // The buffer as one bit index: a glyph column moves step_x bits, a glyph row step_y
        // bits; each is +/-1 along a panel row or +/-stride_bits across rows.
        let stride_bits = i32::try_from(self.stride.saturating_mul(8)).unwrap_or(i32::MAX);
        let step_x = panel
            .dx_y
            .wrapping_mul(stride_bits)
            .wrapping_add(panel.dx_x);
        let step_y = panel
            .dy_y
            .wrapping_mul(stride_bits)
            .wrapping_add(panel.dy_x);
        let row_bit = panel
            .y
            .wrapping_sub(self.origin_y)
            .wrapping_mul(stride_bits)
            .wrapping_add(panel.x)
            .wrapping_add(span.left.wrapping_mul(step_x))
            .wrapping_add(span.top.wrapping_mul(step_y));
        // One loop per bit depth and drawn values: no per-pixel branch on either. In black and
        // white every non-white value is drawn as `black` says; the gray planes only set bits;
        // 1-bit glyphs draw their ink as `black` says in every plane.
        let black_ink = if black { 0x00 } else { 0xFF };
        let blit = Blit {
            buf: &mut *self.buf,
            bits,
            width,
            span,
            row_bit,
            step_x,
            step_y,
            along_row: panel.dx_x != 0,
            ink: black_ink,
        };
        match (two_bit, plane) {
            (false, _) => blit.run::<false, 0x02>(),
            (true, Plane::Bw) => blit.run::<true, 0x0e>(),
            (true, Plane::GrayMsb) => Blit { ink: 0xFF, ..blit }.run::<true, 0x06>(),
            (true, Plane::GrayLsb) => Blit { ink: 0xFF, ..blit }.run::<true, 0x04>(),
        }
    }
}

/// A glyph's pixels and where they go: `span` of the glyph (which is `width` wide), pixel
/// (`span.left`, `span.top`) at bit `row_bit` of `buf`, one glyph column `step_x` bits on and one
/// row `step_y`; `along_row` when a glyph row runs along a panel row (`step_x` is +/-1) rather
/// than across rows (a multiple of 8). Drawn values take `ink`: 0x00 black, 0xFF white.
struct Blit<'a> {
    buf: &'a mut [u8],
    bits: &'a [u8],
    width: i32,
    span: Span,
    row_bit: i32,
    step_x: i32,
    step_y: i32,
    along_row: bool,
    ink: u8,
}

impl Blit<'_> {
    /// Draws the 2-bit or 1-bit pixels whose values `LEVELS` has (bit n for value n).
    fn run<const TWO_BIT: bool, const LEVELS: u8>(self) {
        let Blit {
            buf,
            bits,
            width,
            span,
            mut row_bit,
            step_x,
            step_y,
            along_row,
            ink,
        } = self;
        // At least 0 and at most the glyph's size here.
        #[allow(clippy::cast_sign_loss)]
        let (left, glyph_width, count) = (
            span.left as usize,
            width as usize,
            (span.right - span.left) as usize,
        );
        for y in span.top..span.bottom {
            #[allow(clippy::cast_sign_loss)]
            let source = (y as usize).wrapping_mul(glyph_width).wrapping_add(left);
            if along_row {
                // Neighbouring bits. A negative index (never after clipping) wraps to one
                // get_mut refuses.
                #[allow(clippy::cast_sign_loss)]
                let (step, mut at) = (step_x as usize, row_bit as usize);
                each_pixel::<TWO_BIT>(bits, source, count, |value| {
                    if drawn::<LEVELS>(value)
                        && let Some(b) = buf.get_mut(at >> 3)
                    {
                        let mask = 0x80u8 >> (at & 7);
                        *b = (*b & !mask) | (mask & ink);
                    }
                    at = at.wrapping_add(step);
                });
            } else {
                // Portrait text: the same bit of every stride-th byte. A negative index (never
                // after clipping) wraps to one get_mut refuses.
                let mask = 0x80u8 >> (row_bit & 7);
                let (keep, set) = (!mask, mask & ink);
                #[allow(clippy::cast_sign_loss)]
                let (step, mut byte) = ((step_x >> 3) as usize, (row_bit >> 3) as usize);
                each_pixel::<TWO_BIT>(bits, source, count, |value| {
                    if drawn::<LEVELS>(value)
                        && let Some(b) = buf.get_mut(byte)
                    {
                        *b = (*b & keep) | set;
                    }
                    byte = byte.wrapping_add(step);
                });
            }
            row_bit = row_bit.wrapping_add(step_y);
        }
    }
}
