//! Exports for `lib/ZipFile` (see `rust/include/bembeyaz/zip.h`).

use bembeyaz_zip::{Cursor, Directory, Entry, Error, Source};
use core::ffi::{c_char, c_int, c_void};
use core::ops::ControlFlow;

pub const BB_ZIP_OK: c_int = 0;
pub const BB_ZIP_NOT_FOUND: c_int = 1;

/// Errors map to -1, -2, ... in the order of [`Error::ALL`].
fn status(error: Error) -> c_int {
    let index = Error::ALL.iter().position(|&e| e == error).unwrap_or(0);
    -c_int::try_from(index + 1).unwrap_or(c_int::MAX)
}

#[repr(C)]
pub struct BbZipSource {
    context: *mut c_void,
    size: u64,
    read_at: unsafe extern "C" fn(
        context: *mut c_void,
        offset: u64,
        buffer: *mut u8,
        len: usize,
    ) -> bool,
}

impl Source for &BbZipSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error> {
        if unsafe { (self.read_at)(self.context, offset, buf.as_mut_ptr(), buf.len()) } {
            Ok(())
        } else {
            Err(Error::Io)
        }
    }
}

#[repr(C)]
pub struct BbZipDirectory {
    offset: u64,
    size: u64,
    entries: u64,
}

#[repr(C)]
pub struct BbZipCursor {
    offset: u64,
    index: u64,
}

#[repr(C)]
pub struct BbZipEntry {
    method: u16,
    flags: u16,
    crc32: u32,
    compressed_size: u64,
    uncompressed_size: u64,
    local_header_offset: u64,
}

impl From<&Entry> for BbZipEntry {
    fn from(e: &Entry) -> Self {
        Self {
            method: e.method,
            flags: e.flags,
            crc32: e.crc32,
            compressed_size: e.compressed_size,
            uncompressed_size: e.uncompressed_size,
            local_header_offset: e.local_header_offset,
        }
    }
}

impl From<&BbZipEntry> for Entry {
    fn from(e: &BbZipEntry) -> Self {
        Self {
            method: e.method,
            flags: e.flags,
            crc32: e.crc32,
            compressed_size: e.compressed_size,
            uncompressed_size: e.uncompressed_size,
            local_header_offset: e.local_header_offset,
        }
    }
}

fn directory(d: &BbZipDirectory) -> Directory {
    Directory {
        offset: d.offset,
        size: d.size,
        entries: d.entries,
    }
}

/// Finds the central directory of the archive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_zip_locate(
    source: *const BbZipSource,
    out: *mut BbZipDirectory,
) -> c_int {
    let mut src = unsafe { &*source };
    match Directory::locate(&mut src) {
        Ok(d) => {
            unsafe {
                *out = BbZipDirectory {
                    offset: d.offset,
                    size: d.size,
                    entries: d.entries,
                };
            }
            BB_ZIP_OK
        }
        Err(e) => status(e),
    }
}

/// Finds an entry by name, resuming at `cursor` (zero it to start over).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_zip_find(
    source: *const BbZipSource,
    dir: *const BbZipDirectory,
    name: *const c_char,
    name_len: usize,
    cursor: *mut BbZipCursor,
    out: *mut BbZipEntry,
) -> c_int {
    unsafe {
        let mut src = &*source;
        let name = if name_len == 0 {
            &[]
        } else {
            core::slice::from_raw_parts(name.cast(), name_len)
        };
        let mut position = Cursor::from_parts((*cursor).offset, (*cursor).index);
        match directory(&*dir).find(&mut src, name, &mut position) {
            Ok(Some(entry)) => {
                let (offset, index) = position.into_parts();
                *cursor = BbZipCursor { offset, index };
                *out = BbZipEntry::from(&entry);
                BB_ZIP_OK
            }
            Ok(None) => BB_ZIP_NOT_FOUND,
            Err(e) => status(e),
        }
    }
}

/// Calls `visit` for each entry in directory order until it returns false.
/// `name` is valid only during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_zip_for_each(
    source: *const BbZipSource,
    dir: *const BbZipDirectory,
    visit: unsafe extern "C" fn(
        name: *const c_char,
        name_len: usize,
        entry: *const BbZipEntry,
        context: *mut c_void,
    ) -> bool,
    context: *mut c_void,
) -> c_int {
    unsafe {
        let mut src = &*source;
        let result = directory(&*dir).for_each(&mut src, |name, entry| {
            let entry = BbZipEntry::from(entry);
            if visit(name.as_ptr().cast(), name.len(), &entry, context) {
                ControlFlow::Continue(())
            } else {
                ControlFlow::Break(())
            }
        });
        result.map_or_else(status, |()| BB_ZIP_OK)
    }
}

/// Resolves the offset of an entry's data, checking it lies inside the archive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_zip_data_offset(
    source: *const BbZipSource,
    dir: *const BbZipDirectory,
    entry: *const BbZipEntry,
    out: *mut u64,
) -> c_int {
    unsafe {
        let mut src = &*source;
        match directory(&*dir).data_range(&mut src, &Entry::from(&*entry)) {
            Ok(range) => {
                *out = range.start;
                BB_ZIP_OK
            }
            Err(e) => status(e),
        }
    }
}

/// Describes a status code, for logs.
#[unsafe(no_mangle)]
pub extern "C" fn bb_zip_status_str(code: c_int) -> *const c_char {
    let error = usize::try_from(-i64::from(code) - 1)
        .ok()
        .and_then(|i| Error::ALL.get(i));
    let text = match (code, error) {
        (BB_ZIP_OK, _) => c"ok",
        (BB_ZIP_NOT_FOUND, _) => c"not found",
        (_, Some(error)) => error.message(),
        (_, None) => c"unknown",
    };
    text.as_ptr()
}
