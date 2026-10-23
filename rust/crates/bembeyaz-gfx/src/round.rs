//! Arcs and rounded rectangles, drawn as rows of `fill_rect`, and the corner mask that rounds an
//! image's corners off. The work grows with the radius, whatever is visible (an arc takes one
//! step per pixel of radius, the mask up to radius² pixels per corner): callers round small UI corners.

use super::{Canvas, Fill};

/// Which corners of a rounded rectangle are round: any of the four below, or'ed together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Corners(pub u8);

impl Corners {
    pub const TOP_LEFT: Self = Self(1);
    pub const TOP_RIGHT: Self = Self(2);
    pub const BOTTOM_LEFT: Self = Self(4);
    pub const BOTTOM_RIGHT: Self = Self(8);
    pub const ALL: Self = Self(15);

    /// Whether all of `corners` are round.
    #[must_use]
    pub fn has(self, corners: Self) -> bool {
        self.0 & corners.0 == corners.0
    }
}

impl core::ops::BitOr for Corners {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

fn square(v: i32) -> i64 {
    i64::from(v) * i64::from(v)
}

impl Canvas<'_> {
    /// A quarter of the ring between the circles of `radius` and `radius - stroke` around
    /// (`cx`, `cy`), on the side `x_dir` and `y_dir` point to (1 or -1 each); a stroke as wide
    /// as the radius fills the quarter disc.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        radius: i32,
        cx: i32,
        cy: i32,
        x_dir: i32,
        y_dir: i32,
        stroke: i32,
        fill: Fill,
    ) {
        if radius <= 0 {
            return;
        }
        let inner = radius.wrapping_sub(stroke.min(radius)).max(0);
        let (outer_sq, inner_sq) = (square(radius), square(inner));
        // Scanned inward as the row moves away from the centre: no square roots.
        let (mut x_outer, mut x_inner) = (radius, inner);
        for dy in 0..=radius {
            while x_outer > 0 && square(x_outer) + square(dy) > outer_sq {
                x_outer -= 1;
            }
            // The smallest x on or outside the inner circle.
            while x_inner > 0 && square(x_inner - 1) + square(dy) >= inner_sq {
                x_inner -= 1;
            }
            if x_outer < x_inner {
                continue;
            }
            let x0 = cx.wrapping_add(x_dir.wrapping_mul(x_inner));
            let x1 = cx.wrapping_add(x_dir.wrapping_mul(x_outer));
            let width = x1.wrapping_sub(x0).wrapping_abs().wrapping_add(1);
            self.fill_rect(
                x0.min(x1),
                cy.wrapping_add(y_dir.wrapping_mul(dy)),
                width,
                1,
                fill,
            );
        }
    }

    /// A rectangle's outline, `width` lines thick, inside it, with the given corners rounded to
    /// `radius` (at most half the rectangle); sides between two square corners are left out.
    /// Returns how many pixels were off the panel, which only a radius of zero (a plain outline)
    /// counts.
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rect(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        width: i32,
        radius: i32,
        corners: Corners,
        black: bool,
    ) -> u32 {
        if width <= 0 || w <= 0 || h <= 0 {
            return 0;
        }
        let radius = radius.min(w / 2).min(h / 2);
        if radius <= 0 {
            return self.rect(x, y, w, h, width, black);
        }
        let fill = if black { Fill::Black } else { Fill::White };
        let stroke = width.min(radius);
        let right = x.wrapping_add(w).wrapping_sub(1);
        let bottom = y.wrapping_add(h).wrapping_sub(1);
        let top_left = corners.has(Corners::TOP_LEFT);
        let top_right = corners.has(Corners::TOP_RIGHT);
        let bottom_left = corners.has(Corners::BOTTOM_LEFT);
        let bottom_right = corners.has(Corners::BOTTOM_RIGHT);
        // The radius is at most half of w and h, so neither side length goes negative.
        let across = w - 2 * radius;
        if across > 0 {
            if top_left || top_right {
                self.fill_rect(x.wrapping_add(radius), y, across, stroke, fill);
            }
            if bottom_left || bottom_right {
                let top = bottom.wrapping_sub(stroke).wrapping_add(1);
                self.fill_rect(x.wrapping_add(radius), top, across, stroke, fill);
            }
        }
        let down = h - 2 * radius;
        if down > 0 {
            if top_left || bottom_left {
                self.fill_rect(x, y.wrapping_add(radius), stroke, down, fill);
            }
            if top_right || bottom_right {
                let left = right.wrapping_sub(stroke).wrapping_add(1);
                self.fill_rect(left, y.wrapping_add(radius), stroke, down, fill);
            }
        }
        let xs = (x.wrapping_add(radius), right.wrapping_sub(radius));
        let ys = (y.wrapping_add(radius), bottom.wrapping_sub(radius));
        self.corner_arcs(radius, xs, ys, width, corners, fill);
        0
    }

    /// Fills a rectangle with the given corners rounded to `radius`: at most half the rectangle
    /// when all four are round, at most all of it otherwise (one rounded side).
    #[allow(clippy::too_many_arguments)]
    pub fn fill_rounded_rect(
        &mut self,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        radius: i32,
        corners: Corners,
        fill: Fill,
    ) {
        if w <= 0 || h <= 0 {
            return;
        }
        let sides = if corners.has(Corners::ALL) { 2 } else { 1 };
        let radius = radius.min(w / sides).min(h / sides);
        if radius <= 0 {
            self.fill_rect(x, y, w, h, fill);
            return;
        }
        let top_left = corners.has(Corners::TOP_LEFT);
        let top_right = corners.has(Corners::TOP_RIGHT);
        let bottom_left = corners.has(Corners::BOTTOM_LEFT);
        let bottom_right = corners.has(Corners::BOTTOM_RIGHT);
        // The middle columns, then the two side columns (radius + 1 wide, each cut short by the
        // round corners at its ends), then the corners; they overlap where the rectangle is narrow.
        let across = w.wrapping_sub(radius.wrapping_mul(2));
        if across > 0 {
            self.fill_rect(
                x.wrapping_add(radius).wrapping_add(1),
                y,
                across - 2,
                h,
                fill,
            );
        }
        let cut = radius.wrapping_add(1);
        let bottom = y.wrapping_add(h).wrapping_sub(1);
        let right = x.wrapping_add(w).wrapping_sub(cut);
        for (left, round_top, round_bottom) in
            [(x, top_left, bottom_left), (right, top_right, bottom_right)]
        {
            let top = if round_top { y.wrapping_add(cut) } else { y };
            let end = if round_bottom {
                bottom.wrapping_sub(cut)
            } else {
                bottom
            };
            if end >= top {
                let height = end.wrapping_sub(top).wrapping_add(1);
                self.fill_rect(left, top, cut, height, fill);
            }
        }
        let xs = (x.wrapping_add(radius), right);
        let ys = (y.wrapping_add(radius), bottom.wrapping_sub(radius));
        self.corner_arcs(radius, xs, ys, radius, corners, fill);
    }

    /// The arcs of a rounded rectangle's round corners, around the (left, right) centre x in `xs`
    /// and the (top, bottom) centre y in `ys`.
    fn corner_arcs(
        &mut self,
        radius: i32,
        xs: (i32, i32),
        ys: (i32, i32),
        stroke: i32,
        corners: Corners,
        fill: Fill,
    ) {
        let ((left, right), (top, bottom)) = (xs, ys);
        for (corner, cx, cy, x_dir, y_dir) in [
            (Corners::TOP_LEFT, left, top, -1, -1),
            (Corners::TOP_RIGHT, right, top, 1, -1),
            (Corners::BOTTOM_RIGHT, right, bottom, 1, 1),
            (Corners::BOTTOM_LEFT, left, bottom, -1, 1),
        ] {
            if corners.has(corner) {
                self.arc(radius, cx, cy, x_dir, y_dir, stroke, fill);
            }
        }
    }

    /// Paints the pixels of a rectangle's four `radius` by `radius` corners that lie outside the
    /// circle of `radius - 1` around each corner's inner pixel, so that what was drawn there
    /// (an image) looks rounded. Returns how many pixels were off the panel.
    pub fn mask_corners(&mut self, x: i32, y: i32, w: i32, h: i32, radius: i32, fill: Fill) -> u32 {
        if radius <= 0 {
            return 0;
        }
        let rr = radius - 1;
        let right = x.wrapping_add(w).wrapping_sub(1);
        let bottom = y.wrapping_add(h).wrapping_sub(1);
        let mut off = 0u32;
        for dy in 0..radius {
            for dx in 0..radius {
                // Moving right brings the pixel closer to the circle's centre, so the rest of the row
                // is inside once one pixel is.
                if square(rr - dx) + square(rr - dy) <= square(rr) {
                    break;
                }
                let (left, rgt) = (x.wrapping_add(dx), right.wrapping_sub(dx));
                let (top, bot) = (y.wrapping_add(dy), bottom.wrapping_sub(dy));
                for (px, py) in [(left, top), (rgt, top), (left, bot), (rgt, bot)] {
                    if !self.pixel(px, py, fill.black_at(px, py)) {
                        off = off.wrapping_add(1);
                    }
                }
            }
        }
        off
    }
}
