//! The Library index file (`CLX1`) as lib/LibraryIndex's `LibraryIndexFile` reads it, which
//! `test/library_format` and `test/library_index_file` pin. lib/LibraryIndex/LibraryFormat.h
//! describes the layout; the C++ index builder writes it with the same [`layout_sections`].
//!
//! Every section starts on a 512-byte boundary: the 64-byte header, the folder records, the
//! 128-byte book records in title order, the author and arrival permutations (u16 each), then the
//! name blobs. Section arithmetic is 32-bit and wraps, as the format's writer computes it; a header
//! is only trusted after [`validate`] (or [`validate_structure`] when reconciling).

use alloc::vec::Vec;

use crate::OutOfMemory;

pub const ALIGN: u32 = 512;
pub const FORMAT_VERSION: u8 = 2;
pub const FOLD_VERSION: u8 = 4;
pub const MAX_RECORDS: u16 = 4096;
pub const FOLD_BYTES: usize = 96;
pub const AUTHOR_KEY_BYTES: usize = 12;
const RECORD_BYTES: u32 = 128;
// `ClixRecord`'s fields, by offset (the reader and the build module's writer share them).
pub(crate) const RECORD_NAME_OFF: usize = 0;
pub(crate) const RECORD_FILE_SIZE: usize = 4;
pub(crate) const RECORD_FIRST_SEEN: usize = 8;
pub(crate) const RECORD_FOLDER_ID: usize = 10;
pub(crate) const RECORD_NAME_LEN: usize = 12;
pub(crate) const RECORD_FOLD_LEN: usize = 13;
pub(crate) const RECORD_AUTHOR_KEY_LEN: usize = 14;
pub(crate) const RECORD_METADATA_STATUS: usize = 15;
pub(crate) const RECORD_FOLD: usize = 16;
pub(crate) const RECORD_AUTHOR_KEY: usize = 112;
pub(crate) const RECORD_MODIFICATION_TIME: usize = 124;
const MAGIC: [u8; 4] = *b"CLX1";
const NONE: u16 = 0xFFFF;
const METADATA_FAILED: u8 = 2;

/// `ClixHeader`.
#[repr(C, packed)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Header {
    pub magic: [u8; 4],
    pub format_version: u8,
    pub fold_version: u8,
    pub flags: u8,
    pub metadata_enabled: u8,
    pub book_count: u16,
    pub folder_count: u16,
    pub next_first_seen: u16,
    pub padding1: u16,
    pub folder_start: u32,
    pub folder_len: u32,
    pub record_start: u32,
    pub perm_start: u32,
    pub name_start: u32,
    pub name_len: u32,
    pub self_size: u32,
    pub reserved: [u8; 20],
}

/// `ClixRecord`.
#[repr(C, packed)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Record {
    pub name_off: u32,
    pub file_size: u32,
    pub first_seen: u16,
    pub folder_id: u16,
    pub name_len: u8,
    pub fold_len: u8,
    pub author_key_len: u8,
    pub metadata_status: u8,
    pub fold: [u8; FOLD_BYTES],
    pub author_key: [u8; AUTHOR_KEY_BYTES],
    pub modification_time: u32,
}

const _: () = {
    use core::mem::offset_of;
    assert!(size_of::<Header>() == 64 && size_of::<Record>() == 128);
    assert!(
        offset_of!(Record, name_off) == RECORD_NAME_OFF
            && offset_of!(Record, file_size) == RECORD_FILE_SIZE
            && offset_of!(Record, first_seen) == RECORD_FIRST_SEEN
            && offset_of!(Record, folder_id) == RECORD_FOLDER_ID
            && offset_of!(Record, name_len) == RECORD_NAME_LEN
            && offset_of!(Record, fold_len) == RECORD_FOLD_LEN
            && offset_of!(Record, author_key_len) == RECORD_AUTHOR_KEY_LEN
            && offset_of!(Record, metadata_status) == RECORD_METADATA_STATUS
            && offset_of!(Record, fold) == RECORD_FOLD
            && offset_of!(Record, author_key) == RECORD_AUTHOR_KEY
            && offset_of!(Record, modification_time) == RECORD_MODIFICATION_TIME
    );
};

