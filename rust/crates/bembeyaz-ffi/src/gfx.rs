//! Exports for lib/GfxRenderer (see `rust/include/bembeyaz/gfx.h`).

use bembeyaz_gfx::{Canvas, Clip, Fill, Orientation};

/// gfx.h's `bb_gfx_canvas`.
#[repr(C)]
pub struct CCanvas {
    buf: *mut u8,
    len: usize,
    stride: usize,
    origin_y: i32,
    rows: i32,
    panel_width: i32,
    panel_height: i32,
    clip_left: i32,
    clip_top: i32,
    clip_right: i32,
    clip_bottom: i32,
    orientation: u8,
}

// gfx.h's bb_gfx_canvas; GfxRenderer.cpp checks the same layout.
const _: () = assert!(
    size_of::<CCanvas>() == 3 * size_of::<usize>() + 8 * 4 + size_of::<usize>()
        && core::mem::offset_of!(CCanvas, origin_y) == 3 * size_of::<usize>()
        && core::mem::offset_of!(CCanvas, orientation) == 3 * size_of::<usize>() + 32
);

/// The canvas `c` describes, or none for NULL.
///
/// # Safety
/// `c` is NULL or as gfx.h describes: `buf` holds `len` writable bytes for the call.
unsafe fn canvas<'a>(c: *const CCanvas) -> Option<Canvas<'a>> {
    // SAFETY: as documented.
    let c = unsafe { c.as_ref() }?;
    // gfx.h's BB_GFX_* orientations, which GfxRenderer.cpp checks against its own; anything else
    // is the panel's own layout.
    let orientation = match c.orientation {
        0 => Orientation::Portrait,
        1 => Orientation::LandscapeClockwise,
        2 => Orientation::PortraitInverted,
        _ => Orientation::LandscapeCounterClockwise,
    };
    Some(Canvas {
        // SAFETY: as documented.
        buf: unsafe { crate::view_mut(c.buf, c.len) },
        stride: c.stride,
        origin_y: c.origin_y,
        rows: c.rows,
        panel_width: c.panel_width,
        panel_height: c.panel_height,
        orientation,
        clip: Clip {
            left: c.clip_left,
            top: c.clip_top,
            right: c.clip_right,
            bottom: c.clip_bottom,
        },
    })
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_line(
    canvas: *const CCanvas,
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    width: i32,
    black: bool,
) -> u32 {
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| c.thick_line(x1, y1, x2, y2, width, black))
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_rect(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    width: i32,
    black: bool,
) -> u32 {
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| c.rect(x, y, w, h, width, black))
}

/// gfx.h's BB_GFX_* fills, which GfxRenderer.cpp checks against its `Color`.
const BLACK: u8 = 0x10;
const WHITE: u8 = 0x01;
const LIGHT_GRAY: u8 = 0x05;
const DARK_GRAY: u8 = 0x0A;

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_fill_rect(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: u8,
) {
    let fill = match color {
        BLACK => Fill::Black,
        WHITE => Fill::White,
        LIGHT_GRAY => Fill::LightGray,
        DARK_GRAY => Fill::DarkGray,
        _ => return,
    };
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.fill_rect(x, y, w, h, fill);
    }
}
