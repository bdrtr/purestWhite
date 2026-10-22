//! `book.bin` and the two files it is built from (`test/book_metadata_cache` pins them). Packed,
//! little-endian; a string is a u32 length, then its bytes.
//!
//! - A spine record: its href (string), the cumulative size of the items through it (u32) and
//!   its TOC entry (i16, -1 for none); the temporary file's records hold 0 and -1.
//! - A TOC record: its title, href and anchor (strings), its level (u8) and its spine item (i16,
//!   -1 for none).
//! - `book.bin`: [`VERSION`] (u8), where the LUTs start (u32), the spine and TOC counts (u16 each),
//!   five metadata strings (title, author, language, cover href, text reference href); the spine
//!   LUT then the TOC LUT (each record's position, u32); then the spine records and the TOC
//!   records.
//!
//! A build writes the spine records, then the TOC records, to two temporary files as the OPF and
//! the TOC are parsed; [`build`] then writes `book.bin` from them.

use bembeyaz_io::{Cursor, Out, Sink, Source};

/// Changes when `book.bin` changes; a file of another version is rebuilt.
pub const VERSION: u8 = 11;
/// Spine items per window of cumulative sizes.
pub const WINDOW: usize = 32;

const HEADER_A: u32 = 1 + 4 + 2 + 2;

/// A string field.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Field {
    Title = 0,
    Author = 1,
    Language = 2,
    Cover = 3,
    TextReference = 4,
    Href = 5,
    TocTitle = 6,
    Anchor = 7,
}

const METADATA: [Field; 5] = [
    Field::Title,
    Field::Author,
    Field::Language,
    Field::Cover,
    Field::TextReference,
];

/// Holds what a read finds.
pub trait Target {
    /// Room for string `field`'s `len` bytes (only for one that is not empty).
    fn text(&mut self, field: Field, len: usize) -> Option<&mut [u8]>;
}

// Reads a string into the target's room. False when the file cannot hold it (the reads after it
// fail too) or there is no room.
fn read_text<S: Source>(cur: &mut Cursor<S>, target: &mut impl Target, field: Field) -> bool {
    let Some(len) = cur.string_len() else {
        return false;
    };
    if len == 0 {
        return true;
    }
    if let Some(room) = target.text(field, len) {
        return cur.read(room);
    }
    // No room: past it, so the fields after it read as they are.
    let _ = cur.skip(u32::try_from(len).unwrap_or(u32::MAX));
    false
}

// `base + index * 4` as a position; past u32, the end.
fn lut_entry(base: u32, index: u32) -> u32 {
    u32::try_from(u64::from(base) + u64::from(index) * 4).unwrap_or(u32::MAX)
}

// A LUT entry, read alone rather than through a cursor's window; None when it cannot be read.
fn lut_at(file: &mut impl Source, at: u32) -> Option<u32> {
    let mut b = [0u8; 4];
    file.read_at(at, &mut b).then(|| u32::from_le_bytes(b))
}

/// What [`load`] read. `bb_book_header` in C.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Header {
    pub lut: u32,
    pub spine_count: u16,
    pub toc_count: u16,
}

/// Why [`load`] turned a file down.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadError {
    Version(u8),
    /// The header or the metadata cannot be read, or there is no room for it.
    Damaged,
}

/// Reads `book.bin`'s header and metadata (into `target`).
///
/// # Errors
///
/// Another version, or a header or metadata that cannot be read or has no room.
pub fn load(
    file: &mut impl Source,
    buf: &mut [u8],
    target: &mut impl Target,
) -> Result<Header, LoadError> {
    let mut cur = Cursor::new(file, 0, buf);
    let version = cur.u8();
    if version != VERSION {
        return Err(LoadError::Version(version));
    }
    let lut = cur.u32_there().ok_or(LoadError::Damaged)?;
    let mut counts = [0u8; 4];
    if !cur.read(&mut counts) {
        return Err(LoadError::Damaged);
    }
    for field in METADATA {
        if !read_text(&mut cur, target, field) {
            return Err(LoadError::Damaged);
        }
    }
    Ok(Header {
        lut,
        spine_count: u16::from_le_bytes([counts[0], counts[1]]),
        toc_count: u16::from_le_bytes([counts[2], counts[3]]),
    })
}

