//! Exports for lib/GfxRenderer's BitmapHelpers and lib/Epub's framebuffer converters (see
//! `rust/include/bembeyaz/dither.h`).

use bembeyaz_dither::{Ditherer, GrayBlock, Line, Method, Quantize, Scale};

/// dither.h's `bb_dither`.
#[repr(C)]
pub struct CDither {
    errors: *mut i16,
    errors_len: usize,
    rows_done: u32,
    method: u8,
    original_thresholds: bool,
}

// dither.h's layout; BitmapHelpers.h checks the same, and the BB_DITHER_* values.
const _: () = assert!(
    size_of::<CDither>() == 2 * size_of::<usize>() + 8
        && core::mem::offset_of!(CDither, rows_done) == 2 * size_of::<usize>()
        && core::mem::offset_of!(CDither, method) == 2 * size_of::<usize>() + 4
        && core::mem::offset_of!(CDither, original_thresholds) == 2 * size_of::<usize>() + 5
);

/// dither.h's BB_DITHER_* methods.
fn method(value: u8) -> Method {
    match value {
        1 => Method::Native,
        2 => Method::Atkinson,
        3 => Method::FloydSteinberg,
        4 => Method::Atkinson1,
        _ => Method::Threshold,
    }
}

/// The ditherer `d` describes, or none for NULL.
///
/// # Safety
/// `d` is NULL or as dither.h describes: `errors` is NULL or holds `errors_len` values, used by
/// nothing else during the call.
unsafe fn ditherer<'a>(d: *mut CDither) -> Option<Ditherer<'a>> {
    // SAFETY: as documented.
    let d = unsafe { d.as_mut() }?;
    Some(Ditherer {
        method: method(d.method),
        original_thresholds: d.original_thresholds,
        // SAFETY: as documented.
        errors: unsafe { crate::view_mut(d.errors, d.errors_len) },
        rows_done: &mut d.rows_done,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_dither_error_len(method_value: u8, width: i32) -> usize {
    method(method_value).error_len(usize::try_from(width).unwrap_or(0))
}

/// # Safety
/// `d` is NULL or as dither.h describes; `gray` is NULL or holds `count` bytes; `out` is NULL or
/// holds `out_len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dither_pixels(
    d: *mut CDither,
    gray: *const u8,
    count: usize,
    x0: usize,
    y: i32,
    out: *mut u8,
    out_len: usize,
) {
    // SAFETY: as documented.
    let Some(mut d) = (unsafe { ditherer(d) }) else {
        return;
    };
    // SAFETY: as documented.
    let (gray, out) = unsafe { (crate::view(gray, count), crate::view_mut(out, out_len)) };
    d.pixels(gray, x0, y, out);
}

/// # Safety
/// `d` is NULL or as dither.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dither_next_row(d: *mut CDither) {
    // SAFETY: as documented.
    if let Some(mut d) = unsafe { ditherer(d) } {
        d.next_row();
    }
}

/// # Safety
/// `d` is NULL or as dither.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dither_reset(d: *mut CDither) {
    // SAFETY: as documented.
    if let Some(mut d) = unsafe { ditherer(d) } {
        d.reset();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_dither_adjust(gray: u8) -> u8 {
    bembeyaz_dither::adjust(gray)
}

/// dither.h's `bb_dither_gray_block`.
#[repr(C)]
pub struct CGrayBlock {
    pixels: *const u8,
    len: usize,
    stride: usize,
    width: i32,
    height: i32,
    x: i32,
    y: i32,
}

/// dither.h's `bb_dither_scale`.
#[repr(C)]
pub struct CScale {
    x: i32,
    inv_x: i32,
    y: i32,
    inv_y: i32,
}

// dither.h's layouts; the JPEG converter checks the same.
const _: () = assert!(
    size_of::<CGrayBlock>() == 3 * size_of::<usize>() + 16
        && core::mem::offset_of!(CGrayBlock, width) == 3 * size_of::<usize>()
        && core::mem::offset_of!(CGrayBlock, y) == 3 * size_of::<usize>() + 12
        && size_of::<CScale>() == 16
        && core::mem::offset_of!(CScale, inv_y) == 12
);

/// dither.h's BB_DITHER_QUANTIZE_* values.
fn quantize(value: u8) -> Quantize {
    match value {
        1 => Quantize::Thirds,
        2 => Quantize::Quarters,
        _ => Quantize::Bayer,
    }
}

/// # Safety
/// `block` and `scale` are NULL or valid, and `block`'s `pixels` is NULL or holds `len` bytes;
/// `out` is NULL or holds `out_len` writable bytes.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_dither_block_row(
    block: *const CGrayBlock,
    scale: *const CScale,
    dst_y: i32,
    start: i32,
    end: i32,
    ox: i32,
    oy: i32,
    quantize_value: u8,
    out: *mut u8,
    out_len: usize,
) {
    // SAFETY: as documented.
    let (Some(b), Some(s)) = (unsafe { block.as_ref() }, unsafe { scale.as_ref() }) else {
        return;
    };
    let block = GrayBlock {
        // SAFETY: as documented.
        pixels: unsafe { crate::view(b.pixels, b.len) },
        stride: b.stride,
        width: b.width,
        height: b.height,
        x: b.x,
        y: b.y,
    };
    let scale = Scale {
        x: s.x,
        inv_x: s.inv_x,
        y: s.y,
        inv_y: s.inv_y,
    };
    // SAFETY: as documented.
    let out = unsafe { crate::view_mut(out, out_len) };
    bembeyaz_dither::block_row(
        &block,
        &scale,
        dst_y,
        (start, end),
        (ox, oy),
        quantize(quantize_value),
        out,
    );
}

/// # Safety
/// `dst` is NULL or holds `dst_len` writable bytes; `src` is NULL or holds `src_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_dither_put_levels(
    dst: *mut u8,
    dst_len: usize,
    at: i32,
    src: *const u8,
    src_len: usize,
    count: usize,
) {
    // SAFETY: as documented.
    let (dst, src) = unsafe { (crate::view_mut(dst, dst_len), crate::view(src, src_len)) };
    bembeyaz_dither::put_levels(dst, at, src, count);
}

