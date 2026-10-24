//! Exports for `lib/Epub/Epub/htmlEntities`, `lib/Txt/TxtToHtml` and the chapter parser (see
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

use bembeyaz_html::words::{self, InitialLetter, MAX_WORD, Piece, Scan, Words};
use core::ffi::c_int;

/// html.h's `bb_html_words`.
#[repr(C)]
pub struct WordsView {
    buf: *mut u8,
    len: *mut c_int,
    offset: *mut u32,
    continues: *mut bool,
    context: *mut c_void,
    flush: Option<unsafe extern "C" fn(*mut c_void)>,
    suppress_indent: Option<unsafe extern "C" fn(*mut c_void)>,
    line_break: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
    initial: *mut InitialLetter,
    block_empty: Option<unsafe extern "C" fn(*mut c_void) -> bool>,
}

// html.h's BB_HTML_MAX_WORD and bb_html_initial_letter.
const _: () = {
    use core::mem::offset_of;
    assert!(
        MAX_WORD == 200
            && size_of::<InitialLetter>() == 24
            && offset_of!(InitialLetter, len) == 13
            && offset_of!(InitialLetter, pending) == 17
            && offset_of!(InitialLetter, span_depth) == 20
    );
};

// The buffer is read and written through its pointers only between callbacks, which may change it
// too; no reference to it outlives a call.
//
// SAFETY (every method): the view is as html.h describes; `buf` has room for MAX_WORD + 1 bytes.
impl Words for &WordsView {
    fn buffered(&mut self) -> usize {
        // SAFETY: as above.
        usize::try_from(unsafe { *self.len })
            .unwrap_or(0)
            .min(MAX_WORD + 1)
    }
    fn read(&mut self, out: &mut [u8; MAX_WORD]) -> usize {
        let n = self.buffered().min(MAX_WORD);
        // SAFETY: as above; `n` bytes are in the buffer and fit `out`.
        unsafe { ptr::copy_nonoverlapping(self.buf, out.as_mut_ptr(), n) };
        n
    }
    fn set(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(MAX_WORD);
        // SAFETY: as above; `n` bytes fit the buffer.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), self.buf, n);
            *self.len = c_int::try_from(n).unwrap_or(0);
        }
    }
    fn push(&mut self, byte: u8) {
        let n = self.buffered();
        // The parser ends the word with a NUL at its length: at most MAX_WORD.
        if n < MAX_WORD {
            // SAFETY: as above; index n is inside the buffer.
            unsafe {
                *self.buf.add(n) = byte;
                *self.len = c_int::try_from(n + 1).unwrap_or(0);
            }
        }
    }
    fn offset(&mut self) -> u32 {
        // SAFETY: as above.
        unsafe { *self.offset }
    }
    fn set_offset(&mut self, offset: u32) {
        // SAFETY: as above.
        unsafe { *self.offset = offset };
    }
    fn set_continues(&mut self, continues: bool) {
        // SAFETY: as above.
        unsafe { *self.continues = continues };
    }
    fn flush(&mut self) {
        if let Some(f) = self.flush {
            // SAFETY: the caller's callback.
            unsafe { f(self.context) };
        }
    }
    fn suppress_indent(&mut self) {
        if let Some(f) = self.suppress_indent {
            // SAFETY: the caller's callback.
            unsafe { f(self.context) };
        }
    }
    fn line_break(&mut self) -> bool {
        // SAFETY: the caller's callback.
        self.line_break.is_some_and(|f| unsafe { f(self.context) })
    }
    fn initial(&mut self) -> InitialLetter {
        // SAFETY: as above; the letter is checked non-null before the scan.
        unsafe { self.initial.read() }
    }
    fn set_initial(&mut self, letter: &InitialLetter) {
        // SAFETY: as above.
        unsafe { self.initial.write(*letter) };
    }
    fn block_empty(&mut self) -> bool {
        // SAFETY: the caller's callback.
        self.block_empty.is_some_and(|f| unsafe { f(self.context) })
    }
}

/// # Safety
/// `text` is NULL or holds `len` bytes; `words` is NULL or as html.h describes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_scan_words(
    text: *const c_char,
    len: usize,
    counted: bool,
    start: u32,
    preserve: bool,
    footnote_link: bool,
    words: *const WordsView,
) -> c_int {
    // SAFETY: as documented.
    let Some(mut view) = (unsafe { words.as_ref() }) else {
        return 0;
    };
    if view.buf.is_null()
        || view.len.is_null()
        || view.offset.is_null()
        || view.continues.is_null()
        || view.initial.is_null()
    {
        return 0;
    }
    let text = if text.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: as documented.
        unsafe { slice::from_raw_parts(text.cast::<u8>(), len) }
    };
    let piece = Piece {
        counted,
        start,
        preserve,
        footnote_link,
    };
    match words::scan(text, piece, &mut view) {
        Scan::Done => 0,
        Scan::Stopped => 1,
    }
}

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

use bembeyaz_html::list::{self, List, MAX_MARKER};

// html.h's BB_HTML_MAX_MARKER and bb_html_list.
const _: () = assert!(
    MAX_MARKER == 12 && size_of::<List>() == 8 && core::mem::offset_of!(List, counter) == 4
);

/// # Safety
/// `list` is NULL or writable; `out` is NULL or has room for [`MAX_MARKER`] bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_html_list_marker(list: *mut List, out: *mut c_char) -> usize {
    // SAFETY: as documented.
    let Some(out) = (unsafe { out.cast::<[u8; MAX_MARKER]>().as_mut() }) else {
        return 0;
    };
    // SAFETY: as documented.
    list::marker(unsafe { list.as_mut() }, out)
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