/// Reads the cumulative size before each window of spine items into `starts` (one per window:
/// `spine_count` divided by [`WINDOW`], rounded up). Returns the last item's, the book's size. A
/// spine record that cannot be read gives zero sizes from it on.
pub fn window_starts(
    file: &mut impl Source,
    buf: &mut [u8],
    header: &Header,
    starts: &mut [u32],
) -> u32 {
    let at = lut_entry(
        header.lut,
        u32::from(header.spine_count) + u32::from(header.toc_count),
    );
    let mut cur = Cursor::new(file, at, buf);
    let mut book_size = 0;
    for i in 0..usize::from(header.spine_count) {
        if i % WINDOW == 0
            && let Some(start) = starts.get_mut(i / WINDOW)
        {
            *start = book_size;
        }
        // The href, then the size; a cut record reads as zero, and so does every one after it.
        let _ = cur.skip_string();
        book_size = cur.u32();
        let _ = cur.i16();
    }
    book_size
}

/// Reads spine item `index` (its href into `target`): its cumulative size and TOC entry. A field
/// that cannot be read reads as empty or zero.
pub fn spine_entry(
    file: &mut impl Source,
    buf: &mut [u8],
    lut: u32,
    index: u16,
    target: &mut impl Target,
) -> (u32, i16) {
    let at = lut_at(file, lut_entry(lut, u32::from(index))).unwrap_or(0);
    let mut cur = Cursor::new(file, at, buf);
    let _ = read_text(&mut cur, target, Field::Href);
    (cur.u32(), cur.i16())
}

/// Reads TOC entry `index` (its title, href and anchor into `target`): its level and spine item.
/// As [`spine_entry`].
pub fn toc_entry(
    file: &mut impl Source,
    buf: &mut [u8],
    lut: u32,
    spine_count: u16,
    index: u16,
    target: &mut impl Target,
) -> (u8, i16) {
    let at = lut_at(
        file,
        lut_entry(lut, u32::from(spine_count) + u32::from(index)),
    )
    .unwrap_or(0);
    let mut cur = Cursor::new(file, at, buf);
    for field in [Field::TocTitle, Field::Href, Field::Anchor] {
        let _ = read_text(&mut cur, target, field);
    }
    (cur.u8(), cur.i16())
}

/// Reads the cumulative sizes of the window of spine items from `first` (as many as there are,
/// up to [`WINDOW`]) into `window`. False when any of them cannot be read.
pub fn fill_window(
    file: &mut impl Source,
    buf: &mut [u8],
    lut: u32,
    spine_count: u16,
    first: u16,
    window: &mut [u32; WINDOW],
) -> bool {
    let Some(at) = lut_at(file, lut_entry(lut, u32::from(first))) else {
        return false;
    };
    let mut cur = Cursor::new(file, at, buf);
    let items = usize::from(spine_count.saturating_sub(first)).min(WINDOW);
    for size in window.iter_mut().take(items) {
        let Some(len) = cur.u32_there() else {
            return false;
        };
        let Some(value) = cur.skip(len).then(|| cur.u32_there()).flatten() else {
            return false;
        };
        *size = value;
        if !cur.skip(2) {
            return false;
        }
    }
    true
}

/// The cumulative sizes a loaded `book.bin` keeps in memory: the size before each window, and
/// one window read from the file.
pub struct Sizes<'a> {
    pub spine_count: u16,
    pub book_size: u32,
    pub starts: &'a [u32],
    /// The window's first item, or -1 for none.
    pub window_first: &'a mut i32,
    pub window: &'a mut [u32; WINDOW],
}