/// `ClixValidity`.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Validity {
    Ok = 0,
    BadMagic = 1,
    UnknownFormatVersion = 2,
    StaleFoldVersion = 3,
    SizeMismatch = 4,
    CountOutOfRange = 5,
    SectionsInconsistent = 6,
}

/// The index file.
pub trait Source {
    /// Fills `buf` from `offset`; false when it cannot (and the read counts as failed).
    fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool;
    /// Marks the index as failed without a read: a record points outside its section.
    fn fail(&mut self);
}

#[must_use]
pub fn align_up(value: u32) -> u32 {
    value.wrapping_add(ALIGN - 1) / ALIGN * ALIGN
}

/// Fills in every offset and the expected file size from the counts.
pub fn layout_sections(h: &mut Header, folder_bytes: u32, name_bytes: u32) {
    let books = u32::from(h.book_count);
    h.folder_start = ALIGN;
    h.folder_len = folder_bytes;
    h.record_start = align_up(ALIGN.wrapping_add(folder_bytes));
    h.perm_start = align_up(
        h.record_start
            .wrapping_add(books.wrapping_mul(RECORD_BYTES)),
    );
    h.name_start = align_up(h.perm_start.wrapping_add(books.wrapping_mul(4)));
    h.name_len = name_bytes;
    h.self_size = h.name_start.wrapping_add(name_bytes);
}

#[must_use]
pub fn record_offset(h: &Header, ordinal: u16) -> u32 {
    h.record_start
        .wrapping_add(u32::from(ordinal) * RECORD_BYTES)
}

#[must_use]
pub fn author_order_offset(h: &Header, k: u16) -> u32 {
    h.perm_start.wrapping_add(u32::from(k) * 2)
}

#[must_use]
pub fn arrival_order_offset(h: &Header, k: u16) -> u32 {
    h.perm_start
        .wrapping_add((u32::from(h.book_count) + u32::from(k)).wrapping_mul(2))
}

/// The header's structure against the file's real size, the fold version aside.
#[must_use]
pub fn validate_structure(h: &Header, file_size: u64) -> Validity {
    let header = *h;
    if header.magic != MAGIC {
        return Validity::BadMagic;
    }
    if header.format_version != FORMAT_VERSION {
        return Validity::UnknownFormatVersion;
    }
    if header.book_count > MAX_RECORDS {
        return Validity::CountOutOfRange;
    }
    if header.metadata_enabled > 1 {
        return Validity::SectionsInconsistent;
    }
    if file_size != u64::from(header.self_size) {
        return Validity::SizeMismatch;
    }
    // Capped against the real size, the lengths cannot wrap the section sums.
    if u64::from(header.folder_len) > file_size || u64::from(header.name_len) > file_size {
        return Validity::SectionsInconsistent;
    }
    let mut expected = header;
    layout_sections(&mut expected, header.folder_len, header.name_len);
    let (e, h) = (expected, header);
    if e.folder_start != h.folder_start
        || e.record_start != h.record_start
        || e.perm_start != h.perm_start
        || e.name_start != h.name_start
        || e.self_size != h.self_size
    {
        return Validity::SectionsInconsistent;
    }
    Validity::Ok
}

#[must_use]
pub fn validate(h: &Header, file_size: u64) -> Validity {
    match validate_structure(h, file_size) {
        Validity::Ok if h.fold_version == FOLD_VERSION => Validity::Ok,
        Validity::Ok => Validity::StaleFoldVersion,
        other => other,
    }
}

/// FNV-1a 64 of a complete path: a book's identity in the index.
#[must_use]
pub fn path_hash(path: &[u8]) -> u64 {
    path.iter().fold(14_695_981_039_346_656_037, |hash, &c| {
        (hash ^ u64::from(c)).wrapping_mul(1_099_511_628_211)
    })
}

