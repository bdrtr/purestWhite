//! The X4 Pro's panel as a frame: 800 by 480 pixels, a bit each, 1 white, in 100-byte rows, drawn
//! upright as the device is held: 480 wide, 800 high.

use bembeyaz_gfx::{Canvas, Clip, Orientation};

const PANEL_WIDTH: i32 = 800;
const PANEL_HEIGHT: i32 = 480;
const STRIDE: usize = 100;
/// Bytes in a frame.
pub const FRAME_BYTES: usize = STRIDE * PANEL_HEIGHT as usize;
/// The upright screen's size.
pub const WIDTH: i32 = PANEL_HEIGHT;
pub const HEIGHT: i32 = PANEL_WIDTH;

/// Whether the upright pixel (`x`, `y`) of `frame` is black; off the screen it is white, as the
/// paper.
#[must_use]
pub fn is_black(frame: &[u8], x: i32, y: i32) -> bool {
    if !(0..WIDTH).contains(&x) || !(0..HEIGHT).contains(&y) {
        return false;
    }
    // Portrait: the upright x runs up the panel's rows, y along them (`Canvas::to_panel`).
    let (px, py) = (y, PANEL_HEIGHT - 1 - x);
    #[allow(
        clippy::cast_sign_loss,
        reason = "both are on the panel, so not negative"
    )]
    let at = py as usize * STRIDE + px as usize / 8;
    frame
        .get(at)
        .is_some_and(|byte| byte & (0x80 >> (px % 8)) == 0)
}

/// `frame` as `bembeyaz_gfx` draws into it, upright.
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
