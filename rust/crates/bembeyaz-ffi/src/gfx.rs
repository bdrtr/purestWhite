//! Exports for lib/GfxRenderer (see `rust/include/bembeyaz/gfx.h`).

use bembeyaz_gfx::{
    Bitmap, Canvas, Clip, Corners, Fill, Frame, Orientation, Picture, Placement, Plane, Widen,
};

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

// gfx.h's BB_GFX_* corners.
const _: () = assert!(
    Corners::TOP_LEFT.0 == 1
        && Corners::TOP_RIGHT.0 == 2
        && Corners::BOTTOM_LEFT.0 == 4
        && Corners::BOTTOM_RIGHT.0 == 8
);

/// gfx.h's BB_GFX_* fills, which GfxRenderer.cpp checks against its `Color`.
const BLACK: u8 = 0x10;
const WHITE: u8 = 0x01;
const LIGHT_GRAY: u8 = 0x05;
const DARK_GRAY: u8 = 0x0A;

/// The fill a BB_GFX_* color names; none for any other value.
fn fill(color: u8) -> Option<Fill> {
    match color {
        BLACK => Some(Fill::Black),
        WHITE => Some(Fill::White),
        LIGHT_GRAY => Some(Fill::LightGray),
        DARK_GRAY => Some(Fill::DarkGray),
        _ => None,
    }
}

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
    let Some(fill) = fill(color) else { return };
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.fill_rect(x, y, w, h, fill);
    }
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_arc(
    canvas: *const CCanvas,
    radius: i32,
    cx: i32,
    cy: i32,
    x_dir: i32,
    y_dir: i32,
    width: i32,
    black: bool,
) {
    let fill = if black { Fill::Black } else { Fill::White };
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.arc(radius, cx, cy, x_dir, y_dir, width, fill);
    }
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_rounded_rect(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    width: i32,
    radius: i32,
    round: u8,
    black: bool,
) -> u32 {
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| {
        c.rounded_rect(x, y, w, h, width, radius, Corners(round), black)
    })
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_fill_rounded_rect(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    round: u8,
    color: u8,
) {
    let Some(fill) = fill(color) else { return };
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.fill_rounded_rect(x, y, w, h, radius, Corners(round), fill);
    }
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_mask_corners(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color: u8,
) -> u32 {
    let Some(fill) = fill(color) else { return 0 };
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| c.mask_corners(x, y, w, h, radius, fill))
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `xs` and `ys` are NULL or hold `count` values each.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_fill_polygon(
    canvas: *const CCanvas,
    xs: *const core::ffi::c_int,
    ys: *const core::ffi::c_int,
    count: usize,
    black: bool,
) {
    let fill = if black { Fill::Black } else { Fill::White };
    // SAFETY: as documented.
    let (xs, ys) = unsafe { (crate::view(xs, count), crate::view(ys, count)) };
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.fill_polygon(xs, ys, fill);
    }
}

/// gfx.h's `bb_gfx_picture`.
#[repr(C)]
pub struct CPicture {
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    max_width: i32,
    max_height: i32,
    crop_x: f32,
    crop_y: f32,
    top_down: bool,
    one_bit: bool,
    white_transparent: bool,
    absolute_gray: bool,
    plane: u8,
}

/// gfx.h's `bb_gfx_drawn`.
#[repr(C)]
pub struct CDrawn {
    crop_x: i32,
    crop_y: i32,
    scale: f32,
    width: i32,
    height: i32,
    failed_row: i32,
}

// gfx.h's layouts; GfxRenderer.cpp checks the same.
const _: () = assert!(
    size_of::<CPicture>() == 40
        && core::mem::offset_of!(CPicture, crop_x) == 24
        && core::mem::offset_of!(CPicture, plane) == 36
        && size_of::<CDrawn>() == 24
);