/// Sort orders, `SortOrder`'s numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Order {
    RecentAsc,
    RecentDesc,
    TitleAsc,
    TitleDesc,
    AuthorAsc,
    AuthorDesc,
}

impl Order {
    #[must_use]
    pub fn from_u8(order: u8) -> Option<Order> {
        Some(match order {
            0 => Order::RecentAsc,
            1 => Order::RecentDesc,
            2 => Order::TitleAsc,
            3 => Order::TitleDesc,
            4 => Order::AuthorAsc,
            5 => Order::AuthorDesc,
            _ => return None,
        })
    }
}

fn read_u16(src: &mut impl Source, offset: u32) -> Option<u16> {
    let mut b = [0; 2];
    src.read_at(offset, &mut b).then(|| u16::from_le_bytes(b))
}

/// The record ordinal at display row `row`; 0xFFFF past the end or when unreadable.
pub fn ordinal_for_row(h: &Header, order: Order, row: u16, src: &mut impl Source) -> u16 {
    let count = h.book_count;
    if row >= count {
        return NONE;
    }
    let from_perm = |src: &mut _, offset| match read_u16(src, offset) {
        Some(ordinal) if ordinal < count => ordinal,
        _ => NONE,
    };
    match order {
        // The records are in title order.
        Order::TitleAsc => row,
        Order::TitleDesc => count - 1 - row,
        Order::AuthorAsc => from_perm(src, author_order_offset(h, row)),
        Order::AuthorDesc => from_perm(src, author_order_offset(h, count - 1 - row)),
        // The arrival permutation runs oldest first.
        Order::RecentAsc => from_perm(src, arrival_order_offset(h, row)),
        Order::RecentDesc => from_perm(src, arrival_order_offset(h, count - 1 - row)),
    }
}

impl Record {
    pub(crate) fn from_bytes(b: &[u8; 128]) -> Record {
        let u32_at = |i: usize| {
            u32::from_le_bytes(core::array::from_fn(|k| b.get(i + k).copied().unwrap_or(0)))
        };
        let u16_at = |i: usize| {
            u16::from_le_bytes(core::array::from_fn(|k| b.get(i + k).copied().unwrap_or(0)))
        };
        let byte = |i: usize| b.get(i).copied().unwrap_or(0);
        Record {
            name_off: u32_at(RECORD_NAME_OFF),
            file_size: u32_at(RECORD_FILE_SIZE),
            first_seen: u16_at(RECORD_FIRST_SEEN),
            folder_id: u16_at(RECORD_FOLDER_ID),
            name_len: byte(RECORD_NAME_LEN),
            fold_len: byte(RECORD_FOLD_LEN),
            author_key_len: byte(RECORD_AUTHOR_KEY_LEN),
            metadata_status: byte(RECORD_METADATA_STATUS),
            fold: core::array::from_fn(|k| byte(RECORD_FOLD + k)),
            author_key: core::array::from_fn(|k| byte(RECORD_AUTHOR_KEY + k)),
            modification_time: u32_at(RECORD_MODIFICATION_TIME),
        }
    }
}

/// Reads record `ordinal` into `out`, clamped: lengths to their fields, a name offset past the
/// name section to an empty name. False past the end, when unreadable, or for an unknown metadata
/// status (`out` then holds the record with clamped lengths).
pub fn read_record(h: &Header, ordinal: u16, src: &mut impl Source, out: &mut Record) -> bool {
    if ordinal >= h.book_count {
        return false;
    }
    let mut b = [0u8; 128];
    if !src.read_at(record_offset(h, ordinal), &mut b) {
        return false;
    }
    let mut r = Record::from_bytes(&b);
    // The lengths come off a card the user can write to and that can rot.
    #[allow(clippy::cast_possible_truncation)]
    {
        r.fold_len = r.fold_len.min(FOLD_BYTES as u8);
        r.author_key_len = r.author_key_len.min(AUTHOR_KEY_BYTES as u8);
    }
    *out = r;
    if out.metadata_status > METADATA_FAILED {
        return false;
    }
    // A forged offset near the top would wrap the sums readers check against.
    if out.name_off > h.name_len {
        out.name_len = 0;
        out.name_off = 0;
    }
    true
}

