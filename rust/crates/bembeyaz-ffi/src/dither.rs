//! Exports for lib/GfxRenderer's BitmapHelpers (see `rust/include/bembeyaz/dither.h`).

use bembeyaz_dither::{Ditherer, Method};

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
