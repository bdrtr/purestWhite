//! Exports for `lib/Epub/Epub/htmlEntities` (see `rust/include/bembeyaz/html.h`).

use core::ffi::c_char;
use core::{ptr, slice};

/// # Safety
/// `name` points to `len` bytes (or `len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_entity(name: *const c_char, len: usize) -> *const c_char {
    let name = if name.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at name, as the caller promises.
        unsafe { slice::from_raw_parts(name.cast::<u8>(), len) }
    };
    bembeyaz_html::entity(name).map_or(ptr::null(), |value| value.as_c_str().cast())
}
