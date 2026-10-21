//! XTC and XTCH books (Xteink's pre-rendered page format) as lib/Xtc's `XtcParser` read them,
//! which `test/xtc_parser` pins. The file is read through [`Source`]; nothing is allocated.
//!
//! Layout, all little-endian:
//! - Header, 56 bytes: magic `XTC\0` (1-bit pages) or `XTCH` (2-bit) at 0, version 1.0 (or 0.1)
//!   at 4, page count at 6, metadata flag at 9, chapter flag at 0x0B, page table offset at 0x18,
//!   first page data offset at 0x20, chapter table offset at 0x30. Files from before the chapter
//!   field start their page table at 0x30.
//! - Metadata: a NUL-terminated title at 0x38 (127 bytes kept) and author at 0xB8 (63 bytes).
//! - Page table: 16 bytes a page: data offset (8 bytes), data size (4), width (2), height (2).
//! - Page: a 22-byte header, magic `XTG\0` (1-bit) or `XTH\0` (2-bit) at 0, width at 4, height at
//!   6, bitmap size at 0x0A, then the bitmap. XTG rows are 8 pixels a byte; XTH holds two planes
//!   of columns, 8 vertical pixels a byte.
//! - Chapters: 96-byte records up to the page table, the page data or the end of the file: a
//!   NUL-terminated name (80 bytes), then 1-based first and last pages at 0x50 and 0x52.
//!
//! lib/Xtc/README describes the pixels and the fields this reader skips.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

const HEADER_LEN: usize = 56;
const PAGE_ENTRY_LEN: usize = 16;
const PAGE_HEADER_LEN: usize = 22;
const CHAPTER_LEN: usize = 96;
const HEADER_SIZE: u64 = HEADER_LEN as u64;
const LEGACY_HEADER_SIZE: u64 = 0x30;
const PAGE_ENTRY_SIZE: u64 = PAGE_ENTRY_LEN as u64;
const PAGE_HEADER_SIZE: u64 = PAGE_HEADER_LEN as u64;
const CHAPTER_SIZE: u64 = CHAPTER_LEN as u64;
const XTC_MAGIC: u32 = 0x0043_5458;
const XTCH_MAGIC: u32 = 0x4843_5458;
const XTG_MAGIC: u32 = 0x0047_5458;
const XTH_MAGIC: u32 = 0x0048_5458;
pub const TITLE_BYTES: usize = 127;
pub const AUTHOR_BYTES: usize = 63;
pub const CHAPTER_NAME_BYTES: usize = 80;

/// What `open` learned. `bb_xtc` in C.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Book {
    pub page_table_offset: u64,
    pub data_offset: u64,
    pub page_count: u16,
    /// The first page's size.
    pub width: u16,
    pub height: u16,
    /// 1 for XTC, 2 for XTCH.
    pub bit_depth: u8,
    /// The header says there are chapters and has room for their offset.
    pub has_chapters: bool,
    pub title_len: u8,
    pub author_len: u8,
    pub title: [u8; TITLE_BYTES],
    pub author: [u8; AUTHOR_BYTES],
}

/// Why a book or page cannot be read: `XtcError`'s numbers.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    InvalidMagic = 2,
    InvalidVersion = 3,
    CorruptedHeader = 4,
    PageOutOfRange = 5,
    ReadError = 6,
    MemoryError = 8,
}

/// A page table entry.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Page {
    pub offset: u64,
    pub size: u32,
    pub width: u16,
    pub height: u16,
}

/// The file.
pub trait Source {
    /// Reads from `offset` into the start of `buf`: how many bytes, or `None` when the file
    /// cannot be opened or seeked to `offset`.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Option<usize>;
    fn size(&mut self) -> u64;
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    let mut le = [0; 2];
    if let Some(src) = bytes.get(at..at + 2) {
        le.copy_from_slice(src);
    }
    u16::from_le_bytes(le)
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut le = [0; 4];
    if let Some(src) = bytes.get(at..at + 4) {
        le.copy_from_slice(src);
    }
    u32::from_le_bytes(le)
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    let mut le = [0; 8];
    if let Some(src) = bytes.get(at..at + 8) {
        le.copy_from_slice(src);
    }
    u64::from_le_bytes(le)
}

/// The bytes before the first NUL.
fn c_string(bytes: &[u8]) -> &[u8] {
    bytes
        .get(..bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len()))
        .unwrap_or_default()
}

