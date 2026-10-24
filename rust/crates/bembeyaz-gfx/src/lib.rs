//! Drawing for lib/GfxRenderer into the e-ink framebuffer: one bit per pixel, most significant
//! bit first, 0 for black, in the panel's own rows; callers draw in logical coordinates that one
//! of four orientations maps onto the panel. A canvas may also be a horizontal strip of the
//! panel's rows, for grayscale planes drawn band by band.
//!
//! Nothing here panics: pixels outside the canvas are skipped, and coordinates wrap rather than
//! overflow.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

/// How logical coordinates map onto the panel (lib/GfxRenderer's `GfxRenderer::Orientation`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Logical x runs up the panel, y along it: rotated 90° clockwise.
    Portrait,
    /// Both axes reversed: rotated 180°.
    LandscapeClockwise,
    /// Rotated 90° counter-clockwise.
    PortraitInverted,
    /// The panel's own layout.
    LandscapeCounterClockwise,
}

/// A fill: solid ink, or one of two period-2 dither patterns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fill {
    Black,
    White,
    /// Black where logical x and y are both even.
    LightGray,
    /// Black where logical x + y is even.
    DarkGray,
}

impl Fill {
    /// Whether the fill is black at logical (`x`, `y`).
    #[must_use]
    pub fn black_at(self, x: i32, y: i32) -> bool {
        match self {
            Fill::Black => true,
            Fill::White => false,
            Fill::LightGray => x & 1 == 0 && y & 1 == 0,
            Fill::DarkGray => x.wrapping_add(y) & 1 == 0,
        }
    }
}

/// The pixels a canvas lets through, in logical coordinates: `left <= x < right`, `top <= y < bottom`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clip {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The rows of the panel being drawn into.
pub struct Canvas<'a> {
    /// `rows` rows of `stride` bytes, the first being panel row `origin_y`.
    pub buf: &'a mut [u8],
    pub stride: usize,
    pub origin_y: i32,
    pub rows: i32,
    pub panel_width: i32,
    pub panel_height: i32,
    pub orientation: Orientation,
    pub clip: Clip,
}