/// dither.h's `bb_dither_line`.
#[repr(C)]
pub struct CLine {
    gray: *const u8,
    alpha: *const u8,
    len: usize,
    crop: i32,
    visible: i32,
    dst_width: i32,
}

// dither.h's layout; the PNG converter checks the same.
const _: () = assert!(
    size_of::<CLine>() == (3 * size_of::<usize>() + 12).next_multiple_of(align_of::<usize>())
        && core::mem::offset_of!(CLine, crop) == 3 * size_of::<usize>()
        && core::mem::offset_of!(CLine, dst_width) == 3 * size_of::<usize>() + 8
);

/// # Safety
/// `line` is NULL or valid, its `gray` and `alpha` each NULL or holding `len` bytes; `out` is NULL
/// or holds `out_len` writable bytes, and `mask` NULL or `mask_len`.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_dither_line_row(
    line: *const CLine,
    start: i32,
    end: i32,
    ox: i32,
    out_y: i32,
    quantize_value: u8,
    out: *mut u8,
    out_len: usize,
    mask: *mut u8,
    mask_len: usize,
) {
    // SAFETY: as documented.
    let Some(l) = (unsafe { line.as_ref() }) else {
        return;
    };
    let line = Line {
        // SAFETY: as documented.
        gray: unsafe { crate::view(l.gray, l.len) },
        // SAFETY: as documented.
        alpha: (!l.alpha.is_null()).then(|| unsafe { crate::view(l.alpha, l.len) }),
        crop: l.crop,
        visible: l.visible,
        dst_width: l.dst_width,
    };
    // SAFETY: as documented.
    let out = unsafe { crate::view_mut(out, out_len) };
    // SAFETY: as documented.
    let mask = (!mask.is_null()).then(|| unsafe { crate::view_mut(mask, mask_len) });
    bembeyaz_dither::line_row(
        &line,
        (start, end),
        (ox, out_y),
        quantize(quantize_value),
        out,
        mask,
    );
}
