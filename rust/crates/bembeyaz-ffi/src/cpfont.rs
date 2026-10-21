//! Exports for lib/EpdFont's `SdCardFont` (see `rust/include/bembeyaz/cpfont.h`).

use bembeyaz_font::cpfont::{self, Header, IntervalCheck, IntervalFault, Style};
use core::ffi::{c_int, c_void};

// cpfont.h's bb_cpfont_style, bb_cpfont_header, bb_cpfont_intervals, bb_cpfont_interval_fault,
// BB_CPFONT_FORMAT_VERSION and BB_CPFONT_MAX_STYLES.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Style>() == 52
            && offset_of!(Style, bitmap_offset) == 32
            && offset_of!(Style, ascender) == 36
            && offset_of!(Style, kern_right_entries) == 42
            && offset_of!(Style, advance_y) == 44
            && offset_of!(Style, present) == 48
            && size_of::<Header>() == 220
            && offset_of!(Header, content_hash) == 208
            && offset_of!(Header, version) == 212
            && offset_of!(Header, two_bit) == 214
            && offset_of!(Header, style_count) == 215
            && offset_of!(Header, failed_entry) == 216
            && size_of::<IntervalCheck>() == 16
            && size_of::<IntervalFault>() == 12
            && offset_of!(IntervalFault, span) == 8
            && align_of::<IntervalCheck>() == 4
            && cpfont::VERSION == 4
            && cpfont::MAX_STYLES == 4
    );
};

type Read = unsafe extern "C" fn(*mut c_void, *mut u8, usize) -> usize;

/// # Safety
///
/// `header` is null or writable; `read` fills up to `len` bytes of `buf` and returns how many.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_read_header(
    header: *mut Header,
    read: Option<Read>,
    context: *mut c_void,
) -> c_int {
    let (false, Some(read)) = (header.is_null(), read) else {
        return -1;
    };
    // SAFETY: the caller's writable bb_cpfont_header, which may not be set yet.
    let header = unsafe {
        header.write(Header::default());
        &mut *header
    };
    // SAFETY: the callback is the caller's; the buffer stays valid for the call.
    let result = cpfont::read_header(header, |buf| {
        unsafe { read(context, buf.as_mut_ptr(), buf.len()) }.min(buf.len())
    });
    match result {
        Ok(()) => 0,
        Err(error) => c_int::from(error as u8),
    }
}

/// # Safety
///
/// `check` is null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_intervals_init(check: *mut IntervalCheck, glyph_count: u32) {
    if !check.is_null() {
        // SAFETY: the caller's writable bb_cpfont_intervals, which may not be set up yet.
        unsafe { check.write(IntervalCheck::new(glyph_count)) };
    }
}

/// # Safety
///
/// `check` is null or a `bb_cpfont_intervals` set up by `bb_cpfont_intervals_init`; `fault` is
/// null or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_intervals_check(
    check: *mut IntervalCheck,
    first: u32,
    last: u32,
    offset: u32,
    fault: *mut IntervalFault,
) -> bool {
    // SAFETY: the caller's bb_cpfont_intervals.
    let Some(check) = (unsafe { check.as_mut() }) else {
        return false;
    };
    match check.check(first, last, offset) {
        Ok(()) => true,
        Err(f) => {
            if !fault.is_null() {
                // SAFETY: the caller's writable fault, which may not be set yet.
                unsafe { fault.write(f) };
            }
            false
        }
    }
}

/// # Safety
///
/// `check` is null or a `bb_cpfont_intervals` set up by `bb_cpfont_intervals_init`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_cpfont_intervals_bmp16(check: *const IntervalCheck) -> bool {
    // SAFETY: the caller's bb_cpfont_intervals.
    unsafe { check.as_ref() }.is_some_and(IntervalCheck::bmp16)
}
