//! Exports for lib/GfxRenderer's `Bitmap` (see `rust/include/bembeyaz/bmp.h`).

use bembeyaz_bmp::{Bmp, Source};
use core::ffi::{c_int, c_void};
use core::slice;

// bmp.h's bb_bmp.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Bmp>() == 280
            && offset_of!(Bmp, height) == 4
            && offset_of!(Bmp, row_bytes) == 8
            && offset_of!(Bmp, data_offset) == 12
            && offset_of!(Bmp, colors_used) == 16
            && offset_of!(Bmp, bits_per_pixel) == 20
            && offset_of!(Bmp, top_down) == 22
            && offset_of!(Bmp, native_palette) == 23
            && offset_of!(Bmp, palette_lum) == 24
    );
};

type Read = unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> usize;
type Skip = unsafe extern "C" fn(*mut c_void, u32);
type Seek = unsafe extern "C" fn(*mut c_void, u32) -> bool;

struct File {
    read: Read,
    skip: Skip,
    seek: Seek,
    context: *mut c_void,
}

// SAFETY (every call): the callbacks are the caller's and take the arguments the header
// documents; the buffer stays valid for the call.
impl Source for File {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        unsafe { (self.read)(self.context, buf.as_mut_ptr(), buf.len()) }
    }
    fn skip(&mut self, count: u32) {
        unsafe { (self.skip)(self.context, count) }
    }
    fn seek(&mut self, position: u32) -> bool {
        unsafe { (self.seek)(self.context, position) }
    }
}

/// # Safety
///
/// `bmp` is null or writable; the callbacks follow bmp.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_bmp_parse(
    bmp: *mut Bmp,
    read: Option<Read>,
    skip: Option<Skip>,
    seek: Option<Seek>,
    context: *mut c_void,
) -> c_int {
    // SAFETY: the caller's writable bb_bmp.
    let (Some(bmp), Some(read), Some(skip), Some(seek)) =
        (unsafe { bmp.as_mut() }, read, skip, seek)
    else {
        return 1;
    };
    let mut file = File {
        read,
        skip,
        seek,
        context,
    };
    match bembeyaz_bmp::parse(&mut file, bmp) {
        Ok(()) => 0,
        Err(error) => c_int::from(error as u8),
    }
}

/// # Safety
///
/// `bmp` is readable, `row` null or `len` readable bytes, `out` null or `count` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_bmp_row_luminance(
    bmp: *const Bmp,
    row: *const u8,
    len: usize,
    first: usize,
    out: *mut u8,
    count: usize,
) -> bool {
    // SAFETY: the caller's readable bb_bmp.
    let Some(bmp) = (unsafe { bmp.as_ref() }) else {
        return false;
    };
    let row = if row.is_null() {
        &[][..]
    } else {
        // SAFETY: len readable bytes.
        unsafe { slice::from_raw_parts(row, len) }
    };
    let out = if out.is_null() {
        &mut [][..]
    } else {
        // SAFETY: count writable bytes.
        unsafe { slice::from_raw_parts_mut(out, count) }
    };
    bmp.row_luminance(row, first, out)
}
