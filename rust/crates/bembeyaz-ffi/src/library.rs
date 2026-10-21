//! Exports for lib/LibraryIndex's `LibraryText` (see `rust/include/bembeyaz/library.h`).

use alloc::vec::Vec;
use bembeyaz_library::OutOfMemory;
use core::ffi::{c_char, c_void};
use core::slice;

type Out = unsafe extern "C" fn(*mut c_void, *const c_char, usize);

// library.h's BB_LIBRARY_AUTHOR_KEY_MAX_BYTES.
const _: () = assert!(bembeyaz_library::AUTHOR_KEY_MAX_BYTES == 12);

/// # Safety
/// `text` is NULL or `len` readable bytes.
unsafe fn bytes<'a>(text: *const c_char, len: usize) -> &'a [u8] {
    if text.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: as documented.
        unsafe { slice::from_raw_parts(text.cast(), len) }
    }
}

/// Runs `f` and hands its result to `out`.
///
/// # Safety
/// `text` as for [`bytes`]; `out` takes `context` and the result, valid for the call.
unsafe fn give(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
    f: fn(&[u8], &mut Vec<u8>) -> Result<(), OutOfMemory>,
) -> bool {
    let Some(out) = out else {
        return false;
    };
    let mut result = Vec::new();
    // SAFETY: as documented.
    if f(unsafe { bytes(text, len) }, &mut result).is_err() {
        return false;
    }
    // SAFETY: the caller's callback; the result stays valid for the call.
    unsafe { out(context, result.as_ptr().cast(), result.len()) };
    true
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_fold(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::fold) }
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_clean_person_name(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::clean_person_name) }
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_author_key(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::author_key) }
}

/// # Safety
/// See library.h.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_surname_key(
    text: *const c_char,
    len: usize,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    unsafe { give(text, len, out, context, bembeyaz_library::surname_key) }
}

/// # Safety
/// `folded` is NULL or `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_folded_group_initial(folded: *const c_char, len: usize) -> u32 {
    // SAFETY: as documented.
    bembeyaz_library::folded_group_initial(unsafe { bytes(folded, len) })
}

/// # Safety
/// `haystack` and `needle` are NULL or their lengths' readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_matches_query(
    haystack: *const c_char,
    haystack_len: usize,
    needle: *const c_char,
    needle_len: usize,
) -> bool {
    // SAFETY: as documented.
    unsafe {
        bembeyaz_library::matches_query(bytes(haystack, haystack_len), bytes(needle, needle_len))
    }
}

use bembeyaz_library::format::{self, Header, Identity, Order, Record, Source};

// library.h's index types are LibraryFormat.h's ClixHeader, ClixRecord and BookIdentity.
const _: () = {
    use core::mem::offset_of;
    assert!(
        size_of::<Header>() == 64
            && offset_of!(Header, folder_start) == 16
            && offset_of!(Header, self_size) == 40
            && size_of::<Record>() == 128
            && offset_of!(Record, fold) == 16
            && offset_of!(Record, modification_time) == 124
            && size_of::<Identity>() == 16
            && offset_of!(Identity, file_size) == 8
    );
    // library.h's BB_LIBRARY_FORMAT_VERSION, BB_LIBRARY_FOLD_VERSION and BB_LIBRARY_MAX_RECORDS.
    assert!(
        format::FORMAT_VERSION == 2 && format::FOLD_VERSION == 4 && format::MAX_RECORDS == 4096
    );
};

/// `bb_library_index_file`.
#[repr(C)]
pub struct IndexFile {
    read_at: Option<unsafe extern "C" fn(*mut c_void, u32, *mut c_void, usize) -> bool>,
    fail: Option<unsafe extern "C" fn(*mut c_void)>,
    context: *mut c_void,
}

// SAFETY (both calls): the callbacks are the caller's and take the arguments the header
// documents; the buffer stays valid for the call.
impl Source for &IndexFile {
    fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
        self.read_at
            .is_some_and(|f| unsafe { f(self.context, offset, buf.as_mut_ptr().cast(), buf.len()) })
    }
    fn fail(&mut self) {
        if let Some(f) = self.fail {
            unsafe { f(self.context) };
        }
    }
}

/// # Safety
/// `header` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_layout_sections(
    header: *mut Header,
    folder_bytes: u32,
    name_bytes: u32,
) {
    // SAFETY: as documented.
    if let Some(h) = unsafe { header.as_mut() } {
        format::layout_sections(h, folder_bytes, name_bytes);
    }
}

