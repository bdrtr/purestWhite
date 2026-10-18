//! ZIP container reader for EPUB files.
//!
//! Locates the central directory (including ZIP64 archives and archives with
//! long comments), walks and searches its entries, and resolves where an
//! entry's data lives. Decompression is left to the caller.
//!
//! Every field comes from an untrusted file, so offsets are checked before
//! use, nothing is allocated from sizes read from the file, and a malformed
//! archive yields an [`Error`] instead of garbage entries.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use core::ffi::CStr;
use core::fmt;
use core::ops::{ControlFlow, Range};

/// Random-access byte source, such as an open file.
pub trait Source {
    /// Total size in bytes.
    fn size(&self) -> u64;
    /// Fills `buf` from `offset`. A short read is an error.
    ///
    /// # Errors
    /// [`Error::Io`] when the bytes cannot be read.
    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error>;
}

impl Source for &[u8] {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn read_exact_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), Error> {
        let start = usize::try_from(offset).map_err(|_| Error::Io)?;
        let end = start.checked_add(buf.len()).ok_or(Error::Io)?;
        buf.copy_from_slice(self.get(start..end).ok_or(Error::Io)?);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The source could not be read.
    Io,
    /// No end of central directory record: not a ZIP file.
    NoEndRecord,
    /// An end record exists but points outside the file.
    BadEndRecord,
    /// A central directory entry is malformed or runs past the directory.
    BadCentralEntry,
    /// The local header of an entry is missing or malformed.
    BadLocalHeader,
    /// An entry's data runs past the start of the central directory.
    DataOutOfBounds,
    /// A stored entry whose compressed and uncompressed sizes differ.
    StoredSizeMismatch,
    /// The entry is encrypted with ZIP encryption.
    Encrypted,
}

impl Error {
    /// Every error, in the order of their C status codes.
    pub const ALL: [Error; 8] = [
        Error::Io,
        Error::NoEndRecord,
        Error::BadEndRecord,
        Error::BadCentralEntry,
        Error::BadLocalHeader,
        Error::DataOutOfBounds,
        Error::StoredSizeMismatch,
        Error::Encrypted,
    ];

    /// A short description, NUL-terminated so C code can log it too.
    #[must_use]
    pub fn message(self) -> &'static CStr {
        match self {
            Error::Io => c"read failed",
            Error::NoEndRecord => c"not a zip file (no end of central directory)",
            Error::BadEndRecord => c"end of central directory points outside the file",
            Error::BadCentralEntry => c"malformed central directory entry",
            Error::BadLocalHeader => c"malformed local file header",
            Error::DataOutOfBounds => c"entry data runs past the central directory",
            Error::StoredSizeMismatch => c"stored entry sizes differ",
            Error::Encrypted => c"entry is encrypted",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message().to_str().unwrap_or_default())
    }
}

impl core::error::Error for Error {}

pub const METHOD_STORED: u16 = 0;
pub const METHOD_DEFLATED: u16 = 8;
/// Longest entry name [`Directory::for_each`] reports; longer ones are skipped.
/// [`Directory::find`] has no limit.
pub const MAX_NAME_LEN: usize = 512;

const EOCD_SIG: u32 = 0x0605_4b50;
const EOCD_LEN: u64 = 22;
const MAX_COMMENT_LEN: u64 = 0xFFFF;
const ZIP64_LOCATOR_SIG: u32 = 0x0706_4b50;
const ZIP64_LOCATOR_LEN: u64 = 20;
const ZIP64_EOCD_SIG: u32 = 0x0606_4b50;
const ZIP64_EOCD_LEN: u64 = 56;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const CENTRAL_LEN: u64 = 46;
const LOCAL_SIG: u32 = 0x0403_4b50;
const LOCAL_LEN: u64 = 30;
const ZIP64_EXTRA_ID: u16 = 0x0001;
const FLAG_ENCRYPTED: u16 = 1;

/// Little-endian fields of a fixed-size record. Out-of-range reads give 0;
/// callers only use offsets inside the record they read.
struct Record<'a>(&'a [u8]);

impl Record<'_> {
    fn bytes<const N: usize>(&self, at: usize) -> [u8; N] {
        at.checked_add(N)
            .and_then(|end| self.0.get(at..end))
            .and_then(|s| s.try_into().ok())
            .unwrap_or([0; N])
    }
    fn u16(&self, at: usize) -> u16 {
        u16::from_le_bytes(self.bytes(at))
    }
    fn u32(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.bytes(at))
    }
    fn u64(&self, at: usize) -> u64 {
        u64::from_le_bytes(self.bytes(at))
    }
}

