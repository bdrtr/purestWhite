//! Exports for `src/util/BookmarkFile` (see `rust/include/bembeyaz/store_bookmarks.h`): a book's
//! bookmarks file read and written by `bembeyaz_store::bookmarks`. Read bookmarks reach the caller
//! one per callback call, and written text through a `bb_store_write` callback, so no Rust buffer
//! outlives a call.

use crate::store::{Text, Write, hand_over};
use alloc::vec::Vec;
use bembeyaz_store::bookmarks::{Bookmark, read_each, write};
use core::ffi::{c_char, c_void};

/// store_bookmarks.h's `bb_store_bookmark`.
#[repr(C)]
pub struct BbBookmark {
    xpath: Text,
    summary: Text,
    name: Text,
    percentage: f32,
    spine_index: u16,
    chapter_page_count: u16,
    chapter_progress: u16,
    has_visible_text_offset: bool,
    visible_text_offset: u32,
}

// store_bookmarks.h's struct, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let text = size_of::<Text>();
    assert!(offset_of!(BbBookmark, percentage) == 3 * text);
    assert!(offset_of!(BbBookmark, chapter_progress) == 3 * text + 8);
    assert!(offset_of!(BbBookmark, has_visible_text_offset) == 3 * text + 10);
    assert!(offset_of!(BbBookmark, visible_text_offset) == 3 * text + 12);
    assert!(size_of::<BbBookmark>() == 3 * text + 16);
};

/// store_bookmarks.h's `bb_store_bookmark_read`.
type ReadBookmark = unsafe extern "C" fn(*const BbBookmark, usize, *mut c_void);

impl BbBookmark {
    fn of(b: &Bookmark<&[u8]>) -> BbBookmark {
        BbBookmark {
            xpath: Text::of(b.xpath),
            summary: Text::of(b.summary),
            name: Text::of(b.name),
            percentage: b.percentage,
            spine_index: b.spine_index,
            chapter_page_count: b.chapter_page_count,
            chapter_progress: b.chapter_progress,
            has_visible_text_offset: b.visible_text_offset.is_some(),
            visible_text_offset: b.visible_text_offset.unwrap_or(0),
        }
    }

    /// # Safety
    /// The texts are NULL or readable and outlive `'a`.
    unsafe fn view<'a>(&self) -> Bookmark<&'a [u8]> {
        // SAFETY: as documented.
        unsafe {
            Bookmark {
                xpath: self.xpath.bytes(),
                summary: self.summary.bytes(),
                name: self.name.bytes(),
                percentage: self.percentage,
                spine_index: self.spine_index,
                chapter_page_count: self.chapter_page_count,
                chapter_progress: self.chapter_progress,
                visible_text_offset: self
                    .has_visible_text_offset
                    .then_some(self.visible_text_offset),
            }
        }
    }
}

/// Hands `write_text` a bookmarks file's text for the `count` bookmarks at `bookmarks`, in one
/// call; false when it could not be made (memory ran out) and `write_text` was not called.
///
/// # Safety
/// `bookmarks` is NULL or `count` readable bookmarks whose texts are NULL or readable;
/// `write_text` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_bookmarks_write(
    bookmarks: *const BbBookmark,
    count: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let bookmarks = unsafe { crate::view(bookmarks, count) };
    // SAFETY: as documented; the texts outlive this call.
    let view: Vec<Bookmark<&[u8]>> = bookmarks.iter().map(|b| unsafe { b.view() }).collect();
    // SAFETY: as documented.
    unsafe { hand_over(write(&view).as_deref(), write_text, context) }
}

/// Reads a bookmarks file's text; NULL when it was read (each bookmark handed to `bookmark` in
/// order, with how many there are), else ArduinoJson's name for why not, and `bookmark` is not
/// called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `bookmark` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_bookmarks_read(
    text: *const c_char,
    len: usize,
    bookmark: Option<ReadBookmark>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let text = unsafe { crate::view(text.cast::<u8>(), len) };
    let handed = read_each(text, |b, count| {
        if let Some(bookmark) = bookmark {
            let entry = BbBookmark::of(b);
            // SAFETY: `entry`'s texts are readable for the call; the caller vouches for bookmark
            // and context.
            unsafe { bookmark(&raw const entry, count, context) };
        }
    });
    match handed {
        Ok(()) => core::ptr::null(),
        Err(error) => error.c_name().as_ptr(),
    }
}
