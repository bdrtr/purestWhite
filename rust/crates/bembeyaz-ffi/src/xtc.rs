//! Exports for lib/Xtc's `XtcParser` (see `rust/include/bembeyaz/xtc.h`).

use bembeyaz_xtc::{Book, Error, Source};
use core::ffi::{c_char, c_int, c_void};
use core::mem::offset_of;
use core::slice;

// xtc.h's bb_xtc.
const _: () = assert!(
    size_of::<Book>() == 216
        && offset_of!(Book, data_offset) == 8
        && offset_of!(Book, page_count) == 16
        && offset_of!(Book, width) == 18
        && offset_of!(Book, height) == 20
        && offset_of!(Book, bit_depth) == 22
        && offset_of!(Book, has_chapters) == 23
        && offset_of!(Book, title_len) == 24
        && offset_of!(Book, author_len) == 25
        && offset_of!(Book, title) == 26
        && offset_of!(Book, author) == 153
);

/// `bb_xtc_file`.
#[repr(C)]
pub struct File {
    read_at: Option<unsafe extern "C" fn(*mut c_void, u64, *mut u8, usize) -> i64>,
    size: Option<unsafe extern "C" fn(*mut c_void) -> u64>,
    context: *mut c_void,
}

/// `bb_xtc_page`.
#[repr(C)]
pub struct CPage {
    offset: u64,
    size: u32,
    width: u16,
    height: u16,
}

// SAFETY (both calls): the callbacks are the caller's and take the arguments the header
// documents; the buffer stays valid for the call.
impl Source for &File {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Option<usize> {
        let f = self.read_at?;
        let count = unsafe { f(self.context, offset, buf.as_mut_ptr(), buf.len()) };
        usize::try_from(count).ok().map(|n| n.min(buf.len()))
    }
    fn size(&mut self) -> u64 {
        self.size.map_or(0, |f| unsafe { f(self.context) })
    }
}

fn code(result: Result<(), Error>) -> c_int {
    result.map_or_else(|error| c_int::from(error as u8), |()| 0)
}

/// # Safety
///
/// `book` and `file` are null or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xtc_open(book: *mut Book, file: *const File) -> c_int {
    // SAFETY: the caller's pointers.
    let (Some(book), Some(mut file)) = (unsafe { book.as_mut() }, unsafe { file.as_ref() }) else {
        return c_int::from(Error::ReadError as u8);
    };
    code(bembeyaz_xtc::open(&mut file, book))
}

/// # Safety
///
/// `book`, `file` and `out` are null or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xtc_page_info(
    book: *const Book,
    file: *const File,
    index: u32,
    out: *mut CPage,
) -> bool {
    // SAFETY: the caller's pointers.
    let (Some(book), Some(mut file), Some(out)) =
        (unsafe { book.as_ref() }, unsafe { file.as_ref() }, unsafe {
            out.as_mut()
        })
    else {
        return false;
    };
    let Some(page) = bembeyaz_xtc::page(&mut file, book, index) else {
        return false;
    };
    *out = CPage {
        offset: page.offset,
        size: page.size,
        width: page.width,
        height: page.height,
    };
    true
}

/// # Safety
///
/// `book`, `file` and `loaded` are null or valid; `buf` is null or has `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xtc_load_page(
    book: *const Book,
    file: *const File,
    index: u32,
    buf: *mut u8,
    len: usize,
    loaded: *mut usize,
) -> c_int {
    // SAFETY: the caller's pointers.
    let (Some(book), Some(mut file)) = (unsafe { book.as_ref() }, unsafe { file.as_ref() }) else {
        return c_int::from(Error::ReadError as u8);
    };
    let buf = if buf.is_null() {
        &mut [][..]
    } else {
        // SAFETY: len writable bytes.
        unsafe { slice::from_raw_parts_mut(buf, len) }
    };
    let result = bembeyaz_xtc::load_page(&mut file, book, index, buf);
    // SAFETY: the caller's pointer.
    if let Some(loaded) = unsafe { loaded.as_mut() } {
        *loaded = *result.as_ref().unwrap_or(&0);
    }
    code(result.map(|_| ()))
}

/// # Safety
///
/// `book` and `file` are null or valid; `chunk` is null or has `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xtc_stream_page(
    book: *const Book,
    file: *const File,
    index: u32,
    chunk: *mut u8,
    len: usize,
    on_chunk: Option<unsafe extern "C" fn(*mut c_void, *const u8, usize, usize)>,
    context: *mut c_void,
) -> c_int {
    // SAFETY: the caller's pointers.
    let (Some(book), Some(mut file), Some(on_chunk)) =
        (unsafe { book.as_ref() }, unsafe { file.as_ref() }, on_chunk)
    else {
        return c_int::from(Error::ReadError as u8);
    };
    let chunk = if chunk.is_null() {
        &mut [][..]
    } else {
        // SAFETY: len writable bytes.
        unsafe { slice::from_raw_parts_mut(chunk, len) }
    };
    code(bembeyaz_xtc::stream_page(
        &mut file,
        book,
        index,
        chunk,
        |data, offset| {
            // SAFETY: the caller's callback; data stays valid for the call.
            unsafe {
                on_chunk(
                    context,
                    data.as_ptr(),
                    data.len(),
                    usize::try_from(offset).unwrap_or(usize::MAX),
                );
            }
        },
    ))
}

/// # Safety
///
/// `book`, `file` and `listed` are null or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xtc_chapters(
    book: *const Book,
    file: *const File,
    on_table: Option<unsafe extern "C" fn(*mut c_void, usize)>,
    on_chapter: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, u16, u16)>,
    context: *mut c_void,
    listed: *mut bool,
) -> c_int {
    // SAFETY: the caller's pointers.
    let (Some(book), Some(mut file), Some(on_chapter)) = (
        unsafe { book.as_ref() },
        unsafe { file.as_ref() },
        on_chapter,
    ) else {
        return c_int::from(Error::ReadError as u8);
    };
    let records = |count: u64| {
        if let Some(on_table) = on_table {
            // SAFETY: the caller's callback.
            unsafe { on_table(context, usize::try_from(count).unwrap_or(usize::MAX)) };
        }
    };
    let result = bembeyaz_xtc::chapters(&mut file, book, records, |name, first, last| {
        // SAFETY: the caller's callback; the name stays valid for the call.
        unsafe { on_chapter(context, name.as_ptr().cast(), name.len(), first, last) };
    });
    // SAFETY: the caller's pointer.
    if let Some(listed) = unsafe { listed.as_mut() } {
        *listed = result == Ok(true);
    }
    code(result.map(|_| ()))
}

/// # Safety
///
/// `bytes` is null or has `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_xtc_is_book_magic(bytes: *const u8, len: usize) -> bool {
    if bytes.is_null() {
        return false;
    }
    // SAFETY: len readable bytes.
    bembeyaz_xtc::is_book_magic(unsafe { slice::from_raw_parts(bytes, len) })
}