impl Sizes<'_> {
    fn start(&self, window: usize) -> u32 {
        self.starts.get(window).copied().unwrap_or(0)
    }

    // Has the window from `first` read, through `fill`; false when it cannot be.
    fn hold(&mut self, first: i32, fill: impl FnOnce(i32, &mut [u32; WINDOW]) -> bool) -> bool {
        if *self.window_first == first {
            return true;
        }
        *self.window_first = -1;
        if fill(first, self.window) {
            *self.window_first = first;
            true
        } else {
            false
        }
    }

    /// The cumulative size up to and including spine item `index` (0 out of range). A window
    /// that cannot be read answers the total before it.
    pub fn cumulative(
        &mut self,
        index: i32,
        fill: impl FnOnce(i32, &mut [u32; WINDOW]) -> bool,
    ) -> u32 {
        let count = i32::from(self.spine_count);
        if index < 0 || index >= count {
            return 0;
        }
        if index == count - 1 {
            return self.book_size;
        }
        let window = usize::try_from(index).unwrap_or(0) / WINDOW;
        let at = usize::try_from(index).unwrap_or(0) % WINDOW;
        // A window's last item's total is the next window's start.
        if at == WINDOW - 1 {
            return self.start(window + 1);
        }
        let first = i32::try_from(window * WINDOW).unwrap_or(0);
        if !self.hold(first, fill) {
            return self.start(window);
        }
        self.window.get(at).copied().unwrap_or(0)
    }

    /// The first spine item whose cumulative size reaches `size` (-1 past the book or without
    /// items). Reads at most one window; one that cannot be read answers its last item.
    pub fn index_for(
        &mut self,
        size: u32,
        fill: impl FnOnce(i32, &mut [u32; WINDOW]) -> bool,
    ) -> i32 {
        if self.spine_count == 0 || size > self.book_size {
            return -1;
        }
        // The answer lies in the window before the first start (after the first) that reaches it.
        let later = self.starts.get(1..).unwrap_or_default();
        let window = later.partition_point(|&start| start < size);
        let first = i32::try_from(window * WINDOW).unwrap_or(0);
        let last =
            (first + i32::try_from(WINDOW).unwrap_or(0)).min(i32::from(self.spine_count)) - 1;
        if !self.hold(first, fill) {
            return last;
        }
        let items = usize::try_from(last - first).unwrap_or(0);
        match self
            .window
            .iter()
            .take(items)
            .position(|&total| total >= size)
        {
            Some(i) => first + i32::try_from(i).unwrap_or(0),
            None => last,
        }
    }
}

/// Writes a spine record at `at`. Returns where it ends, unless the write failed.
pub fn write_spine_record(
    sink: &mut impl Sink,
    at: u32,
    href: &[u8],
    cumulative: u32,
    toc: i16,
) -> Option<u32> {
    let mut out = Out::new(sink, at);
    out.push_string(href);
    out.push(&cumulative.to_le_bytes());
    out.push(&toc.to_le_bytes());
    out.finish()
}

/// Writes a TOC record at `at`. As [`write_spine_record`].
pub fn write_toc_record(
    sink: &mut impl Sink,
    at: u32,
    texts: [&[u8]; 3],
    level: u8,
    spine: i16,
) -> Option<u32> {
    let mut out = Out::new(sink, at);
    for text in texts {
        out.push_string(text);
    }
    out.push(&[level]);
    out.push(&spine.to_le_bytes());
    out.finish()
}

fn skip_spine<S: Source>(cur: &mut Cursor<S>) -> bool {
    cur.skip_string() && cur.skip(4 + 2)
}

