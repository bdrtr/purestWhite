//! Exports for `lib/FsHelpers` (see `rust/include/bembeyaz/path.h`).

use core::ffi::c_char;
use core::slice;

/// # Safety
/// `path` points to `len` writable bytes (or `len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_decode_uri_escapes(path: *mut c_char, len: usize) -> usize {
    if path.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller's buffer.
    bembeyaz_path::decode_uri_escapes(unsafe { slice::from_raw_parts_mut(path.cast::<u8>(), len) })
}

/// # Safety
/// `path` points to `len` writable bytes (or `len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_normalise(path: *mut c_char, len: usize) -> usize {
    if path.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller's buffer.
    bembeyaz_path::normalise(unsafe { slice::from_raw_parts_mut(path.cast::<u8>(), len) })
}
