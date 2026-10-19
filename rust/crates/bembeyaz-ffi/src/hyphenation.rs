//! Exports for `lib/Epub/Epub/hyphenation` (see `rust/include/bembeyaz/hyphenation.h`).

use bembeyaz_hyphenation::{self as hyphenation, Language};
use core::ffi::{c_char, c_int, c_void};
use core::slice;

type EmitFn = unsafe extern "C" fn(context: *mut c_void, offset: usize, inserted_hyphen: bool);

unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { slice::from_raw_parts(data.cast(), len) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_hyph_language(tag: *const c_char, len: usize) -> c_int {
    hyphenation::language(unsafe { bytes(tag, len) })
        .and_then(|l| c_int::try_from(l.index()).ok())
        .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_hyph_breaks(
    language: c_int,
    word: *const c_char,
    len: usize,
    fallback: bool,
    emit: Option<EmitFn>,
    context: *mut c_void,
) {
    let Some(emit) = emit else {
        return;
    };
    let language = self::language(language);
    hyphenation::breaks(
        unsafe { bytes(word, len) },
        language,
        fallback,
        |b| unsafe {
            emit(context, b.offset, b.inserted_hyphen);
        },
    );
}

/// The language of `bb_hyph_language`'s index; -1 (or any other) for none.
pub(crate) fn language(index: c_int) -> Option<Language> {
    usize::try_from(index).ok().and_then(Language::from_index)
}
