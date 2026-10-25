//! Exports for `lib/Epub/Epub/htmlEntities`, `lib/Txt/TxtToHtml` and the chapter parser's host
//! tests (see `rust/include/bembeyaz/html.h`).

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

use bembeyaz_html::words;

use bembeyaz_html::element;

// html.h's BB_HTML_* element flags and names.
const _: () = assert!(
    element::HEADING_LEVEL == 0x7
        && element::BLOCK == 1 << 3
        && element::BOLD == 1 << 4
        && element::ITALIC == 1 << 5
        && element::UNDERLINE == 1 << 6
        && element::LINE_THROUGH == 1 << 7
        && element::IMAGE == 1 << 8
        && element::TABLE == 1 << 9
        && element::HIDDEN == 1 << 10
        && element::NAME == 0x1f << 11
        && element::name::A == 1 << 11
        && element::name::HR == 2 << 11
        && element::name::SPAN == 3 << 11
        && element::name::SUP == 4 << 11
        && element::name::SUB == 5 << 11
        && element::name::RUBY == 6 << 11
        && element::name::RT == 7 << 11
        && element::name::HTML == 8 << 11
        && element::name::BODY == 9 << 11
        && element::name::LI == 10 << 11
        && element::name::OL == 11 << 11
        && element::name::UL == 12 << 11
        && element::name::P == 13 << 11
        && element::name::PRE == 14 << 11
        && element::name::BR == 15 << 11
        && element::name::TABLE == 16 << 11
        && element::name::TR == 17 << 11
        && element::name::TD == 18 << 11
        && element::name::TH == 19 << 11
);

/// `len` bytes at `data`, or none for NULL or 0.
///
/// # Safety
/// `data` is NULL or holds `len` bytes.
unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    // SAFETY: as documented.
    unsafe { crate::view(data.cast::<u8>(), len) }
}

/// # Safety
/// `name` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_element(name: *const c_char, len: usize) -> u16 {
    // SAFETY: as documented.
    element::kind(unsafe { bytes(name, len) })
}

/// # Safety
/// `value` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_table_span(value: *const c_char, len: usize) -> u16 {
    // SAFETY: as documented.
    match element::table_span(unsafe { bytes(value, len) }) {
        element::Span::Cells(n) => n.get(),
        element::Span::Rest => u16::MAX,
    }
}

/// # Safety
/// `href` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_internal_link(href: *const c_char, len: usize) -> bool {
    // SAFETY: as documented.
    element::is_internal_link(unsafe { bytes(href, len) })
}

/// # Safety
/// `text` is NULL or holds `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_collapse_space(text: *mut c_char, len: usize) -> usize {
    // SAFETY: as documented.
    words::collapse_space(unsafe { crate::view_mut(text.cast::<u8>(), len) })
}

/// # Safety
/// `html` is NULL or holds `len` bytes; `out` is callable with `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_normalize_xhtml(
    html: *const u8,
    len: usize,
    out: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize) -> bool>,
    context: *mut c_void,
) -> bool {
    let Some(out) = out else {
        return false;
    };
    // SAFETY: as documented.
    let html = unsafe { crate::view(html, len) };
    // SAFETY: the caller's callback; each piece stays valid for the call.
    bembeyaz_html::xhtml::normalize(html, |piece| unsafe {
        out(context, piece.as_ptr(), piece.len())
    })
}

/// # Safety
/// `html` is NULL or holds `len` bytes; `out` is NULL or writable for `out_len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_to_plain(
    html: *const u8,
    len: usize,
    out: *mut u8,
    out_len: usize,
) -> usize {
    // SAFETY: as documented.
    let (html, out) = unsafe { (crate::view(html, len), crate::view_mut(out, out_len)) };
    bembeyaz_html::plain::to_plain(html, out)
}