fn read_record<S: Source, const N: usize>(src: &mut S, offset: u64) -> Result<[u8; N], Error> {
    let mut buf = [0; N];
    src.read_exact_at(offset, &mut buf)?;
    Ok(buf)
}

enum Zip64 {
    Absent,
    Invalid,
    Found(Directory, u64),
}

/// The central directory, as described by the end record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Directory {
    pub offset: u64,
    pub size: u64,
    pub entries: u64,
}

/// A central directory entry, without its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Entry {
    pub method: u16,
    pub flags: u16,
    pub crc32: u32,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub local_header_offset: u64,
}

/// A central directory entry and where its name and successor are.
struct Header {
    entry: Entry,
    name_len: u16,
    name_offset: u64,
    next_offset: u64,
}

/// Where a search resumes. Consecutive lookups in directory order, as an EPUB
/// load does them, each read only the entries between two matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cursor {
    offset: u64,
    index: u64,
}

impl Cursor {
    /// Raw parts, so a C caller can keep the cursor between calls.
    #[must_use]
    pub fn into_parts(self) -> (u64, u64) {
        (self.offset, self.index)
    }
    #[must_use]
    pub fn from_parts(offset: u64, index: u64) -> Self {
        Self { offset, index }
    }
}

impl Directory {
    /// Finds the end of central directory record and the directory it
    /// describes.
    ///
    /// Searches backwards from the end of the file, past a comment of up to
    /// 64 KiB. A signature is only accepted when the record it starts fits the
    /// file and points to a directory before itself, so a signature inside a
    /// comment is skipped.
    ///
    /// # Errors
    /// [`Error::NoEndRecord`] when there is no signature, [`Error::BadEndRecord`]
    /// when every candidate is inconsistent, [`Error::Io`] on a read failure.
    pub fn locate<S: Source>(src: &mut S) -> Result<Self, Error> {
        let len = src.size();
        if len < EOCD_LEN {
            return Err(Error::NoEndRecord);
        }
        let earliest = len.saturating_sub(EOCD_LEN + MAX_COMMENT_LEN);
        let signature = EOCD_SIG.to_le_bytes();
        let mut buf = [0u8; 512];
        let mut found_signature = false;
        // Scan windows from the end; consecutive windows overlap by three bytes
        // so a signature split between them is still seen.
        let mut hi = len;
        loop {
            let lo = hi.saturating_sub(buf.len() as u64).max(earliest);
            let window = buf
                .get_mut(..usize::try_from(hi - lo).map_err(|_| Error::Io)?)
                .ok_or(Error::Io)?;
            src.read_exact_at(lo, window)?;
            let positions = window.windows(4).enumerate().rev();
            for (i, _) in positions.filter(|(_, w)| *w == signature) {
                let at = lo + i as u64;
                if at + EOCD_LEN > len {
                    continue;
                }
                found_signature = true;
                if let Some(directory) = Self::from_end_record(src, at, len) {
                    return Ok(directory);
                }
            }
            if lo == earliest {
                break;
            }
            hi = lo + 3;
        }
        Err(if found_signature {
            Error::BadEndRecord
        } else {
            Error::NoEndRecord
        })
    }

    fn from_end_record<S: Source>(src: &mut S, at: u64, len: u64) -> Option<Self> {
        let raw: [u8; 22] = read_record(src, at).ok()?;
        let end = Record(&raw);
        let comment_len = u64::from(end.u16(20));
        if at + EOCD_LEN + comment_len > len {
            return None;
        }
        let mut directory = Self {
            offset: u64::from(end.u32(16)),
            size: u64::from(end.u32(12)),
            entries: u64::from(end.u16(10)),
        };
        let mut limit = at;
        match Self::zip64_end_record(src, at) {
            Zip64::Found(zip64, record_at) => (directory, limit) = (zip64, record_at),
            Zip64::Invalid => return None,
            // ZIP64 sentinels without a ZIP64 record.
            Zip64::Absent
                if end.u16(10) == u16::MAX
                    || end.u32(12) == u32::MAX
                    || end.u32(16) == u32::MAX =>
            {
                return None;
            }
            Zip64::Absent => {}
        }
        let directory_end = directory.offset.checked_add(directory.size)?;
        (directory_end <= limit).then_some(directory)
    }