/// gfx.h's BB_GFX_PLANE_*, which GfxRenderer.cpp checks against its `RenderMode`.
const PLANE_GRAY_LSB: u8 = 1;
const PLANE_GRAY_MSB: u8 = 2;

/// gfx.h's BB_GFX_PLANE_*; anything else is black and white.
fn plane(value: u8) -> Plane {
    match value {
        PLANE_GRAY_LSB => Plane::GrayLsb,
        PLANE_GRAY_MSB => Plane::GrayMsb,
        _ => Plane::Bw,
    }
}

/// Reads the next picture row into its argument; false when it cannot.
pub type ReadRow = unsafe extern "C" fn(ctx: *mut core::ffi::c_void, row: *mut u8) -> bool;

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `picture` and `drawn` are NULL or valid; `row` is NULL
/// or holds `row_len` writable bytes; `read` may be called with `ctx` and `row`, writes at most
/// `(width + 3) / 4` bytes there and does not draw.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_draw_picture(
    canvas: *const CCanvas,
    picture: *const CPicture,
    row: *mut u8,
    row_len: usize,
    read: Option<ReadRow>,
    ctx: *mut core::ffi::c_void,
    drawn: *mut CDrawn,
) -> bool {
    // SAFETY: as documented.
    let (Some(p), Some(read)) = (unsafe { picture.as_ref() }, read) else {
        return false;
    };
    // SAFETY: as documented.
    let Some(mut c) = (unsafe { self::canvas(canvas) }) else {
        return false;
    };
    // read() fills a whole packed row: refuse a buffer it would overrun.
    let needed = usize::try_from(p.width.max(0)).unwrap_or(0).div_ceil(4);
    if needed > 0 && (row.is_null() || row_len < needed) {
        let failed = CDrawn {
            crop_x: 0,
            crop_y: 0,
            scale: 1.0,
            width: 0,
            height: 0,
            failed_row: 0,
        };
        // SAFETY: as documented.
        unsafe { crate::set(drawn, failed) };
        return false;
    }
    // SAFETY: as documented; the row is only written through `read`, between Rust's reads.
    let row = unsafe { crate::view_mut(row, row_len) };
    let plane = plane(p.plane);
    let picture = Picture {
        width: p.width,
        height: p.height,
        top_down: p.top_down,
        one_bit: p.one_bit,
    };
    let place = Placement {
        x: p.x,
        y: p.y,
        max_width: p.max_width,
        max_height: p.max_height,
        crop_x: p.crop_x,
        crop_y: p.crop_y,
        white_transparent: p.white_transparent,
        plane,
        absolute_gray: p.absolute_gray,
    };
    // SAFETY: as documented.
    let result = c.picture(&picture, &place, row, |row| unsafe {
        read(ctx, row.as_mut_ptr())
    });
    let out = match result {
        Ok(d) => CDrawn {
            crop_x: d.crop.0,
            crop_y: d.crop.1,
            scale: d.scale,
            width: d.width,
            height: d.height,
            failed_row: -1,
        },
        Err(failed_row) => CDrawn {
            crop_x: 0,
            crop_y: 0,
            scale: 1.0,
            width: 0,
            height: 0,
            failed_row,
        },
    };
    let ok = out.failed_row < 0;
    // SAFETY: as documented.
    unsafe { crate::set(drawn, out) };
    ok
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `bits` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_icon(
    canvas: *const CCanvas,
    bits: *const u8,
    len: usize,
    x: i32,
    y: i32,
    size: i32,
) -> u32 {
    // SAFETY: as documented.
    let bits = unsafe { crate::view(bits, len) };
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| c.icon(bits, x, y, size))
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_invert_rect(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
) {
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.invert_rect(x, y, w, h);
    }
}

