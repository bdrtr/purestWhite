//! What the image draws: the frame upright as the device is held, the key boxes with their signs
//! and names, the ticks under them, the gestures' marks, the counts' line and the sleep screen's
//! moon.

use crate::font::Font;
use alloc::string::String;
use bembeyaz_gfx::{Canvas, Clip, Corners, Fill, Orientation};

/// The panel in its own layout: 800 by 480 pixels, a bit each, 1 white, in 100-byte rows. It is
/// drawn upright as the device is held: 480 wide, 800 high.
const PANEL_WIDTH: i32 = 800;
const PANEL_HEIGHT: i32 = 480;
const STRIDE: usize = 100;
pub const FRAME_BYTES: usize = STRIDE * PANEL_HEIGHT as usize;
const WIDTH: i32 = PANEL_HEIGHT;
const HEIGHT: i32 = PANEL_WIDTH;

/// The key boxes along the top, in the order Left, Right, Power, Home.
pub const KEYS: usize = 4;
const KEY_TOP: i32 = 20;
const KEY_WIDTH: i32 = 100;
const KEY_HEIGHT: i32 = 80;
/// The boxes' names, under them in the card's font when it is there.
const LABELS: [&str; KEYS] = ["Sol", "Sağ", "Güç", "Home"];
const LABEL_TOP: i32 = KEY_TOP + KEY_HEIGHT + 4;
/// The counts' line at the bottom, in the card's font.
const STATUS_TOP: i32 = HEIGHT - 40;

/// Ticks under a box: in rows of `TICKS_PER_ROW`, `TICK_ROWS` of them, then they wrap.
const TICK: i32 = 10;
const TICK_STEP: i32 = 14;
const TICKS_PER_ROW: u32 = 7;
const TICK_ROWS: u32 = 3;

/// A box's left edge.
fn key_left(key: usize) -> i32 {
    // At most 4, so the cast is exact.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let key = key as i32;
    15 + key * 115
}

/// The frame as `bembeyaz_gfx` draws into it, upright.
pub fn canvas(frame: &mut [u8]) -> Canvas<'_> {
    Canvas {
        buf: frame,
        stride: STRIDE,
        origin_y: 0,
        rows: PANEL_HEIGHT,
        panel_width: PANEL_WIDTH,
        panel_height: PANEL_HEIGHT,
        orientation: Orientation::Portrait,
        clip: Clip {
            left: 0,
            top: 0,
            right: WIDTH,
            bottom: HEIGHT,
        },
    }
}

/// A disc of radius `r` around (`x`, `y`).
fn disc(c: &mut Canvas<'_>, x: i32, y: i32, r: i32, fill: Fill) {
    c.fill_rounded_rect(x - r, y - r, 2 * r, 2 * r, r, Corners::ALL, fill);
}

/// The four boxes, each with its sign (an arrow left, an arrow right, a ring, a house) and, in
/// `font`, its name under it.
pub fn draw_keys(c: &mut Canvas<'_>, font: &mut Option<Font<'_>>) {
    if let Some(font) = font {
        for (key, label) in LABELS.iter().enumerate() {
            let width = font.width(label);
            font.draw(c, key_left(key) + (KEY_WIDTH - width) / 2, LABEL_TOP, label);
        }
    }
    for key in 0..KEYS {
        let left = key_left(key);
        c.rect(left, KEY_TOP, KEY_WIDTH, KEY_HEIGHT, 3, true);
        draw_sign(c, key, left + KEY_WIDTH / 2, KEY_TOP + KEY_HEIGHT / 2);
    }
}

/// `key`'s sign around (`cx`, `cy`).
fn draw_sign(c: &mut Canvas<'_>, key: usize, cx: i32, cy: i32) {
    match key {
        0 => c.fill_polygon(
            &[cx - 25, cx + 20, cx + 20],
            &[cy, cy - 25, cy + 25],
            Fill::Black,
        ),
        1 => c.fill_polygon(
            &[cx + 25, cx - 20, cx - 20],
            &[cy, cy - 25, cy + 25],
            Fill::Black,
        ),
        2 => {
            disc(c, cx, cy, 25, Fill::Black);
            disc(c, cx, cy, 18, Fill::White);
        }
        _ => {
            c.fill_polygon(
                &[cx - 28, cx, cx + 28],
                &[cy - 2, cy - 28, cy - 2],
                Fill::Black,
            );
            c.fill_rect(cx - 20, cy - 2, 40, 28, Fill::Black);
        }
    }
}

/// The `n`th tick (from 0) under `key`'s box; the ticks wrap after `TICKS_PER_ROW * TICK_ROWS`,
/// clearing the space first.
pub fn draw_tick(c: &mut Canvas<'_>, key: usize, n: u32) {
    let n = n % (TICKS_PER_ROW * TICK_ROWS);
    let left = key_left(key);
    let top = KEY_TOP + KEY_HEIGHT + 34;
    if n == 0 {
        #[allow(clippy::cast_possible_wrap, reason = "3")]
        c.fill_rect(
            left,
            top,
            KEY_WIDTH,
            TICK_STEP * TICK_ROWS as i32,
            Fill::White,
        );
    }
    #[allow(clippy::cast_possible_wrap, reason = "below 21")]
    let (column, row) = ((n % TICKS_PER_ROW) as i32, (n / TICKS_PER_ROW) as i32);
    c.fill_rect(
        left + column * TICK_STEP,
        top + row * TICK_STEP,
        TICK,
        TICK,
        Fill::Black,
    );
}

/// A tap's mark: a dot where it touched down.
pub fn draw_tap(c: &mut Canvas<'_>, (x, y): (i32, i32)) {
    disc(c, x, y, 10, Fill::Black);
}

/// A long press's mark: a ring where the contact was held.
pub fn draw_long_press(c: &mut Canvas<'_>, (x, y): (i32, i32)) {
    disc(c, x, y, 24, Fill::Black);
    disc(c, x, y, 19, Fill::White);
}

/// A swipe's mark: a line with a dot at its end.
pub fn draw_swipe(c: &mut Canvas<'_>, (x1, y1): (i32, i32), (x2, y2): (i32, i32)) {
    c.thick_line(x1, y1, x2, y2, 5, true);
    disc(c, x2, y2, 9, Fill::Black);
}

/// The crescent moon of the sleep screen, with a good night under it in `font`.
pub fn draw_moon(c: &mut Canvas<'_>, font: &mut Option<Font<'_>>) {
    c.fill_rect(0, 0, WIDTH, HEIGHT, Fill::White);
    disc(c, WIDTH / 2, HEIGHT / 2, 90, Fill::Black);
    disc(c, WIDTH / 2 + 40, HEIGHT / 2 - 25, 80, Fill::White);
    if let Some(font) = font {
        let text = "İyi geceler";
        let width = font.width(text);
        font.draw(c, (WIDTH - width) / 2, HEIGHT / 2 + 120, text);
    }
}

/// The line at the bottom, in `font`; `text` makes it only when there is a font.
pub fn draw_status(c: &mut Canvas<'_>, font: &mut Option<Font<'_>>, text: impl FnOnce() -> String) {
    if let Some(font) = font {
        c.fill_rect(0, STATUS_TOP, WIDTH, HEIGHT - STATUS_TOP, Fill::White);
        font.draw(c, 15, STATUS_TOP + 8, &text());
    }
}
