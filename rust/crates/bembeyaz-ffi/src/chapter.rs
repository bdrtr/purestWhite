//! Exports for reading the chapter's anchors (see `rust/include/bembeyaz/chapter.h`).

use bembeyaz_chapter::anchors::{Anchors, MAX_ANCHORS};
use bembeyaz_chapter::footnotes::{HREF_LEN, NUMBER_LEN};
use core::ffi::c_char;

// chapter.h's BB_FOOTNOTE_*_LEN, which the parser checks against FootnoteEntry, and the anchor
// limit rust/README.md gives.
const _: () = assert!(MAX_ANCHORS == 1024 && NUMBER_LEN == 32 && HREF_LEN == 256);

/// # Safety
/// `anchors` is NULL or from `bb_chapter_anchors`, valid for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_len(anchors: *const Anchors) -> usize {
    // SAFETY: as documented.
    unsafe { anchors.as_ref() }.map_or(0, Anchors::len)
}

/// # Safety
/// As `bb_anchors_len`'s; `len` and `page` are NULL or writable. The name returned stays valid
/// until the anchors change.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_get(
    anchors: *const Anchors,
    i: usize,
    len: *mut usize,
    page: *mut u16,
) -> *const c_char {
    // SAFETY: as documented.
    let Some((name, at)) = (unsafe { anchors.as_ref() }).and_then(|a| a.get(i)) else {
        return core::ptr::null();
    };
    // SAFETY: as documented.
    unsafe {
        crate::set(len, name.len());
        crate::set(page, at);
    }
    name.as_ptr().cast()
}

/// # Safety
/// As `bb_anchors_len`'s; `name` is NULL or holds `len` bytes; `page` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_find(
    anchors: *const Anchors,
    name: *const c_char,
    len: usize,
    page: *mut u16,
) -> bool {
    // SAFETY: as documented.
    let Some(anchors) = (unsafe { anchors.as_ref() }) else {
        return false;
    };
    // SAFETY: as documented.
    let Some(found) = anchors.find(unsafe { crate::view(name.cast::<u8>(), len) }) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { crate::set(page, found) };
    true
}