    /// The ZIP64 directory and the offset of its record, when a ZIP64 locator
    /// precedes the end record at `end_at`.
    fn zip64_end_record<S: Source>(src: &mut S, end_at: u64) -> Zip64 {
        let Some(locator_at) = end_at.checked_sub(ZIP64_LOCATOR_LEN) else {
            return Zip64::Absent;
        };
        let Ok(raw) = read_record::<S, 20>(src, locator_at) else {
            return Zip64::Absent;
        };
        let locator = Record(&raw);
        if locator.u32(0) != ZIP64_LOCATOR_SIG {
            return Zip64::Absent;
        }
        let record_at = locator.u64(8);
        if record_at
            .checked_add(ZIP64_EOCD_LEN)
            .is_none_or(|end| end > locator_at)
        {
            return Zip64::Invalid;
        }
        let Ok(raw) = read_record::<S, 56>(src, record_at) else {
            return Zip64::Invalid;
        };
        let record = Record(&raw);
        if record.u32(0) != ZIP64_EOCD_SIG {
            return Zip64::Invalid;
        }
        let directory = Self {
            offset: record.u64(48),
            size: record.u64(40),
            entries: record.u64(32),
        };
        Zip64::Found(directory, record_at)
    }

    fn end(&self) -> u64 {
        self.offset.saturating_add(self.size)
    }

    fn start(&self) -> Cursor {
        Cursor {
            offset: self.offset,
            index: 0,
        }
    }

    fn is_exhausted(&self, cursor: Cursor) -> bool {
        cursor.offset >= self.end() || cursor.index >= self.entries
    }

    /// Reads the central header that starts at `at`.
    fn read_header<S: Source>(&self, src: &mut S, at: u64) -> Result<Header, Error> {
        let fixed_end = at.checked_add(CENTRAL_LEN).ok_or(Error::BadCentralEntry)?;
        if fixed_end > self.end() {
            return Err(Error::BadCentralEntry);
        }
        let raw: [u8; 46] = read_record(src, at)?;
        let header = Record(&raw);
        if header.u32(0) != CENTRAL_SIG {
            return Err(Error::BadCentralEntry);
        }
        let name_len = header.u16(28);
        let extra_len = header.u16(30);
        let comment_len = header.u16(32);
        let extra_at = fixed_end.saturating_add(u64::from(name_len));
        let next_offset = extra_at
            .saturating_add(u64::from(extra_len))
            .saturating_add(u64::from(comment_len));
        if next_offset > self.end() {
            return Err(Error::BadCentralEntry);
        }

        let mut entry = Entry {
            method: header.u16(10),
            flags: header.u16(8),
            crc32: header.u32(16),
            compressed_size: u64::from(header.u32(20)),
            uncompressed_size: u64::from(header.u32(24)),
            local_header_offset: u64::from(header.u32(42)),
        };
        let sentinels = [header.u32(24), header.u32(20), header.u32(42)].map(|v| v == u32::MAX);
        if sentinels.contains(&true) {
            read_zip64_extra(src, extra_at, extra_len, sentinels, &mut entry)?;
        }
        Ok(Header {
            entry,
            name_len,
            name_offset: fixed_end,
            next_offset,
        })
    }

    /// Calls `visit` with each entry's name and metadata, in directory order,
    /// until it returns [`ControlFlow::Break`]. Names longer than
    /// [`MAX_NAME_LEN`] are skipped.
    ///
    /// # Errors
    /// A malformed entry ends the walk with an error; entries before it have
    /// already been visited.
    pub fn for_each<S: Source>(
        &self,
        src: &mut S,
        mut visit: impl FnMut(&[u8], &Entry) -> ControlFlow<()>,
    ) -> Result<(), Error> {
        let mut name = [0u8; MAX_NAME_LEN];
        let mut cursor = self.start();
        while !self.is_exhausted(cursor) {
            let header = self.read_header(src, cursor.offset)?;
            if let Some(name) = name.get_mut(..usize::from(header.name_len)) {
                src.read_exact_at(header.name_offset, name)?;
                if visit(name, &header.entry).is_break() {
                    break;
                }
            }
            cursor = Cursor {
                offset: header.next_offset,
                index: cursor.index + 1,
            };
        }
        Ok(())
    }

