//! Exports for `lib/MiniBidi/BidiUtils` (see `rust/include/bembeyaz/bidi.h`).

use bembeyaz_bidi::{self as bidi, Level, MAX_LINE, Visual, Workspace};
use core::ffi::{c_char, c_int, c_void};
use core::mem::{MaybeUninit, align_of, size_of};

pub const BB_BIDI_DONE: c_int = 0;
pub const BB_BIDI_EMPTY: c_int = 1;
pub const BB_BIDI_TOO_LONG: c_int = 2;

/// `bb_bidi_workspace` in the header: the same size, and no stricter alignment than its
/// `uint32_t` array.
const _: () = assert!(size_of::<Workspace>() == 1536 && align_of::<Workspace>() <= 4);
const _: () = assert!(MAX_LINE == 128);

/// `BB_BIDI_OUT_SIZE`: [`bidi::visual`] emits at most one codepoint per character read.
const OUT_SIZE: usize = 4 * MAX_LINE;

type WordFn =
    unsafe extern "C" fn(index: usize, len: *mut usize, context: *const c_void) -> *const c_char;

unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(data.cast(), len) }
    }
}

/// The caller's workspace.
///
/// SAFETY: `workspace` is null or points to a `bb_bidi_workspace` that was zeroed once (a
/// static is) and is used by one call at a time. All-zero bytes are a valid [`Workspace`], and
/// the crate leaves it valid after every call.
unsafe fn workspace<'a>(workspace: *mut c_void) -> Option<&'a mut Workspace> {
    unsafe { workspace.cast::<Workspace>().as_mut() }
}

#[unsafe(no_mangle)]
pub extern "C" fn bb_bidi_is_transparent_mark(cp: u32) -> bool {
    bidi::is_transparent_mark(cp)
}

/// 1 when the first strong character among the first `max_chars` is right-to-left, 0 when
/// left-to-right, else `fallback & 1`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_bidi_paragraph_level(
    text: *const c_char,
    len: usize,
    fallback: c_int,
    max_chars: c_int,
) -> c_int {
    let max_chars = usize::try_from(max_chars).unwrap_or(0);
    match bidi::first_strong_is_rtl(unsafe { bytes(text, len) }, max_chars) {
        Some(rtl) => c_int::from(rtl),
        None => fallback & 1,
    }
}

/// Writes `text` in visual order, as UTF-8, to `out` (`BB_BIDI_OUT_SIZE` bytes, which always
/// hold a line) and its length to `out_len`; `level` is -1 for auto, else its low bit. Both are
/// written only for `BB_BIDI_DONE`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_bidi_visual(
    text: *const c_char,
    len: usize,
    level: c_int,
    workspace: *mut c_void,
    out: *mut u8,
    out_len: *mut usize,
) -> c_int {
    let Some(workspace) = (unsafe { self::workspace(workspace) }) else {
        return BB_BIDI_EMPTY;
    };
    if out.is_null() || out_len.is_null() {
        return BB_BIDI_EMPTY;
    }
    // SAFETY: the caller passes OUT_SIZE writable bytes, which need not be initialised.
    let out = unsafe { core::slice::from_raw_parts_mut(out.cast::<MaybeUninit<u8>>(), OUT_SIZE) };
    let level = match level {
        ..0 => Level::Auto,
        _ if level & 1 == 1 => Level::Rtl,
        _ => Level::Ltr,
    };
    let mut used = 0;
    let result = bidi::visual(unsafe { bytes(text, len) }, level, workspace, |cp| {
        let mut buf = [0u8; 4];
        let piece = bembeyaz_utf8::encode(cp, &mut buf);
        if let Some(dst) = out.get_mut(used..used + piece.len()) {
            dst.write_copy_of_slice(piece);
            used += piece.len();
        }
    });
    if result == Visual::Done {
        unsafe { *out_len = used };
    }
    match result {
        Visual::Done => BB_BIDI_DONE,
        Visual::Empty => BB_BIDI_EMPTY,
        Visual::TooLong => BB_BIDI_TOO_LONG,
    }
}

/// Fills `order[0..count]` with the visual order of `count` words, which `word` hands out by
/// index. True when the caller is to use it: the words moved, or a line without right-to-left
/// text sits in a right-to-left paragraph (the order is then the logical one). False when they
/// keep their order in a left-to-right paragraph or cannot be ordered; `order` is then
/// meaningless.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_bidi_word_order(
    count: usize,
    word: WordFn,
    word_context: *const c_void,
    rtl: bool,
    workspace: *mut c_void,
    order: *mut u16,
) -> bool {
    let Some(workspace) = (unsafe { self::workspace(workspace) }) else {
        return false;
    };
    if order.is_null() || count == 0 || count > MAX_LINE {
        return false;
    }
    // SAFETY: the caller passes `count` initialised, writable entries.
    let order = unsafe { core::slice::from_raw_parts_mut(order, count) };
    bidi::word_order(
        count,
        |i| {
            let mut len = 0;
            let data = unsafe { word(i, &raw mut len, word_context) };
            unsafe { bytes(data, len) }
        },
        rtl,
        workspace,
        order,
    )
    .unwrap_or(false)
}