/// The record's complete-path hash; `None` when it points outside the names (the index is then
/// failed) or is unreadable.
pub fn read_path_hash(h: &Header, record: &Record, src: &mut impl Source) -> Option<u64> {
    let off = record.name_off;
    if off > h.name_len || 8 > h.name_len - off {
        src.fail();
        return None;
    }
    let mut b = [0; 8];
    src.read_at(h.name_start.wrapping_add(off), &mut b)
        .then(|| u64::from_le_bytes(b))
}

/// The name blob's bounds for a record with a name.
fn name_in_bounds(h: &Header, record: &Record) -> bool {
    let (off, len, names) = (record.name_off, u32::from(record.name_len), h.name_len);
    record.name_len != 0 && off <= names && 8 <= names - off && len <= names - off - 8
}

fn read_bytes(
    src: &mut impl Source,
    offset: u32,
    len: usize,
    out: &mut Vec<u8>,
) -> Result<bool, OutOfMemory> {
    out.clear();
    out.try_reserve(len).map_err(|_| OutOfMemory)?;
    out.resize(len, 0);
    Ok(src.read_at(offset, out))
}

/// The display basename, into `out`. False for no name or when unreadable.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn read_name(
    h: &Header,
    record: &Record,
    src: &mut impl Source,
    out: &mut Vec<u8>,
) -> Result<bool, OutOfMemory> {
    out.clear();
    if !name_in_bounds(h, record) {
        return Ok(false);
    }
    let at = h.name_start.wrapping_add(record.name_off).wrapping_add(8);
    read_bytes(src, at, usize::from(record.name_len), out)
}

/// Length-prefixed field `field` after the name (0 author, 1 title, 2 source author) into `out`.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn read_blob_field(
    h: &Header,
    record: &Record,
    field: u8,
    src: &mut impl Source,
    out: &mut Vec<u8>,
) -> Result<bool, OutOfMemory> {
    out.clear();
    if !name_in_bounds(h, record) {
        return Ok(false);
    }
    let names = h.name_len;
    let mut at = record.name_off + 8 + u32::from(record.name_len);
    for i in 0..=field {
        if at >= names {
            return Ok(false);
        }
        let mut len = [0u8];
        if !src.read_at(h.name_start.wrapping_add(at), &mut len) {
            return Ok(false);
        }
        at += 1;
        let len = u32::from(len[0]);
        if len > names - at {
            return Ok(false);
        }
        if i == field {
            return if len == 0 {
                Ok(true)
            } else {
                read_bytes(src, h.name_start.wrapping_add(at), len as usize, out)
            };
        }
        at += len;
    }
    Ok(false)
}

/// The book's absolute path, from its folder record and name, into `out`.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn read_path(
    h: &Header,
    record: &Record,
    src: &mut impl Source,
    out: &mut Vec<u8>,
) -> Result<bool, OutOfMemory> {
    out.clear();
    if record.folder_id >= h.folder_count {
        return Ok(false);
    }
    // Folder records are variable length: reaching folder n walks the n before it.
    let mut offset = h.folder_start;
    let end = h.folder_start.wrapping_add(h.folder_len);
    for i in 0..=record.folder_id {
        if offset >= end {
            return Ok(false);
        }
        let mut len = [0u8];
        if !src.read_at(offset, &mut len) || len[0] == 0 {
            return Ok(false);
        }
        let len = u32::from(len[0]);
        if len > end - offset - 1 {
            return Ok(false);
        }
        if i == record.folder_id {
            let mut dir = Vec::new();
            if !read_bytes(src, offset + 1, len as usize, &mut dir)? {
                return Ok(false);
            }
            let mut name = Vec::new();
            if !read_name(h, record, src, &mut name)? {
                return Ok(false);
            }
            // LibraryText's joinLibraryPath, which the builder hashes paths with: keep them alike.
            out.try_reserve(dir.len() + name.len() + 1)
                .map_err(|_| OutOfMemory)?;
            out.extend_from_slice(&dir);
            if dir.last() != Some(&b'/') {
                out.push(b'/');
            }
            out.extend_from_slice(&name);
            return Ok(true);
        }
        offset += 1 + len;
        if offset >= end {
            return Ok(false);
        }
    }
    Ok(false)
}