/// gfx.h's `bb_gfx_frame`.
#[repr(C)]
pub struct CFrame {
    x: i32,
    y: i32,
    dx_x: i32,
    dx_y: i32,
    dy_x: i32,
    dy_y: i32,
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `bits` is NULL or holds `len` bytes; `frame` is NULL or
/// valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_glyph(
    canvas: *const CCanvas,
    bits: *const u8,
    len: usize,
    width: i32,
    height: i32,
    frame: *const CFrame,
    two_bit: bool,
    plane_value: u8,
    black: bool,
) {
    // SAFETY: as documented.
    let Some(f) = (unsafe { frame.as_ref() }) else {
        return;
    };
    let frame = Frame {
        x: f.x,
        y: f.y,
        dx_x: f.dx_x,
        dx_y: f.dx_y,
        dy_x: f.dy_x,
        dy_y: f.dy_y,
    };
    // SAFETY: as documented.
    let glyph = unsafe { bitmap(bits, len, width, height, two_bit) };
    // SAFETY: as documented.
    if let Some(mut c) = unsafe { self::canvas(canvas) } {
        c.glyph(&glyph, &frame, plane(plane_value), black);
    }
}

/// The glyph bitmap gfx.h's arguments describe.
///
/// # Safety
/// `bits` is NULL or holds `len` bytes.
unsafe fn bitmap<'a>(
    bits: *const u8,
    len: usize,
    width: i32,
    height: i32,
    two_bit: bool,
) -> Bitmap<'a> {
    Bitmap {
        // SAFETY: as documented.
        bits: unsafe { crate::view(bits, len) },
        width,
        height,
        two_bit,
    }
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `bits` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_reduced_glyph(
    canvas: *const CCanvas,
    bits: *const u8,
    len: usize,
    width: i32,
    height: i32,
    two_bit: bool,
    num: i32,
    den: i32,
    x: i32,
    y: i32,
    black: bool,
) -> u32 {
    // SAFETY: as documented.
    let glyph = unsafe { bitmap(bits, len, width, height, two_bit) };
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| {
        c.reduced_glyph(&glyph, (num, den), (x, y), black)
    })
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `bits` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_resampled_glyph(
    canvas: *const CCanvas,
    bits: *const u8,
    len: usize,
    width: i32,
    height: i32,
    two_bit: bool,
    scale256: i32,
    x: i32,
    y: i32,
) -> u32 {
    // SAFETY: as documented.
    let glyph = unsafe { bitmap(bits, len, width, height, two_bit) };
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }.map_or(0, |mut c| c.resampled_glyph(&glyph, scale256, (x, y)))
}

fn widen(aligned: bool) -> Widen {
    if aligned {
        Widen::Aligned
    } else {
        Widen::Touched
    }
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_region_size(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    aligned: bool,
) -> usize {
    // SAFETY: as documented.
    unsafe { self::canvas(canvas) }
        .and_then(|c| c.region(x, y, w, h, widen(aligned)))
        .map_or(0, |r| r.size())
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `out` is NULL or holds `capacity` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_read_region(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    aligned: bool,
    out: *mut u8,
    capacity: usize,
) -> usize {
    // SAFETY: as documented.
    let Some(c) = (unsafe { self::canvas(canvas) }) else {
        return 0;
    };
    let Some(region) = c.region(x, y, w, h, widen(aligned)) else {
        return 0;
    };
    // SAFETY: as documented.
    let out = unsafe { crate::view_mut(out, capacity) };
    if c.read_region(&region, out) {
        region.size()
    } else {
        0
    }
}

/// # Safety
/// `canvas` is NULL or as gfx.h describes; `src` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_gfx_write_region(
    canvas: *const CCanvas,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    aligned: bool,
    src: *const u8,
    len: usize,
) -> bool {
    // SAFETY: as documented.
    let Some(mut c) = (unsafe { self::canvas(canvas) }) else {
        return false;
    };
    let Some(region) = c.region(x, y, w, h, widen(aligned)) else {
        return false;
    };
    // SAFETY: as documented.
    let src = unsafe { crate::view(src, len) };
    c.write_region(&region, src)
}