/// Reads a NUL-terminated field of at most `N` bytes into `out`; its length.
fn read_text<const N: usize>(
    source: &mut impl Source,
    offset: u64,
    out: &mut [u8; N],
) -> Result<u8, Error> {
    let mut field = [0; N];
    let count = source.read_at(offset, &mut field).ok_or(Error::ReadError)?;
    // Bytes not read stay 0.
    if let Some(rest) = field.get_mut(count.min(N)..) {
        rest.fill(0);
    }
    let text = c_string(&field);
    *out = [0; N];
    if let Some(dst) = out.get_mut(..text.len()) {
        dst.copy_from_slice(text);
    }
    Ok(u8::try_from(text.len()).unwrap_or(u8::MAX))
}

/// Reads the header, the metadata and the first page's size into `book`.
///
/// # Errors
///
/// Why the book cannot be opened; the fields read before that are left in `book`.
pub fn open(source: &mut impl Source, book: &mut Book) -> Result<(), Error> {
    let mut header = [0u8; HEADER_LEN];
    let count = source.read_at(0, &mut header).unwrap_or(0);
    book.page_count = u16_at(&header, 6);
    book.page_table_offset = u64_at(&header, 0x18);
    book.data_offset = u64_at(&header, 0x20);
    if count != header.len() {
        return Err(Error::ReadError);
    }
    let magic = u32_at(&header, 0);
    if magic != XTC_MAGIC && magic != XTCH_MAGIC {
        return Err(Error::InvalidMagic);
    }
    book.bit_depth = if magic == XTCH_MAGIC { 2 } else { 1 };
    // 1.0, though some writers swap the bytes.
    if !matches!(
        (header.get(4), header.get(5)),
        (Some(1), Some(0)) | (Some(0), Some(1))
    ) {
        return Err(Error::InvalidVersion);
    }
    if book.page_count == 0 {
        return Err(Error::CorruptedHeader);
    }
    if header.get(9).is_some_and(|&flag| flag != 0) {
        book.title_len = read_text(source, 0x38, &mut book.title)?;
        book.author_len = read_text(source, 0xB8, &mut book.author)?;
    }

    let table = book.page_table_offset;
    if table == 0 {
        return Err(Error::CorruptedHeader);
    }
    let size = source.size();
    let table_size = u64::from(book.page_count) * PAGE_ENTRY_SIZE;
    if table < LEGACY_HEADER_SIZE || table > size || table_size > size - table {
        return Err(Error::CorruptedHeader);
    }
    let mut entry = [0u8; PAGE_ENTRY_LEN];
    if source.read_at(table, &mut entry) != Some(entry.len()) {
        return Err(Error::ReadError);
    }
    book.width = u16_at(&entry, 0x0C);
    book.height = u16_at(&entry, 0x0E);
    // Files from before the chapter field start their page table inside it.
    book.has_chapters = header.get(0x0B) == Some(&1) && table >= HEADER_SIZE;
    Ok(())
}

/// Page `index`'s table entry; `None` past the last page or when it cannot be read.
pub fn page(source: &mut impl Source, book: &Book, index: u32) -> Option<Page> {
    if index >= u32::from(book.page_count) {
        return None;
    }
    let mut entry = [0u8; PAGE_ENTRY_LEN];
    let at = book
        .page_table_offset
        .wrapping_add(u64::from(index) * PAGE_ENTRY_SIZE);
    if source.read_at(at, &mut entry) != Some(entry.len()) {
        return None;
    }
    Some(Page {
        offset: u64_at(&entry, 0),
        size: u32_at(&entry, 8),
        width: u16_at(&entry, 0x0C),
        height: u16_at(&entry, 0x0E),
    })
}

/// Where page `index`'s bitmap starts and its size, from the page header.
fn bitmap(
    source: &mut impl Source,
    book: &Book,
    index: u32,
    bad_header: Error,
) -> Result<(u64, u64), Error> {
    if index >= u32::from(book.page_count) {
        return Err(Error::PageOutOfRange);
    }
    let page = page(source, book, index).ok_or(Error::ReadError)?;
    let mut header = [0u8; PAGE_HEADER_LEN];
    if source.read_at(page.offset, &mut header) != Some(header.len()) {
        return Err(Error::ReadError);
    }
    let expected = if book.bit_depth == 2 {
        XTH_MAGIC
    } else {
        XTG_MAGIC
    };
    if u32_at(&header, 0) != expected {
        return Err(bad_header);
    }
    let width = u64::from(u16_at(&header, 4));
    let height = u64::from(u16_at(&header, 6));
    let size = if book.bit_depth == 2 {
        width * height.div_ceil(8) * 2
    } else {
        width.div_ceil(8) * height
    };
    if u64::from(u32_at(&header, 0x0A)) != size {
        return Err(Error::CorruptedHeader);
    }
    Ok((page.offset.wrapping_add(PAGE_HEADER_SIZE), size))
}