impl Canvas<'_> {
    /// The logical width and height.
    #[must_use]
    pub fn screen_size(&self) -> (i32, i32) {
        match self.orientation {
            Orientation::Portrait | Orientation::PortraitInverted => {
                (self.panel_height, self.panel_width)
            }
            Orientation::LandscapeClockwise | Orientation::LandscapeCounterClockwise => {
                (self.panel_width, self.panel_height)
            }
        }
    }

    /// The panel position of logical (`x`, `y`).
    #[must_use]
    pub fn to_panel(&self, x: i32, y: i32) -> (i32, i32) {
        let (w, h) = (self.panel_width, self.panel_height);
        match self.orientation {
            Orientation::Portrait => (y, h.wrapping_sub(1).wrapping_sub(x)),
            Orientation::LandscapeClockwise => (
                w.wrapping_sub(1).wrapping_sub(x),
                h.wrapping_sub(1).wrapping_sub(y),
            ),
            Orientation::PortraitInverted => (w.wrapping_sub(1).wrapping_sub(y), x),
            Orientation::LandscapeCounterClockwise => (x, y),
        }
    }

    /// Sets logical (`x`, `y`) to black or white; false only when it falls off the panel, true
    /// when drawn, clipped away or outside the strip.
    pub fn pixel(&mut self, x: i32, y: i32, black: bool) -> bool {
        let c = self.clip;
        if x < c.left || y < c.top || x >= c.right || y >= c.bottom {
            return true;
        }
        let (px, py) = self.to_panel(x, y);
        if px < 0 || px >= self.panel_width || py < 0 || py >= self.panel_height {
            return false;
        }
        let row = py.wrapping_sub(self.origin_y);
        if row < 0 || row >= self.rows {
            return true;
        }
        // Both are non-negative here.
        #[allow(clippy::cast_sign_loss)]
        let at = row as usize * self.stride + (px as usize >> 3);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let bit = 0x80u8 >> (px as u32 & 7);
        if let Some(byte) = self.buf.get_mut(at) {
            if black {
                *byte &= !bit;
            } else {
                *byte |= bit;
            }
        }
        true
    }

    /// A line from (`x1`, `y1`) to (`x2`, `y2`), both ends included; returns how many of its
    /// pixels were off the panel.
    pub fn line(&mut self, mut x1: i32, mut y1: i32, mut x2: i32, mut y2: i32, black: bool) -> u32 {
        let mut off = 0;
        let mut put = |canvas: &mut Self, x, y| {
            if !canvas.pixel(x, y, black) {
                off += 1;
            }
        };
        if x1 == x2 {
            if y2 < y1 {
                core::mem::swap(&mut y1, &mut y2);
            }
            let mut y = y1;
            while y <= y2 {
                put(self, x1, y);
                y = y.wrapping_add(1);
                if y == i32::MIN {
                    break;
                }
            }
        } else if y1 == y2 {
            if x2 < x1 {
                core::mem::swap(&mut x1, &mut x2);
            }
            let mut x = x1;
            while x <= x2 {
                put(self, x, y1);
                x = x.wrapping_add(1);
                if x == i32::MIN {
                    break;
                }
            }
        } else {
            // Bresenham, integers only.
            let mut dx = x2.wrapping_sub(x1);
            let mut dy = y2.wrapping_sub(y1);
            let sx = if dx > 0 { 1 } else { -1 };
            let sy = if dy > 0 { 1 } else { -1 };
            dx = dx.wrapping_mul(sx);
            dy = dy.wrapping_mul(sy);
            let mut err = dx.wrapping_sub(dy);
            loop {
                put(self, x1, y1);
                if x1 == x2 && y1 == y2 {
                    break;
                }
                let e2 = err.wrapping_mul(2);
                if e2 > dy.wrapping_neg() {
                    err = err.wrapping_sub(dy);
                    x1 = x1.wrapping_add(sx);
                }
                if e2 < dx {
                    err = err.wrapping_add(dx);
                    y1 = y1.wrapping_add(sy);
                }
            }
        }
        off
    }

    /// `width` lines, each one pixel below the last, from (`x1`, `y1`) to (`x2`, `y2`); returns how
    /// many pixels were off the panel.
    pub fn thick_line(
        &mut self,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        width: i32,
        black: bool,
    ) -> u32 {
        let mut off = 0u32;
        let mut i = 0;
        while i < width {
            off =
                off.wrapping_add(self.line(x1, y1.wrapping_add(i), x2, y2.wrapping_add(i), black));
            i += 1;
        }
        off
    }

    /// A rectangle's outline, `width` lines thick, inside it; returns how many pixels were off
    /// the panel.
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, width: i32, black: bool) -> u32 {
        let mut off = 0u32;
        let right = x.wrapping_add(w).wrapping_sub(1);
        let bottom = y.wrapping_add(h).wrapping_sub(1);
        let mut inset = 0;
        while inset < width {
            let (left, top) = (x.wrapping_add(inset), y.wrapping_add(inset));
            let (rgt, bot) = (right.wrapping_sub(inset), bottom.wrapping_sub(inset));
            off = off.wrapping_add(self.line(left, top, rgt, top, black));
            off = off.wrapping_add(self.line(rgt, top, rgt, bot, black));
            off = off.wrapping_add(self.line(rgt, bot, left, bot, black));
            off = off.wrapping_add(self.line(left, bot, left, top, black));
            inset += 1;
        }
        off
    }

    /// Fills a rectangle, clipped to the screen, the clip and the strip.
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, fill: Fill) {
        if w <= 0 || h <= 0 {
            return;
        }
        let (screen_w, screen_h) = self.screen_size();
        let clip = self.clip;
        let lx0 = x.max(0).max(clip.left);
        let ly0 = y.max(0).max(clip.top);
        let lx1 = screen_w.min(x.wrapping_add(w)).min(clip.right);
        let ly1 = screen_h.min(y.wrapping_add(h)).min(clip.bottom);
        if lx0 >= lx1 || ly0 >= ly1 {
            return;
        }
        // Rotation is rigid, so the two opposite corners bound the panel rectangle.
        let (ax, ay) = self.to_panel(lx0, ly0);
        let (bx, by) = self.to_panel(lx1 - 1, ly1 - 1);
        let (px0, px1) = (ax.min(bx), ax.max(bx));
        let py0 = ay.min(by).max(self.origin_y);
        let py1 = ay
            .max(by)
            .min(self.origin_y.wrapping_add(self.rows).wrapping_sub(1));
        if py0 > py1 {
            return;
        }
        let byte_start = px0 >> 3;
        let byte_end = px1 >> 3;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let head = (0xFFu32 >> (px0 & 7)) as u8;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let tail = (0xFFu32 << (7 - (px1 & 7))) as u8;
        let masks = match fill {
            Fill::Black => [0u8; 2],
            Fill::White => [0xFF; 2],
            Fill::LightGray | Fill::DarkGray => self.dither_rows(fill, py0, byte_start),
        };
        for py in py0..=py1 {
            // Ones where the fill is white.
            let ink = masks.get(usize::from(py & 1 != 0)).copied().unwrap_or(0xFF);
            #[allow(clippy::cast_sign_loss)]
            let start = (py - self.origin_y) as usize * self.stride;
            #[allow(clippy::cast_sign_loss)]
            let (first, last) = (start + byte_start as usize, start + byte_end as usize);
            let Some(row) = self.buf.get_mut(first..=last) else {
                continue;
            };
            let blend = |byte: &mut u8, mask: u8| *byte = (*byte & !mask) | (mask & ink);
            if let [only] = row {
                blend(only, head & tail);
            } else if let [first, middle @ .., last] = row {
                blend(first, head);
                middle.fill(ink);
                blend(last, tail);
            }
        }
    }

    /// The white bits of a dither fill for panel rows of each parity, from `byte_start` on.
    fn dither_rows(&self, fill: Fill, py0: i32, byte_start: i32) -> [u8; 2] {
        // How logical (x, y) move as panel x grows along a row.
        let (dlx, dly) = match self.orientation {
            Orientation::Portrait => (0, 1),
            Orientation::PortraitInverted => (0, -1),
            Orientation::LandscapeClockwise => (-1, 0),
            Orientation::LandscapeCounterClockwise => (1, 0),
        };
        let (w, h) = (self.panel_width, self.panel_height);
        let mut white = [0u8; 2];
        for parity in 0..2 {
            let py = py0 + parity;
            let x0 = byte_start * 8;
            let (lx, ly) = match self.orientation {
                Orientation::Portrait => (h - 1 - py, x0),
                Orientation::PortraitInverted => (py, w - 1 - x0),
                Orientation::LandscapeClockwise => (w - 1 - x0, h - 1 - py),
                Orientation::LandscapeCounterClockwise => (x0, py),
            };
            let mut black = 0u8;
            for b in 0..8 {
                if fill.black_at(lx + b * dlx, ly + b * dly) {
                    black |= 0x80 >> b;
                }
            }
            if let Some(slot) = white.get_mut(usize::from(py & 1 != 0)) {
                *slot = !black;
            }
        }
        white
    }
}

mod glyph;
mod image;
mod levels;
mod polygon;
mod region;
mod round;
mod scaled;
pub use glyph::{Bitmap, Frame};
pub use image::{Drawn, Picture, Placement, Plane};
pub use levels::Ink;
pub use region::{Region, Widen};
pub use round::Corners;

#[cfg(test)]
mod tests;