fn skip_toc<S: Source>(cur: &mut Cursor<S>) -> bool {
    cur.skip_string() && cur.skip_string() && cur.skip_string() && cur.skip(1 + 2)
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;

// Feeds `bytes` to an FNV-1a 64-bit hash.
fn fnv64_feed(hash: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(hash, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The FNV-1a 64-bit hash, as `ZipFile::fnvHash64`.
#[must_use]
pub fn fnv64(bytes: &[u8]) -> u64 {
    fnv64_feed(FNV_OFFSET, bytes)
}

/// A spine href in the hash index. `bb_book_index_entry` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct IndexEntry {
    pub hash: u64,
    pub len: u16,
    pub spine: i16,
}

// Calls `visit` with each of the `count` spine records' href, read in pieces, then (None) its
// end. `visit` returning true on a piece skips the rest of that href; true at the end stops.
// Stops at a record that cannot be read. Returns how many it read whole.
fn hrefs<S: Source>(
    cur: &mut Cursor<S>,
    count: u16,
    mut visit: impl FnMut(u16, Option<&[u8]>) -> bool,
) -> u16 {
    let mut piece = [0u8; 64];
    for i in 0..count {
        let Some(len) = cur.string_len() else {
            return i;
        };
        let mut left = len;
        while left > 0 {
            let n = left.min(piece.len());
            let Some(part) = piece.get_mut(..n) else {
                return i;
            };
            if !cur.read(part) {
                return i;
            }
            if visit(i, Some(part)) {
                // The visitor is done with this record's href.
                let _ = cur.skip(u32::try_from(left - n).unwrap_or(u32::MAX));
                left = n;
            }
            left -= n;
        }
        if !cur.skip(4 + 2) {
            return i;
        }
        if visit(i, None) {
            return i + 1;
        }
    }
    count
}

/// Fills `entries` (one per spine record of the temporary spine file, `count` of them) with
/// each href's hash and length, for [`find_in_index`] once sorted by them. False when a record
/// cannot be read.
pub fn index_spine(
    file: &mut impl Source,
    buf: &mut [u8],
    count: u16,
    entries: &mut [IndexEntry],
) -> bool {
    let mut cur = Cursor::new(file, 0, buf);
    let mut hash = FNV_OFFSET;
    let mut len = 0usize;
    let read = hrefs(&mut cur, count, |i, part| {
        if let Some(bytes) = part {
            hash = fnv64_feed(hash, bytes);
            len += bytes.len();
        } else {
            if let Some(e) = entries.get_mut(usize::from(i)) {
                // Wraps past 16 bits, as `find_in_index`'s key does.
                #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                {
                    *e = IndexEntry {
                        hash,
                        len: len as u16,
                        spine: i as i16,
                    };
                }
            }
            hash = FNV_OFFSET;
            len = 0;
        }
        false
    });
    read == count
}

/// The spine item of `href` in `entries` (sorted by hash, then length), -1 for none.
#[must_use]
pub fn find_in_index(entries: &[IndexEntry], href: &[u8]) -> i16 {
    #[allow(clippy::cast_possible_truncation)]
    let key = (fnv64(href), href.len() as u16);
    let at = entries.partition_point(|e| (e.hash, e.len) < key);
    entries
        .get(at)
        .filter(|e| (e.hash, e.len) == key)
        .map_or(-1, |e| e.spine)
}

/// The first of the `count` spine records of the temporary spine file whose href is `href`, -1
/// for none.
pub fn find_spine(file: &mut impl Source, buf: &mut [u8], count: u16, href: &[u8]) -> i16 {
    let mut cur = Cursor::new(file, 0, buf);
    let mut matched = 0usize;
    let mut same = true;
    let mut found = -1;
    hrefs(&mut cur, count, |i, part| {
        if let Some(bytes) = part {
            same = same && href.get(matched..matched + bytes.len()) == Some(bytes);
            matched += bytes.len();
            return !same;
        }
        if same && matched == href.len() {
            found = i16::try_from(i).unwrap_or(-1);
            return true;
        }
        matched = 0;
        same = true;
        false
    });
    found
}

// Each spine item's first TOC entry, into `toc_map`.
fn map_tocs<S: Source>(
    tocs: &mut Cursor<S>,
    toc_count: u16,
    toc_map: &mut [i16],
) -> Result<(), BuildError> {
    toc_map.fill(-1);
    tocs.seek(0);
    for j in 0..toc_count {
        if !(tocs.skip_string() && tocs.skip_string() && tocs.skip_string() && tocs.skip(1)) {
            return Err(BuildError::Damaged);
        }
        let item = tocs.i16();
        if let Some(slot) = usize::try_from(item).ok().and_then(|i| toc_map.get_mut(i))
            && *slot == -1
        {
            // An entry past 32767 wraps, as the C++ index cast does.
            #[allow(clippy::cast_possible_wrap)]
            {
                *slot = j as i16;
            }
        }
    }
    Ok(())
}

// Copies the TOC records as they are.
fn copy_tocs<S: Source, K: Sink>(
    tocs: &mut Cursor<S>,
    toc_count: u16,
    out: &mut Out<K>,
) -> Result<(), BuildError> {
    tocs.seek(0);
    let mut piece = [0u8; 128];
    for _ in 0..toc_count {
        for _ in 0..3 {
            let len = tocs.string_len().ok_or(BuildError::Damaged)?;
            out.push(&u32::try_from(len).unwrap_or(u32::MAX).to_le_bytes());
            let mut left = len;
            while left > 0 {
                let n = left.min(piece.len());
                let part = piece.get_mut(..n).ok_or(BuildError::Damaged)?;
                if !tocs.read(part) {
                    return Err(BuildError::Damaged);
                }
                out.push(part);
                left -= n;
            }
        }
        let mut tail = [0u8; 3];
        if !tocs.read(&mut tail) {
            return Err(BuildError::Damaged);
        }
        out.push(&tail);
    }
    Ok(())
}

/// Hands each of the `count` records' href to `items` (room for it, then its size, whose answer
/// is not used), in order. False when a record cannot be read or there is no room.
pub fn spine_hrefs(
    file: &mut impl Source,
    buf: &mut [u8],
    count: u16,
    items: &mut impl Build,
) -> bool {
    let mut cur = Cursor::new(file, 0, buf);
    for i in 0..count {
        let Some(len) = cur.string_len() else {
            return false;
        };
        let read = match items.href(len) {
            Some(room) => cur.read(room),
            None => false,
        };
        if !read || !cur.skip(4 + 2) {
            return false;
        }
        let _ = items.size(i);
    }
    true
}

/// Why [`build`] failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildError {
    /// A temporary file's record cannot be read.
    Damaged,
    /// No room for an href.
    Memory,
    Write,
}

