//! The sleep screen: a crescent moon, with the name under it.

use crate::frame::{HEIGHT, WIDTH};
use crate::text::Text;
use bembeyaz_gfx::{Canvas, Corners, Fill};

/// The moon's radius, and how far the disc that cuts the crescent out is moved and how big it is.
const MOON: i32 = 90;
const CUT_DX: i32 = 40;
const CUT_DY: i32 = -25;
const CUT: i32 = 80;

pub fn draw(c: &mut Canvas<'_>, text: Option<&mut Text<'_>>) {
    c.fill_rect(0, 0, WIDTH, HEIGHT, Fill::White);
    let (cx, cy) = (WIDTH / 2, HEIGHT / 2);
    disc(c, cx, cy, MOON, Fill::Black);
    disc(c, cx + CUT_DX, cy + CUT_DY, CUT, Fill::White);
    if let Some(text) = text {
        text.draw_centered(c, cy + MOON + 30, "purestWhite");
    }
}

/// A disc of radius `r` around (`x`, `y`).
fn disc(c: &mut Canvas<'_>, x: i32, y: i32, r: i32, fill: Fill) {
    c.fill_rounded_rect(x - r, y - r, 2 * r, 2 * r, r, Corners::ALL, fill);
}
