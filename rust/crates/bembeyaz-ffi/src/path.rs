//! Exports for `lib/FsHelpers` (see `rust/include/bembeyaz/path.h`).

use bembeyaz_path::web;
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

/// # Safety
/// `bytes` is NULL or holds `len` bytes.
unsafe fn bytes<'a>(bytes: *const c_char, len: usize) -> &'a [u8] {
    // SAFETY: as documented.
    unsafe { crate::view(bytes.cast::<u8>(), len) }
}

/// # Safety
/// `name` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_is_system_name(name: *const c_char, len: usize) -> bool {
    // SAFETY: as documented.
    web::is_system_name(unsafe { bytes(name, len) })
}

/// # Safety
/// `name` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_is_hidden_name(name: *const c_char, len: usize) -> bool {
    // SAFETY: as documented.
    web::is_hidden_name(unsafe { bytes(name, len) })
}

/// # Safety
/// `path` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_has_hidden_component(path: *const c_char, len: usize) -> bool {
    // SAFETY: as documented.
    web::has_hidden_component(unsafe { bytes(path, len) })
}

/// # Safety
/// `path` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_is_sensitive(path: *const c_char, len: usize) -> bool {
    // SAFETY: as documented.
    web::is_sensitive(unsafe { bytes(path, len) })
}

/// # Safety
/// `path` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_is_plugin_path(path: *const c_char, len: usize) -> bool {
    // SAFETY: as documented.
    web::is_plugin_path(unsafe { bytes(path, len) })
}

/// # Safety
/// `path` is NULL or points to `len + 1` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_web(path: *mut c_char, len: usize) -> usize {
    let Some(room) = len.checked_add(1) else {
        return 0;
    };
    if path.is_null() {
        return 0;
    }
    // SAFETY: the caller's buffer, one byte longer than its text.
    web::web_path(
        unsafe { slice::from_raw_parts_mut(path.cast::<u8>(), room) },
        len,
    )
}

/// # Safety
/// `url` is NULL or holds `len` bytes; `start` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_url_path_start(
    url: *const c_char,
    len: usize,
    start: *mut usize,
) -> bool {
    // SAFETY: as documented.
    let Some(at) = web::url_path_start(unsafe { bytes(url, len) }) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { crate::set(start, at) };
    true
}

/// # Safety
/// `path` is NULL or holds `len` bytes; `out` is NULL or holds `3 * len` writable bytes that do not
/// overlap `path`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_path_encode(
    path: *const c_char,
    len: usize,
    out: *mut c_char,
) -> usize {
    // SAFETY: as documented.
    let out = unsafe { crate::view_mut(out.cast::<u8>(), len.saturating_mul(3)) };
    // SAFETY: as documented.
    web::encode_path(unsafe { bytes(path, len) }, out)
}
