//! Exports for lib/LibraryIndex's `LibraryText` (see `rust/include/bembeyaz/library.h`).

use alloc::vec::Vec;
use bembeyaz_library::OutOfMemory;
use core::ffi::{c_char, c_void};
use core::slice;

type Out = unsafe extern "C" fn(*mut c_void, *const c_char, usize);

// library.h's BB_LIBRARY_AUTHOR_KEY_MAX_BYTES.
const _: () = assert!(bembeyaz_library::AUTHOR_KEY_MAX_BYTES == 12);

/// # Safety
/// `text` is NULL or `len` readable bytes.
unsafe fn bytes<'a>(text: *const c_char, len: usize) -> &'a [u8] {
    if text.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: as documented.
        unsafe { slice::from_raw_parts(text.cast(), len) }
    }
}

/// Runs `f` and hands its result to `out`.
///
/// # Safety
/// `text` as for [`bytes`]; `out` takes `context` and the result, valid for the call.
unsafe fn give(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
    f: fn(&[u8], &mut Vec<u8>) -> Result<(), OutOfMemory>,
) -> bool {
    let Some(out) = out else {
        return false;
    };
    let mut result = Vec::new();
    // SAFETY: as documented.
    if f(unsafe { bytes(text, len) }, &mut result).is_err() {
        return false;
    }
    // SAFETY: the caller's callback; the result stays valid for the call.
    unsafe { out(context, result.as_ptr().cast(), result.len()) };
    true
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_fold(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::fold) }
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_clean_person_name(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::clean_person_name) }
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_author_key(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::author_key) }
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_surname_key(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::surname_key) }
}

/// # Safety
/// `folded` is NULL or `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_folded_group_initial(folded: *const c_char, len: usize) -> u32 {
    // SAFETY: as documented.
    bembeyaz_library::folded_group_initial(unsafe { bytes(folded, len) })
}

/// # Safety
/// `haystack` and `needle` are NULL or their lengths' readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_matches_query(
    haystack: *const c_char,
    haystack_len: usize,
    needle: *const c_char,
    needle_len: usize,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        bembeyaz_library::matches_query(bytes(haystack, haystack_len), bytes(needle, needle_len))
    }
}
