//! Exports for `src/RecentBooksStore` (see `rust/include/bembeyaz/store_recent_books.h`):
//! `recent.json` read and written by `bembeyaz_store::recent_books`. Read books reach the caller
//! one per callback call, and written text through a `bb_store_write` callback, so no Rust buffer
//! outlives a call.

use crate::store::{Text, Write, hand_over};
use bembeyaz_store::recent_books::{Book, read, write};
use core::ffi::{c_char, c_void};

/// store_recent_books.h's `bb_store_recent_book`.
#[repr(C)]
pub struct BbBook {
    path: Text,
    title: Text,
    author: Text,
    cover_bmp_path: Text,
}

// store_recent_books.h's struct, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let text = size_of::<Text>();
    assert!(
        size_of::<BbBook>() == 4 * text
            && offset_of!(BbBook, title) == text
            && offset_of!(BbBook, cover_bmp_path) == 3 * text
    );
};

/// store_recent_books.h's `bb_store_recent_book_read`.
type ReadBook = unsafe extern "C" fn(*const BbBook, usize, *mut c_void);

/// Hands `write_text` the `recent.json` text for the `count` books at `books`, in one call; false
/// when it could not be made (memory ran out) and `write_text` was not called.
///
/// # Safety
/// `books` is NULL or `count` readable books whose texts are NULL or readable; `write_text` takes
/// `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_recent_books_write(
    books: *const BbBook,
    count: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let books = unsafe { crate::view(books, count) };
    let view: alloc::vec::Vec<Book<&[u8]>> = books
        .iter()
        .map(|b| {
            // SAFETY: as documented; the texts outlive this call.
            unsafe {
                Book {
                    path: b.path.bytes(),
                    title: b.title.bytes(),
                    author: b.author.bytes(),
                    cover_bmp_path: b.cover_bmp_path.bytes(),
                }
            }
        })
        .collect();
    // SAFETY: as documented.
    unsafe { hand_over(write(&view).as_deref(), write_text, context) }
}

/// Reads `recent.json`'s text, at most `most` books; NULL when it was read (each book handed to
/// `book` in order, with how many there are), else ArduinoJson's name for why not, and `book` is
/// not called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `book` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_recent_books_read(
    text: *const c_char,
    len: usize,
    most: usize,
    book: Option<ReadBook>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let books = match read(unsafe { crate::view(text.cast::<u8>(), len) }, most) {
        Ok(books) => books,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(book) = book {
        for b in &books {
            let entry = BbBook {
                path: Text::of(&b.path),
                title: Text::of(&b.title),
                author: Text::of(&b.author),
                cover_bmp_path: Text::of(&b.cover_bmp_path),
            };
            // SAFETY: `entry`'s texts are readable for the call; the caller vouches for book and
            // context.
            unsafe { book(&raw const entry, books.len(), context) };
        }
    }
    core::ptr::null()
}
