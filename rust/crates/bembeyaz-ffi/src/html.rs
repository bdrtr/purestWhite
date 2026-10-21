//! Exports for `lib/Epub/Epub/htmlEntities` and `lib/Txt/TxtToHtml` (see
//! `rust/include/bembeyaz/html.h`).

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

use bembeyaz_html::text::TextToHtml;
use core::ffi::c_void;

// html.h's bb_text_html.
const _: () = assert!(
    size_of::<TextToHtml>() == 2 * size_of::<usize>()
        && align_of::<TextToHtml>() == align_of::<usize>()
);

/// # Safety
/// `state` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_text_html_init(state: *mut TextToHtml) {
    if !state.is_null() {
        // SAFETY: writable; written whole, never read before.
        unsafe { state.write(TextToHtml::new()) };
    }
}

/// # Safety
/// `state` is NULL or from `bb_text_html_init`; `text` is NULL or `len` readable bytes; `out`
/// takes `context` and the bytes, valid for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_text_html_feed(
    state: *mut TextToHtml,
    text: *const u8,
    len: usize,
    out: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize)>,
    context: *mut c_void,
) {
    // SAFETY: as documented.
    let (Some(state), Some(out)) = (unsafe { state.as_mut() }, out) else {
        return;
    };
    let text = if text.is_null() || len == 0 {
        &[][..]
    } else {
        // SAFETY: len bytes at text.
        unsafe { core::slice::from_raw_parts(text, len) }
    };
    // SAFETY: the caller's callback; each piece stays valid for the call.
    state.feed(text, &mut |piece| unsafe {
        out(context, piece.as_ptr(), piece.len())
    });
}
