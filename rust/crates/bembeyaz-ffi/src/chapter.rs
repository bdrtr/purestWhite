//! Exports for the chapter parser's anchors and note references (see
//! `rust/include/bembeyaz/chapter.h`).

use crate::pager::Callbacks;
use bembeyaz_chapter::anchors::Anchors;
use bembeyaz_chapter::footnotes::Footnotes;
use bembeyaz_layout::pager::Pager;
use core::ffi::{c_char, c_void};
use core::mem::MaybeUninit;

// chapter.h's bb_anchors and bb_footnotes (14 uint64_t each), BB_ANCHORS_MAX and the
// BB_FOOTNOTE_*_LEN that the parser checks against FootnoteEntry.
const _: () = assert!(
    bembeyaz_chapter::anchors::MAX_ANCHORS == 1024
        && bembeyaz_chapter::footnotes::NUMBER_LEN == 32
        && bembeyaz_chapter::footnotes::HREF_LEN == 256
);
const _: () = assert!(
    size_of::<Anchors>() <= 14 * 8
        && align_of::<Anchors>() <= 8
        && size_of::<Footnotes>() <= 14 * 8
        && align_of::<Footnotes>() <= 8
);

/// Exports set-up and release of a store whose heap blocks are Rust's.
macro_rules! store {
    ($ty:ty, $init:ident, $free:ident) => {
        /// # Safety
        /// The pointer is NULL or storage for the store, of any contents.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $init(store: *mut MaybeUninit<$ty>) {
            // SAFETY: as documented.
            if let Some(slot) = unsafe { store.as_mut() } {
                slot.write(<$ty>::new());
            }
        }

        /// # Safety
        /// The pointer is NULL or a store that the init function set up, used by nothing else
        /// during the call. It stays usable, empty.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $free(store: *mut $ty) {
            // SAFETY: as documented.
            if let Some(store) = unsafe { store.as_mut() } {
                *store = <$ty>::new();
            }
        }
    };
}

store!(Anchors, bb_anchors_init, bb_anchors_free);
store!(Footnotes, bb_footnotes_init, bb_footnotes_free);

/// # Safety
/// `anchors` is NULL or a `bb_anchors` that `bb_anchors_init` set up, used by nothing else
/// during the call; `name` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_add_toc(
    anchors: *mut Anchors,
    name: *const c_char,
    len: usize,
) -> bool {
    // SAFETY: as documented.
    let (Some(anchors), name) = (unsafe { anchors.as_mut() }, unsafe {
        crate::view(name.cast::<u8>(), len)
    }) else {
        return false;
    };
    anchors.add_toc(name).is_ok()
}

/// # Safety
/// `anchors` as in `bb_anchors_add_toc`; `pager` and `pages` as in pager.h's calls, and the
/// callbacks do not use the anchors; `name` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_open(
    anchors: *mut Anchors,
    pager: *mut Pager,
    pages: *const Callbacks,
    name: *const c_char,
    len: usize,
    navigable: bool,
) -> bool {
    // SAFETY: as documented.
    let (Some(anchors), Some(pager), Some(mut pages)) =
        (unsafe { (anchors.as_mut(), pager.as_mut(), pages.as_ref()) })
    else {
        return false;
    };
    // SAFETY: as documented.
    let name = unsafe { crate::view(name.cast::<u8>(), len) };
    anchors.open(pager, &mut pages, name, navigable).is_ok()
}

/// # Safety
/// As `bb_anchors_open`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_place_pending(
    anchors: *mut Anchors,
    pager: *mut Pager,
    pages: *const Callbacks,
) -> bool {
    // SAFETY: as documented.
    let (Some(anchors), Some(pager), Some(mut pages)) =
        (unsafe { (anchors.as_mut(), pager.as_mut(), pages.as_ref()) })
    else {
        return false;
    };
    anchors.place_pending(pager, &mut pages).is_ok()
}

/// # Safety
/// As `bb_anchors_add_toc`'s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_anchors_record_pending(anchors: *mut Anchors, page: u16) -> bool {
    // SAFETY: as documented.
    unsafe { anchors.as_mut() }.is_some_and(|a| a.record_pending(page).is_ok())
}

/// # Safety
/// `anchors` is NULL or a `bb_anchors` that `bb_anchors_init` set up.
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

/// # Safety
/// `notes` is NULL or a `bb_footnotes` that `bb_footnotes_init` set up, used by nothing else
/// during the call; `href` is NULL or holds `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_footnotes_open(
    notes: *mut Footnotes,
    href: *const c_char,
    len: usize,
) -> bool {
    // SAFETY: as documented.
    let (Some(notes), href) = (unsafe { notes.as_mut() }, unsafe {
        crate::view(href.cast::<u8>(), len)
    }) else {
        return false;
    };
    notes.open(href).is_ok()
}

/// # Safety
/// `notes` is NULL or a `bb_footnotes` that `bb_footnotes_init` set up. The target returned
/// stays valid until the next `bb_footnotes_open` or `bb_footnotes_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_footnotes_href(notes: *const Footnotes) -> *const c_char {
    // SAFETY: as documented.
    unsafe { notes.as_ref() }.map_or(c"".as_ptr(), |n| n.href().as_ptr().cast())
}

/// # Safety
/// As `bb_footnotes_open`'s, with `text` for `href`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_footnotes_text(notes: *mut Footnotes, text: *const c_char, len: usize) {
    // SAFETY: as documented.
    if let Some(notes) = unsafe { notes.as_mut() } {
        notes.text(unsafe { crate::view(text.cast::<u8>(), len) });
    }
}

/// # Safety
/// `notes` as in `bb_footnotes_open`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_footnotes_close(notes: *mut Footnotes, word: i32) -> bool {
    // SAFETY: as documented.
    unsafe { notes.as_mut() }.is_some_and(|n| n.close(word).is_ok())
}

type Add = unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char);

/// # Safety
/// `notes` as in `bb_footnotes_open`; `add` follows chapter.h and does not use the notes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_footnotes_take(
    notes: *mut Footnotes,
    words: i32,
    add: Option<Add>,
    context: *mut c_void,
) {
    // SAFETY: as documented.
    let (Some(notes), Some(add)) = (unsafe { notes.as_mut() }, add) else {
        return;
    };
    notes.take(words, |number, href| {
        // SAFETY: both are NUL-terminated and live for the call; the callback is the caller's.
        unsafe { add(context, number.as_ptr().cast(), href.as_ptr().cast()) }
    });
}

/// # Safety
/// `notes` is NULL or a `bb_footnotes` that `bb_footnotes_init` set up.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_footnotes_has_pending(notes: *const Footnotes) -> bool {
    // SAFETY: as documented.
    unsafe { notes.as_ref() }.is_some_and(Footnotes::has_pending)
}
