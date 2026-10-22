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