/// # Safety
/// `header` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_validate_header(
    header: *const Header,
    file_size: u64,
    structure_only: bool,
) -> u8 {
    // SAFETY: as documented.
    let Some(h) = (unsafe { header.as_ref() }) else {
        return format::Validity::BadMagic as u8;
    };
    if structure_only {
        format::validate_structure(h, file_size) as u8
    } else {
        format::validate(h, file_size) as u8
    }
}

/// # Safety
/// `path` is NULL or `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_path_hash(path: *const c_char, len: usize) -> u64 {
    // SAFETY: as documented.
    format::path_hash(unsafe { bytes(path, len) })
}

/// # Safety
/// `header` and `file` are NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_ordinal_for_row(
    header: *const Header,
    order: u8,
    row: u16,
    file: *const IndexFile,
) -> u16 {
    // SAFETY: as documented.
    let (Some(h), Some(mut file), Some(order)) = (
        unsafe { header.as_ref() },
        unsafe { file.as_ref() },
        Order::from_u8(order),
    ) else {
        return 0xFFFF;
    };
    format::ordinal_for_row(h, order, row, &mut file)
}

/// # Safety
/// `header`, `file` and `out` are NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_read_record(
    header: *const Header,
    ordinal: u16,
    file: *const IndexFile,
    out: *mut Record,
) -> bool {
    // SAFETY: as documented.
    let (Some(h), Some(mut file), Some(out)) = (
        unsafe { header.as_ref() },
        unsafe { file.as_ref() },
        unsafe { out.as_mut() },
    ) else {
        return false;
    };
    format::read_record(h, ordinal, &mut file, out)
}

/// # Safety
/// `header`, `record`, `file` and `out` are NULL or valid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_read_path_hash(
    header: *const Header,
    record: *const Record,
    file: *const IndexFile,
    out: *mut u64,
) -> bool {
    // SAFETY: as documented.
    let (Some(h), Some(record), Some(mut file), Some(out)) = (
        unsafe { header.as_ref() },
        unsafe { record.as_ref() },
        unsafe { file.as_ref() },
        unsafe { out.as_mut() },
    ) else {
        return false;
    };
    *out = 0;
    match format::read_path_hash(h, record, &mut file) {
        Some(hash) => {
            *out = hash;
            true
        }
        None => false,
    }
}

/// `what`: library.h's `BB_LIBRARY_NAME` (0) to `BB_LIBRARY_PATH` (4). The string read so far goes to `out`
/// either way; the result says whether it is the field.
///
/// # Safety
/// `header`, `record` and `file` are NULL or valid; `out` takes `context` and the bytes, valid for
/// the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_read_string(
    header: *const Header,
    record: *const Record,
    what: u8,
    file: *const IndexFile,
    out: Option<Out>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let (Some(h), Some(record), Some(mut file), Some(out)) = (
        unsafe { header.as_ref() },
        unsafe { record.as_ref() },
        unsafe { file.as_ref() },
        out,
    ) else {
        return false;
    };
    let mut text = Vec::new();
    let result = match what {
        0 => format::read_name(h, record, &mut file, &mut text),
        1..=3 => format::read_blob_field(h, record, what - 1, &mut file, &mut text),
        _ => format::read_path(h, record, &mut file, &mut text),
    };
    let found = result.unwrap_or_else(|OutOfMemory| {
        text.clear();
        false
    });
    // SAFETY: the caller's callback; the bytes stay valid for the call.
    unsafe { out(context, text.as_ptr().cast(), text.len()) };
    found
}

/// 1 when read, 0 when the index cannot be read, -1 out of memory.
///
/// # Safety
/// `header` and `file` are NULL or valid; `books` and `rows` hold `count` entries.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_library_recent_rows_for(
    header: *const Header,
    books: *const Identity,
    rows: *mut u16,
    count: usize,
    file: *const IndexFile,
) -> i32 {
    // SAFETY: as documented.
    let (Some(h), Some(mut file)) = (unsafe { header.as_ref() }, unsafe { file.as_ref() }) else {
        return 0;
    };
    if books.is_null() || rows.is_null() || count == 0 {
        return 1;
    }
    // SAFETY: count entries each.
    let (books, rows) = unsafe {
        (
            slice::from_raw_parts(books, count),
            slice::from_raw_parts_mut(rows, count),
        )
    };
    match format::recent_rows_for(h, books, rows, &mut file) {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(OutOfMemory) => -1,
    }
}