    /// Finds the entry named `name`, starting at `cursor` and wrapping around
    /// once. On a match `cursor` moves past the entry.
    ///
    /// # Errors
    /// A malformed entry ends the search with an error.
    pub fn find<S: Source>(
        &self,
        src: &mut S,
        name: &[u8],
        cursor: &mut Cursor,
    ) -> Result<Option<Entry>, Error> {
        let start = if cursor.offset >= self.offset && !self.is_exhausted(*cursor) {
            *cursor
        } else {
            self.start()
        };
        let mut at = start;
        let mut wrapped = false;
        loop {
            if self.is_exhausted(at) {
                if wrapped || start == self.start() {
                    return Ok(None);
                }
                at = self.start();
                wrapped = true;
            }
            if wrapped && at.offset >= start.offset {
                return Ok(None);
            }
            let header = self.read_header(src, at.offset)?;
            let next = Cursor {
                offset: header.next_offset,
                index: at.index + 1,
            };
            if name_matches(src, &header, name)? {
                *cursor = next;
                return Ok(Some(header.entry));
            }
            at = next;
        }
    }

    /// Byte range of `entry`'s data, checked against its local header and
    /// the start of the central directory.
    ///
    /// # Errors
    /// [`Error::BadLocalHeader`], [`Error::DataOutOfBounds`],
    /// [`Error::StoredSizeMismatch`], [`Error::Encrypted`] or [`Error::Io`].
    pub fn data_range<S: Source>(&self, src: &mut S, entry: &Entry) -> Result<Range<u64>, Error> {
        if entry.flags & FLAG_ENCRYPTED != 0 {
            return Err(Error::Encrypted);
        }
        if entry.method == METHOD_STORED && entry.compressed_size != entry.uncompressed_size {
            return Err(Error::StoredSizeMismatch);
        }
        let header_end = entry
            .local_header_offset
            .checked_add(LOCAL_LEN)
            .filter(|&end| end <= self.offset)
            .ok_or(Error::BadLocalHeader)?;
        let raw: [u8; 30] = read_record(src, entry.local_header_offset)?;
        let header = Record(&raw);
        if header.u32(0) != LOCAL_SIG {
            return Err(Error::BadLocalHeader);
        }
        let start = header_end
            .checked_add(u64::from(header.u16(26)) + u64::from(header.u16(28)))
            .ok_or(Error::DataOutOfBounds)?;
        match start.checked_add(entry.compressed_size) {
            Some(end) if end <= self.offset => Ok(start..end),
            _ => Err(Error::DataOutOfBounds),
        }
    }
}

fn name_matches<S: Source>(src: &mut S, header: &Header, name: &[u8]) -> Result<bool, Error> {
    if usize::from(header.name_len) != name.len() {
        return Ok(false);
    }
    let mut buf = [0u8; 64];
    let mut at = header.name_offset;
    for want in name.chunks(buf.len()) {
        let got = buf.get_mut(..want.len()).ok_or(Error::Io)?;
        src.read_exact_at(at, got)?;
        if got != want {
            return Ok(false);
        }
        at = at.saturating_add(want.len() as u64);
    }
    Ok(true)
}

/// Replaces the 32-bit fields marked in `sentinels` (uncompressed size,
/// compressed size, local header offset) with their ZIP64 extra field values,
/// when the entry has a ZIP64 extra field.
fn read_zip64_extra<S: Source>(
    src: &mut S,
    mut at: u64,
    extra_len: u16,
    sentinels: [bool; 3],
    entry: &mut Entry,
) -> Result<(), Error> {
    let end = at.saturating_add(u64::from(extra_len));
    while at.saturating_add(4) <= end {
        let raw: [u8; 4] = read_record(src, at)?;
        let field = Record(&raw);
        let (id, len) = (field.u16(0), u64::from(field.u16(2)));
        at += 4;
        if at.saturating_add(len) > end {
            break;
        }
        if id == ZIP64_EXTRA_ID {
            let mut values = [0u8; 24];
            let used = values
                .get_mut(..usize::try_from(len.min(24)).unwrap_or(0))
                .ok_or(Error::Io)?;
            src.read_exact_at(at, used)?;
            let values = Record(values.as_slice());
            let mut next = 0;
            let mut take = |missing: bool, field: &mut u64| -> Result<(), Error> {
                if missing {
                    if next + 8 > len {
                        return Err(Error::BadCentralEntry);
                    }
                    *field = values.u64(usize::try_from(next).unwrap_or(usize::MAX));
                    next += 8;
                }
                Ok(())
            };
            let [uncompressed, compressed, offset] = sentinels;
            take(uncompressed, &mut entry.uncompressed_size)?;
            take(compressed, &mut entry.compressed_size)?;
            take(offset, &mut entry.local_header_offset)?;
            return Ok(());
        }
        at += len;
    }
    // No ZIP64 field: keep the 32-bit values, as other readers do, so one odd
    // entry does not make the whole directory unreadable.
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    reason = "tests"
)]
mod tests;