/// Gives a build what it needs as it writes the spine records.
pub trait Build {
    /// Room for the next spine item's href, `len` bytes (also when 0).
    fn href(&mut self, len: usize) -> Option<&mut [u8]>;
    /// The size of spine item `index`, whose href is in the room given last.
    fn size(&mut self, index: u16) -> u32;
}

/// The two temporary files, read through their buffers.
pub struct Parts<'a, S: Source, T: Source> {
    pub spine: &'a mut S,
    pub spine_buf: &'a mut [u8],
    pub spine_count: u16,
    pub toc: &'a mut T,
    pub toc_buf: &'a mut [u8],
    pub toc_count: u16,
}

/// Writes `book.bin` to `sink` from the two temporary files: the header and `metadata`, the
/// LUTs, the spine records with their cumulative sizes and TOC entries, and the TOC records.
/// Each spine item's TOC entry is the first that names it, else the one before's; `toc_map`
/// (one per spine item) is used for that.
///
/// # Errors
///
/// A temporary record that cannot be read, no room for an href, or a write that failed.
pub fn build<S: Source, T: Source>(
    parts: Parts<S, T>,
    sink: &mut impl Sink,
    metadata: [&[u8]; 5],
    toc_map: &mut [i16],
    items: &mut impl Build,
) -> Result<(), BuildError> {
    let Parts {
        spine,
        spine_buf,
        spine_count,
        toc,
        toc_buf,
        toc_count,
    } = parts;
    let text_bytes: u32 = metadata
        .iter()
        .map(|m| u32::try_from(m.len()).unwrap_or(u32::MAX))
        .fold(0, u32::wrapping_add);
    let lut = HEADER_A.wrapping_add(text_bytes).wrapping_add(4 * 5);
    let lut_size = 4 * (u32::from(spine_count) + u32::from(toc_count));
    let mut out = Out::new(sink, 0);
    out.push(&[VERSION]);
    out.push(&lut.to_le_bytes());
    out.push(&spine_count.to_le_bytes());
    out.push(&toc_count.to_le_bytes());
    for m in metadata {
        out.push_string(m);
    }
    let records = lut.wrapping_add(lut_size);
    let mut spines = Cursor::new(spine, 0, spine_buf);
    for _ in 0..spine_count {
        out.push(&spines.position().wrapping_add(records).to_le_bytes());
        if !skip_spine(&mut spines) {
            return Err(BuildError::Damaged);
        }
    }
    let spine_bytes = spines.position();
    let mut tocs = Cursor::new(toc, 0, toc_buf);
    for _ in 0..toc_count {
        out.push(
            &tocs
                .position()
                .wrapping_add(records)
                .wrapping_add(spine_bytes)
                .to_le_bytes(),
        );
        if !skip_toc(&mut tocs) {
            return Err(BuildError::Damaged);
        }
    }
    map_tocs(&mut tocs, toc_count, toc_map)?;
    spines.seek(0);
    let mut total = 0u32;
    let mut last_toc = -1i16;
    for i in 0..spine_count {
        let len = spines.string_len().ok_or(BuildError::Damaged)?;
        let room = items.href(len).ok_or(BuildError::Memory)?;
        if !spines.read(room) {
            return Err(BuildError::Damaged);
        }
        out.push_string(room);
        if !spines.skip(4 + 2) {
            return Err(BuildError::Damaged);
        }
        let mut toc_index = toc_map.get(usize::from(i)).copied().unwrap_or(-1);
        if toc_index == -1 {
            toc_index = last_toc;
        }
        last_toc = toc_index;
        total = total.wrapping_add(items.size(i));
        out.push(&total.to_le_bytes());
        out.push(&toc_index.to_le_bytes());
    }
    copy_tocs(&mut tocs, toc_count, &mut out)?;
    out.finish().map(|_| ()).ok_or(BuildError::Write)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{
        Build, BuildError, Field, IndexEntry, LoadError, Parts, Sizes, Target, VERSION, WINDOW,
        build, fill_window, find_in_index, find_spine, fnv64, index_spine, load, spine_entry,
        toc_entry, write_spine_record, write_toc_record,
    };
    use bembeyaz_io::testing::Bytes;
    use std::string::String;
    use std::vec;
    use std::vec::Vec;

    #[derive(Default)]
    struct Texts(Vec<(Field, Vec<u8>)>);

    impl Target for Texts {
        fn text(&mut self, field: Field, len: usize) -> Option<&mut [u8]> {
            self.0.push((field, vec![0; len]));
            Some(&mut self.0.last_mut().unwrap().1)
        }
    }

    struct Items {
        href: Vec<u8>,
        sizes: Vec<u32>,
        seen: Vec<(u16, Vec<u8>)>,
    }

    impl Build for Items {
        fn href(&mut self, len: usize) -> Option<&mut [u8]> {
            self.href = vec![0; len];
            Some(&mut self.href)
        }
        fn size(&mut self, index: u16) -> u32 {
            if self.sizes.is_empty() {
                self.seen.push((index, self.href.clone()));
                return 0;
            }
            self.sizes[usize::from(index)]
        }
    }

    // Spine a, b, c, d; TOC entries for a, c, d (twice) and one outside the spine.
    fn temporary() -> (Bytes, Bytes) {
        let mut spine = Bytes::new(Vec::new());
        let mut at = 0;
        for href in [&b"a"[..], b"b", b"c", b"d"] {
            at = write_spine_record(&mut spine, at, href, 0, -1).unwrap();
        }
        let mut toc = Bytes::new(Vec::new());
        let mut at = 0;
        for (title, spine_index) in [("A", 0), ("C", 2), ("D", 3), ("D2", 3), ("X", -1)] {
            at = write_toc_record(&mut toc, at, [title.as_bytes(), b"h", b""], 1, spine_index)
                .unwrap();
        }
        (spine, toc)
    }

    fn built() -> Bytes {
        let (mut spine, mut toc) = temporary();
        let mut out = Bytes::new(Vec::new());
        let (mut b1, mut b2) = ([0u8; 64], [0u8; 64]);
        let mut map = [0i16; 4];
        let mut items = Items {
            href: Vec::new(),
            sizes: vec![100, 0, 50, 7],
            seen: Vec::new(),
        };
        build(
            Parts {
                spine: &mut spine,
                spine_buf: &mut b1,
                spine_count: 4,
                toc: &mut toc,
                toc_buf: &mut b2,
                toc_count: 5,
            },
            &mut out,
            [b"T", b"", b"tr", b"c.jpg", b"a"],
            &mut map,
            &mut items,
        )
        .unwrap();
        assert_eq!(map, [0, -1, 1, 2]);
        out
    }

    #[test]
    fn a_built_book_reads_back() {
        let mut file = built();
        assert_eq!(file.data[0], VERSION);
        let mut buf = [0u8; 256];
        let mut texts = Texts::default();
        let mut starts = vec![9u32; 1];
        let loaded = load(&mut file, &mut buf, &mut texts).unwrap();
        assert_eq!((loaded.spine_count, loaded.toc_count), (4, 5));
        assert_eq!(
            super::window_starts(&mut file, &mut buf, &loaded, &mut starts),
            157
        );
        assert_eq!(starts, [0]);
        assert_eq!(texts.0.len(), 4); // the empty author asks for no room
        assert_eq!(texts.0[0], (Field::Title, b"T".to_vec()));

        let mut texts = Texts::default();
        assert_eq!(
            spine_entry(&mut file, &mut buf, loaded.lut, 1, &mut texts),
            (100, 0)
        );
        assert_eq!(texts.0[0].1, b"b");
        let mut texts = Texts::default();
        assert_eq!(
            toc_entry(&mut file, &mut buf, loaded.lut, 4, 3, &mut texts),
            (1, 3)
        );
        assert_eq!(
            texts.0.iter().map(|t| t.1.clone()).collect::<Vec<_>>(),
            [b"D2".to_vec(), b"h".to_vec()]
        );

        let mut window = [0u32; WINDOW];
        assert!(fill_window(
            &mut file,
            &mut buf,
            loaded.lut,
            4,
            0,
            &mut window
        ));
        assert_eq!(&window[..4], &[100, 100, 150, 157]);
        let mut first = -1;
        let mut sizes = Sizes {
            spine_count: 4,
            book_size: 157,
            starts: &starts,
            window_first: &mut first,
            window: &mut window,
        };
        let never = |_: i32, _: &mut [u32; WINDOW]| -> bool { unreachable!() };
        assert_eq!(sizes.cumulative(3, never), 157);
        assert_eq!(sizes.cumulative(4, never), 0);
        let mut fills = 0;
        let mut refill = |f: i32, w: &mut [u32; WINDOW]| {
            fills += 1;
            assert_eq!(f, 0);
            w[..4].copy_from_slice(&[100, 100, 150, 157]);
            true
        };
        assert_eq!(sizes.cumulative(2, &mut refill), 150);
        assert_eq!(sizes.index_for(101, &mut refill), 2);
        assert_eq!(sizes.index_for(100, &mut refill), 0);
        assert_eq!(sizes.index_for(158, &mut refill), -1);
        assert_eq!(fills, 1);
    }

    #[test]
    fn a_window_that_cannot_be_read_answers_the_total_before_it() {
        let starts = [0u32, 500, 900];
        let mut window = [0u32; WINDOW];
        let mut first = -1;
        let mut sizes = Sizes {
            spine_count: 70,
            book_size: 1000,
            starts: &starts,
            window_first: &mut first,
            window: &mut window,
        };
        let fail = |_: i32, _: &mut [u32; WINDOW]| false;
        assert_eq!(sizes.cumulative(40, fail), 500);
        assert_eq!(sizes.cumulative(31, fail), 500); // a window's last item, from memory
        assert_eq!(sizes.cumulative(63, fail), 900);
        assert_eq!(sizes.index_for(600, fail), 63);
        assert_eq!(sizes.index_for(1000, fail), 69);
    }

    #[test]
    fn hrefs_are_found_linearly_and_through_the_index() {
        let (mut spine, _) = temporary();
        let mut buf = [0u8; 16];
        assert_eq!(find_spine(&mut spine, &mut buf, 4, b"c"), 2);
        assert_eq!(find_spine(&mut spine, &mut buf, 4, b"e"), -1);
        assert_eq!(find_spine(&mut spine, &mut buf, 2, b"c"), -1);
        let mut entries = vec![IndexEntry::default(); 4];
        assert!(index_spine(&mut spine, &mut buf, 4, &mut entries));
        assert_eq!(
            entries[2],
            IndexEntry {
                hash: fnv64(b"c"),
                len: 1,
                spine: 2
            }
        );
        entries.sort_by_key(|e| (e.hash, e.len));
        assert_eq!(find_in_index(&entries, b"d"), 3);
        assert_eq!(find_in_index(&entries, b"dd"), -1);
        assert!(!index_spine(&mut spine, &mut buf, 5, &mut entries));
    }

    #[test]
    fn spine_hrefs_come_in_order() {
        let (mut spine, _) = temporary();
        let mut buf = [0u8; 8];
        let mut items = Items {
            href: Vec::new(),
            sizes: Vec::new(),
            seen: Vec::new(),
        };
        assert!(super::spine_hrefs(&mut spine, &mut buf, 4, &mut items));
        assert_eq!(items.seen[3], (3, b"d".to_vec()));
        assert!(!super::spine_hrefs(&mut spine, &mut buf, 5, &mut items));
    }

    #[test]
    fn long_hrefs_are_read_in_pieces() {
        let long: String = "x".repeat(300);
        let mut spine = Bytes::new(Vec::new());
        let at = write_spine_record(&mut spine, 0, b"short", 0, -1).unwrap();
        write_spine_record(&mut spine, at, long.as_bytes(), 0, -1).unwrap();
        let mut buf = [0u8; 64];
        assert_eq!(find_spine(&mut spine, &mut buf, 2, long.as_bytes()), 1);
        assert_eq!(
            find_spine(&mut spine, &mut buf, 2, &long.as_bytes()[..299]),
            -1
        );
        let mut entries = vec![IndexEntry::default(); 2];
        assert!(index_spine(&mut spine, &mut buf, 2, &mut entries));
        assert_eq!(entries[1].hash, fnv64(long.as_bytes()));
        assert_eq!(entries[1].len, 300);
    }

    #[test]
    fn damage_is_refused_or_reads_empty() {
        let good = built();
        let mut buf = [0u8; 256];
        let mut bytes = good.data.clone();
        bytes[0] = VERSION + 1;
        assert_eq!(
            load(&mut Bytes::new(bytes), &mut buf, &mut Texts::default()),
            Err(LoadError::Version(VERSION + 1))
        );
        assert_eq!(
            load(
                &mut Bytes::new(good.data[..11].to_vec()),
                &mut buf,
                &mut Texts::default()
            ),
            Err(LoadError::Damaged)
        );
        // A temporary record cut short fails the build.
        let (mut spine, mut toc) = temporary();
        spine.data.truncate(spine.data.len() - 1);
        let (mut b1, mut b2) = ([0u8; 64], [0u8; 64]);
        let result = build(
            Parts {
                spine: &mut spine,
                spine_buf: &mut b1,
                spine_count: 4,
                toc: &mut toc,
                toc_buf: &mut b2,
                toc_count: 5,
            },
            &mut Bytes::new(Vec::new()),
            [b""; 5],
            &mut [0; 4],
            &mut Items {
                href: Vec::new(),
                sizes: vec![0; 4],
                seen: Vec::new(),
            },
        );
        assert_eq!(result, Err(BuildError::Damaged));
    }

    #[test]
    fn any_book_never_panics() {
        let good = built().data;
        let mut seed = 0xb00c_ca5e_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut buf = [0u8; 64];
        for _ in 0..3000 {
            let mut bytes = good.clone();
            for _ in 0..=next() % 6 {
                let at = next() as usize % bytes.len();
                bytes[at] = next() as u8;
            }
            if next() % 3 == 0 {
                bytes.truncate(next() as usize % bytes.len());
            }
            let mut file = Bytes::new(bytes);
            let mut starts = vec![0u32; 64];
            let lut = match load(&mut file, &mut buf, &mut Texts::default()) {
                Ok(h) => {
                    let _ = super::window_starts(&mut file, &mut buf, &h, &mut starts);
                    h.lut
                }
                Err(_) => next(),
            };
            let i = (next() % 8) as u16;
            let _ = spine_entry(&mut file, &mut buf, lut, i, &mut Texts::default());
            let _ = toc_entry(&mut file, &mut buf, lut, 4, i, &mut Texts::default());
            let _ = fill_window(&mut file, &mut buf, lut, 70, i, &mut [0; WINDOW]);
            let _ = find_spine(&mut file, &mut buf, 8, b"a");
            let _ = index_spine(&mut file, &mut buf, 8, &mut [IndexEntry::default(); 8]);
        }
        // Damaged temporary files.
        let (spine, toc) = temporary();
        for _ in 0..2000 {
            let mut parts = [spine.data.clone(), toc.data.clone()];
            for part in &mut parts {
                for _ in 0..=next() % 4 {
                    let at = next() as usize % part.len();
                    part[at] = next() as u8;
                }
                if next() % 3 == 0 {
                    part.truncate(next() as usize % part.len());
                }
            }
            let [s, t] = parts;
            let (mut s, mut t) = (Bytes::new(s), Bytes::new(t));
            let (mut b1, mut b2) = ([0u8; 32], [0u8; 32]);
            let (sc, tc) = ((next() % 8) as u16, (next() % 8) as u16);
            let mut map = [0i16; 8];
            let mut items = Items {
                href: Vec::new(),
                sizes: vec![1; 8],
                seen: Vec::new(),
            };
            let _ = build(
                Parts {
                    spine: &mut s,
                    spine_buf: &mut b1,
                    spine_count: sc,
                    toc: &mut t,
                    toc_buf: &mut b2,
                    toc_count: tc,
                },
                &mut Bytes::new(Vec::new()),
                [b"x"; 5],
                &mut map[..usize::from(sc)],
                &mut items,
            );
            let _ = super::spine_hrefs(&mut s, &mut b1, sc, &mut items);
        }
    }
}