/// Reads page `index`'s bitmap into the start of `buf`; its size.
///
/// # Errors
///
/// Why the page cannot be read; [`Error::MemoryError`] when `buf` is too small.
pub fn load_page(
    source: &mut impl Source,
    book: &Book,
    index: u32,
    buf: &mut [u8],
) -> Result<usize, Error> {
    let (start, size) = bitmap(source, book, index, Error::InvalidMagic)?;
    let size = usize::try_from(size).map_err(|_| Error::MemoryError)?;
    let out = buf.get_mut(..size).ok_or(Error::MemoryError)?;
    if source.read_at(start, out) != Some(size) {
        return Err(Error::ReadError);
    }
    Ok(size)
}

/// Reads page `index`'s bitmap through `chunk`, handing each piece and its offset to `f`.
///
/// # Errors
///
/// Why the page cannot be read; a wrong page magic is [`Error::ReadError`] here.
pub fn stream_page(
    source: &mut impl Source,
    book: &Book,
    index: u32,
    chunk: &mut [u8],
    mut f: impl FnMut(&[u8], u64),
) -> Result<(), Error> {
    let (start, size) = bitmap(source, book, index, Error::ReadError)?;
    let mut done = 0u64;
    while done < size {
        let want = usize::try_from(size - done).map_or(chunk.len(), |left| left.min(chunk.len()));
        let piece = chunk.get_mut(..want).unwrap_or_default();
        let count = source
            .read_at(start.wrapping_add(done), piece)
            .unwrap_or(0)
            .min(want);
        if count == 0 {
            return Err(Error::ReadError);
        }
        f(piece.get(..count).unwrap_or_default(), done);
        done += count as u64;
    }
    Ok(())
}

/// Tells `records` how many 96-byte records the chapter table has room for, then hands each
/// chapter (name, first and last page, 0-based and within the book) to `f`. `Ok(false)` when there
/// is no chapter table to read, `Ok(true)` when it was read.
///
/// # Errors
///
/// [`Error::ReadError`] when the file cannot be read; chapters already handed over stand.
pub fn chapters(
    source: &mut impl Source,
    book: &Book,
    records: impl FnOnce(u64),
    mut f: impl FnMut(&[u8], u16, u16),
) -> Result<bool, Error> {
    let mut flag = [0u8];
    if source.read_at(0x0B, &mut flag) != Some(1) {
        return Err(Error::ReadError);
    }
    if flag != [1] {
        return Ok(false);
    }
    // Eight bytes: the chapter offset and the padding after it.
    let mut offset = [0u8; 8];
    if source.read_at(0x30, &mut offset) != Some(8) {
        return Err(Error::ReadError);
    }
    let offset = u64::from_le_bytes(offset);
    if offset == 0 {
        return Ok(false);
    }
    let size = source.size();
    if offset < HEADER_SIZE || offset >= size || offset + CHAPTER_SIZE > size {
        return Ok(false);
    }
    let end = if book.page_table_offset > offset && book.page_table_offset <= size {
        book.page_table_offset
    } else if book.data_offset > offset && book.data_offset <= size {
        book.data_offset
    } else {
        size
    };
    if end <= offset {
        return Ok(false);
    }
    let count = (end - offset) / CHAPTER_SIZE;
    if count == 0 {
        return Ok(false);
    }
    records(count);
    let last_page = book.page_count.saturating_sub(1);
    for i in 0..count {
        let mut record = [0u8; CHAPTER_LEN];
        if source.read_at(offset + i * CHAPTER_SIZE, &mut record) != Some(record.len()) {
            return Err(Error::ReadError);
        }
        let name = c_string(record.get(..CHAPTER_NAME_BYTES).unwrap_or_default());
        let start = u16_at(&record, 0x50);
        let last = u16_at(&record, 0x52);
        if name.is_empty() && start == 0 && last == 0 {
            break;
        }
        let start = start.saturating_sub(1);
        if start >= book.page_count {
            continue;
        }
        let last = last.saturating_sub(1).min(last_page);
        if start > last {
            continue;
        }
        f(name, start, last);
    }
    Ok(true)
}

/// Whether `bytes` starts with an XTC or XTCH magic.
#[must_use]
pub fn is_book_magic(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && matches!(u32_at(bytes, 0), XTC_MAGIC | XTCH_MAGIC)
}

#[cfg(test)]
mod tests;