/// One book to find: its path hash, and its size when known (0: check every hash). `BookIdentity`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Identity {
    pub path_hash: u64,
    pub file_size: u32,
}

/// Lookups per call: with more, none is looked up and every row is 0xFFFF.
pub const MAX_LOOKUPS: usize = 16;

/// The `RecentAsc` row of each book (0xFFFF when absent) into `rows`: one pass over the records,
/// then one over the arrival permutation. `Ok(false)` when the index cannot be read.
///
/// # Errors
///
/// When the 4 KB read buffer cannot be had.
pub fn recent_rows_for(
    h: &Header,
    books: &[Identity],
    rows: &mut [u16],
    src: &mut impl Source,
) -> Result<bool, OutOfMemory> {
    const CHUNK: u16 = 32;
    for row in rows.iter_mut().take(books.len()) {
        *row = NONE;
    }
    let count = books.len().min(rows.len());
    if count == 0 || count > MAX_LOOKUPS || h.book_count == 0 {
        return Ok(true);
    }
    let mut chunk: Vec<u8> = Vec::new();
    chunk.try_reserve(4096).map_err(|_| OutOfMemory)?;
    chunk.resize(4096, 0);

    let mut ordinals = [NONE; MAX_LOOKUPS];
    let mut unresolved = count;
    let mut base = 0u16;
    while base < h.book_count && unresolved > 0 {
        let batch = CHUNK.min(h.book_count - base);
        let bytes = usize::from(batch) * 128;
        if !src.read_at(
            record_offset(h, base),
            chunk.get_mut(..bytes).unwrap_or_default(),
        ) {
            return Ok(false);
        }
        let mut r = 0;
        while r < batch && unresolved > 0 {
            let at = usize::from(r) * 128;
            let mut raw = [0u8; 128];
            if let Some(src_bytes) = chunk.get(at..at + 128) {
                raw.copy_from_slice(src_bytes);
            }
            let record = Record::from_bytes(&raw);
            let mut hash = None;
            for (i, book) in books.iter().take(count).enumerate() {
                if ordinals.get(i) != Some(&NONE) {
                    continue;
                }
                // Size 0: the caller could not stat the file, so the hash alone decides.
                if book.file_size != 0 && book.file_size != record.file_size {
                    continue;
                }
                if hash.is_none() {
                    // A record pointing outside the names is skipped.
                    if record.name_off > h.name_len {
                        break;
                    }
                    let Some(read) = read_path_hash(h, &record, src) else {
                        return Ok(false);
                    };
                    hash = Some(read);
                }
                if hash == Some(book.path_hash)
                    && let Some(slot) = ordinals.get_mut(i)
                {
                    *slot = base + r;
                    unresolved -= 1;
                }
            }
            r += 1;
        }
        base = base.saturating_add(CHUNK);
    }

    let mut base = 0u16;
    while base < h.book_count && unresolved < count {
        let batch = (CHUNK * 2).min(h.book_count - base);
        let bytes = usize::from(batch) * 2;
        if !src.read_at(
            arrival_order_offset(h, base),
            chunk.get_mut(..bytes).unwrap_or_default(),
        ) {
            return Ok(false);
        }
        for k in 0..batch {
            let at = usize::from(k) * 2;
            let ordinal = u16::from_le_bytes([
                chunk.get(at).copied().unwrap_or(0),
                chunk.get(at + 1).copied().unwrap_or(0),
            ]);
            for (i, &found) in ordinals.iter().take(count).enumerate() {
                if found != NONE
                    && found == ordinal
                    && let Some(row) = rows.get_mut(i)
                {
                    *row = base + k;
                }
            }
        }
        base = base.saturating_add(CHUNK * 2);
    }
    Ok(true)
}
