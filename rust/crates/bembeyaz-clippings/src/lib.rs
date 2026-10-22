//! A book's clipping store, as src/ClippingStore keeps it on the SD card: the store file (a
//! header, then each clipping's record and text) and the journal of deleted clippings' ids.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use bembeyaz_io::{Cursor, Out, Sink, Source};

/// The version written. Versions 1 to 3 are read too: 1 and 2 have no layout signature, 1 has a
/// u32 text length, versions up to 3 have no sync fields.
pub const VERSION: u8 = 4;
const LAYOUT_VERSION: u8 = 3;
/// The longest clipping text and header string.
pub const TEXT_MAX: u16 = 4096;
pub const ID_LEN: usize = 65;
#[allow(clippy::cast_possible_truncation)]
const ID_STEP: u32 = ID_LEN as u32;
pub const CHAPTER_LEN: usize = 48;

/// A clipping's record (`bb_clip_record` in clippings.h): where it is in the book, its sync
/// state and where its text is in the store file.
#[repr(C)]
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Record {
    pub spine: u16,
    pub start_page: u16,
    pub end_page: u16,
    pub page_count: u16,
    pub start_word: u16,
    pub end_word: u16,
    pub word_count: u16,
    pub paragraph: u16,
    pub timestamp: u32,
    pub layout: u32,
    pub start_offset: u32,
    pub end_offset: u32,
    pub sync_revision: u64,
    pub id: [u8; ID_LEN],
    pub pending_upload: bool,
    pub chapter: [u8; CHAPTER_LEN],
    pub text_offset: u32,
    pub text_length: u16,
}

impl Record {
    /// A new record; also the values of fields that older file versions do not have.
    pub const EMPTY: Self = Self {
        spine: 0,
        start_page: 0,
        end_page: 0,
        page_count: 1,
        start_word: 0,
        end_word: 0,
        word_count: 0,
        paragraph: u16::MAX,
        timestamp: 0,
        layout: 0,
        start_offset: u32::MAX,
        end_offset: u32::MAX,
        sync_revision: 0,
        id: [0; ID_LEN],
        pending_upload: true,
        chapter: [0; CHAPTER_LEN],
        text_offset: 0,
        text_length: 0,
    };
}

/// The header's strings, in order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Field {
    Title = 0,
    Author = 1,
    Path = 2,
}

/// Takes what a load finds.
pub trait Load {
    /// Room for a header string of `len` bytes; None skips it.
    fn text(&mut self, field: Field, len: usize) -> Option<&mut [u8]>;
    /// Room for `count` records; false when there is none.
    fn begin(&mut self, count: u16) -> bool;
    fn record(&mut self, record: &Record);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadError {
    /// An unknown version, a cut file, or a count, string or text over its limit.
    Damaged,
    /// `begin` found no room.
    NoRoom,
}

// Little-endian fields from the front of a record's bytes.
struct Take<'a>(&'a [u8]);

impl Take<'_> {
    fn bytes<const N: usize>(&mut self) -> [u8; N] {
        match self.0.split_first_chunk::<N>() {
            Some((first, rest)) => {
                self.0 = rest;
                *first
            }
            None => [0; N],
        }
    }
    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.bytes())
    }
    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.bytes())
    }
}

// A record's bytes before its text length, by version.
const fn fixed_len(version: u8) -> usize {
    let base = 8 * 2 + 4 + CHAPTER_LEN;
    match version {
        VERSION => base + 4 + 4 + 4 + 8 + 1 + ID_LEN,
        LAYOUT_VERSION => base + 4,
        _ => base,
    }
}

/// Reads the store: its header strings into `target`, then each record. `max_count` is the most
/// clippings a book holds; `buf` is the read buffer.
///
/// # Errors
///
/// [`LoadError`]; `target` may have taken records before it.
pub fn load(
    file: &mut impl Source,
    buf: &mut [u8],
    max_count: u16,
    target: &mut impl Load,
) -> Result<(), LoadError> {
    use LoadError::Damaged;
    let mut c = Cursor::new(file, 0, buf);
    let mut start = [0u8; 3];
    if !c.read(&mut start) {
        return Err(Damaged);
    }
    let [version, count @ ..] = start;
    let count = u16::from_le_bytes(count);
    if !(1..=VERSION).contains(&version) {
        return Err(Damaged);
    }
    for field in [Field::Title, Field::Author, Field::Path] {
        let len = c.u32_there().ok_or(Damaged)?;
        if len > u32::from(TEXT_MAX) || len > c.remaining() {
            return Err(Damaged);
        }
        let read = match target.text(field, len as usize) {
            Some(room) if room.len() == len as usize => c.read(room),
            _ => c.skip(len),
        };
        if !read {
            return Err(Damaged);
        }
    }
    if count > max_count {
        return Err(Damaged);
    }
    if !target.begin(count) {
        return Err(LoadError::NoRoom);
    }
    let mut fixed = [0u8; fixed_len(VERSION)];
    for _ in 0..count {
        let bytes = fixed.get_mut(..fixed_len(version)).ok_or(Damaged)?;
        if !c.read(bytes) {
            return Err(Damaged);
        }
        let mut t = Take(bytes);
        let mut r = Record::EMPTY;
        r.spine = t.u16();
        r.start_page = t.u16();
        r.end_page = t.u16();
        r.page_count = t.u16();
        r.start_word = t.u16();
        r.end_word = t.u16();
        r.word_count = t.u16();
        r.paragraph = t.u16();
        r.timestamp = t.u32();
        if version >= LAYOUT_VERSION {
            r.layout = t.u32();
        }
        if version >= VERSION {
            r.start_offset = t.u32();
            r.end_offset = t.u32();
            r.sync_revision = u64::from_le_bytes(t.bytes());
            r.pending_upload = t.bytes::<1>() != [0];
            r.id = t.bytes();
        }
        r.chapter = t.bytes();
        if let Some(end) = r.id.last_mut() {
            *end = 0;
        }
        if let Some(end) = r.chapter.last_mut() {
            *end = 0;
        }
        let len = if version == 1 {
            c.u32_there().ok_or(Damaged)?
        } else {
            let mut len = [0u8; 2];
            if !c.read(&mut len) {
                return Err(Damaged);
            }
            let len = u16::from_le_bytes(len);
            if len > TEXT_MAX {
                return Err(Damaged);
            }
            u32::from(len)
        };
        r.text_offset = c.position();
        // Version 1 had no limit: its text is the first TEXT_MAX bytes.
        r.text_length = u16::try_from(len.min(u32::from(TEXT_MAX))).unwrap_or(TEXT_MAX);
        if !c.skip(len) {
            return Err(Damaged);
        }
        target.record(&r);
    }
    Ok(())
}

/// Gives the records a save writes.
pub trait Save {
    /// Fills `out` with record `index`'s fields, its text offset and length (in the old file).
    fn record(&mut self, index: u16, out: &mut Record);
}

/// One record's new text, for a save.
pub struct Replacement<'a> {
    pub index: u16,
    pub text: &'a [u8],
}

/// Writes the store (version 4) from offset 0: the header strings, then `count` records with
/// their texts, each from `replacement` or copied from `old` (the old file and a copy buffer) at
/// the record's text offset; a text longer than the buffer takes several reads and writes.
/// `offsets` takes each text's offset in the new file. False when a header string is
/// over [`TEXT_MAX`], or a write, a copy or `offsets` fails.
pub fn save(
    out: &mut impl Sink,
    mut old: Option<(&mut impl Source, &mut [u8])>,
    header: [&[u8]; 3],
    count: u16,
    records: &mut impl Save,
    replacement: Option<&Replacement<'_>>,
    offsets: &mut [u32],
) -> bool {
    if header.iter().any(|s| s.len() > usize::from(TEXT_MAX)) {
        return false;
    }
    let mut w = Out::new(out, 0);
    w.push(&[VERSION]);
    w.push(&count.to_le_bytes());
    for text in header {
        w.push_string(text);
    }
    let mut r = Record::EMPTY;
    for i in 0..count {
        records.record(i, &mut r);
        for field in [
            r.spine,
            r.start_page,
            r.end_page,
            r.page_count,
            r.start_word,
            r.end_word,
            r.word_count,
            r.paragraph,
        ] {
            w.push(&field.to_le_bytes());
        }
        for field in [r.timestamp, r.layout, r.start_offset, r.end_offset] {
            w.push(&field.to_le_bytes());
        }
        w.push(&r.sync_revision.to_le_bytes());
        w.push(&[u8::from(r.pending_upload)]);
        w.push(&r.id);
        w.push(&r.chapter);
        let new = replacement
            .filter(|x| x.index == i)
            .map(|x| x.text.get(..usize::from(TEXT_MAX)).unwrap_or(x.text));
        let len = new.map_or(r.text_length, |t| {
            u16::try_from(t.len()).unwrap_or(TEXT_MAX)
        });
        w.push(&len.to_le_bytes());
        let (Some(at), Some(slot)) = (w.position(), offsets.get_mut(usize::from(i))) else {
            return false;
        };
        *slot = at;
        match new {
            Some(text) => w.push(text),
            None if len > 0 => {
                let Some((src, buf)) = old.as_mut() else {
                    return false;
                };
                if !copy(*src, r.text_offset, len, buf, &mut w) {
                    return false;
                }
            }
            None => {}
        }
        if !w.ok() {
            return false;
        }
    }
    w.finish().is_some()
}

// Copies `len` bytes from `at` in `src` to `w` through `buf`.
fn copy(
    src: &mut impl Source,
    at: u32,
    len: u16,
    buf: &mut [u8],
    w: &mut Out<'_, impl Sink>,
) -> bool {
    let chunk = u32::try_from(buf.len()).unwrap_or(u32::MAX);
    let Some(end) = at.checked_add(u32::from(len)) else {
        return false;
    };
    if chunk == 0 {
        return false;
    }
    let mut at = at;
    while at < end {
        let n = (end - at).min(chunk);
        let Some(part) = buf.get_mut(..n as usize) else {
            return false;
        };
        if !src.read_at(at, part) {
            return false;
        }
        w.push(part);
        at += n;
    }
    true
}

/// The next id from `*offset` in the deletion journal (65-byte ids, each ending in a zero) that
/// `known` (an id still in the store) does not know, with `*offset` past it; an empty id at the
/// journal's end. False when `*offset` is not a multiple of 65 within the journal, or an id
/// cannot be read.
pub fn next_deletion(
    journal: &mut impl Source,
    offset: &mut u32,
    id: &mut [u8; ID_LEN],
    mut known: impl FnMut(&[u8; ID_LEN]) -> bool,
) -> bool {
    id[0] = 0;
    let size = journal.size();
    if !offset.is_multiple_of(ID_STEP) || *offset > size {
        return false;
    }
    while *offset < size {
        if !journal.read_at(*offset, id) {
            return false;
        }
        *offset = offset.saturating_add(ID_STEP);
        id[ID_LEN - 1] = 0;
        if !known(id) {
            return true;
        }
    }
    id[0] = 0;
    true
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        clippy::panic,
        clippy::unwrap_used
    )]

    use super::{
        Field, ID_LEN, Load, LoadError, Record, Replacement, Save, TEXT_MAX, load, next_deletion,
        save,
    };
    use bembeyaz_io::testing::Bytes;
    use std::vec;
    use std::vec::Vec;

    #[derive(Default)]
    struct Found {
        title: Vec<u8>,
        author: Vec<u8>,
        count: Option<u16>,
        records: Vec<Record>,
        refuse: bool,
    }

    impl Load for Found {
        fn text(&mut self, field: Field, len: usize) -> Option<&mut [u8]> {
            let s = match field {
                Field::Title => &mut self.title,
                Field::Author => &mut self.author,
                Field::Path => return None,
            };
            s.resize(len, 0);
            Some(s)
        }
        fn begin(&mut self, count: u16) -> bool {
            self.count = Some(count);
            !self.refuse
        }
        fn record(&mut self, record: &Record) {
            self.records.push(record.clone());
        }
    }

    struct List(Vec<Record>);

    impl Save for List {
        fn record(&mut self, index: u16, out: &mut Record) {
            *out = self.0[usize::from(index)].clone();
        }
    }

    fn sample(n: u16) -> Record {
        let mut r = Record::EMPTY;
        r.spine = n;
        r.start_page = n + 1;
        r.end_page = n + 2;
        r.page_count = n + 3;
        r.start_word = n + 4;
        r.end_word = n + 5;
        r.word_count = n + 6;
        r.paragraph = n + 7;
        r.timestamp = 0x0102_0304 + u32::from(n);
        r.layout = 9;
        r.start_offset = 10;
        r.end_offset = 11;
        r.sync_revision = 0x1122_3344_5566_7788;
        r.pending_upload = n.is_multiple_of(2);
        r.id[..4].copy_from_slice(b"id-x");
        r.chapter[..3].copy_from_slice(b"Cha");
        r
    }

    // Writes records with the given texts (as replacements one at a time, each save copying the
    // others from the last file).
    fn store(texts: &[&[u8]]) -> (Vec<u8>, Vec<Record>) {
        let mut records: Vec<Record> = (0..texts.len() as u16).map(sample).collect();
        let mut file = Bytes::new(Vec::new());
        for (i, text) in texts.iter().enumerate() {
            let mut out = Bytes::new(Vec::new());
            let mut offsets = vec![0u32; texts.len()];
            let mut old = Bytes::new(file.data.clone());
            let ok = save(
                &mut out,
                Some((&mut old, &mut [0u8; 7][..])),
                [b"Title", b"Author", b"/book.epub"],
                texts.len() as u16,
                &mut List(records.clone()),
                Some(&Replacement {
                    index: i as u16,
                    text,
                }),
                &mut offsets,
            );
            assert!(ok);
            records[i].text_length = text.len() as u16;
            for (r, at) in records.iter_mut().zip(offsets) {
                r.text_offset = at;
            }
            file = out;
        }
        (file.data, records)
    }

    #[test]
    fn saves_and_loads_records_and_texts() {
        let long = vec![b'q'; 4096];
        let (data, records) = store(&[b"one", b"", &[b'z'; 300], &long]);
        let mut found = Found::default();
        assert_eq!(
            load(&mut Bytes::new(data.clone()), &mut [0u8; 64], 8, &mut found),
            Ok(())
        );
        assert_eq!(
            (&found.title[..], &found.author[..]),
            (&b"Title"[..], &b"Author"[..])
        );
        assert_eq!(found.records, records);
        let at = records[2].text_offset as usize;
        assert_eq!(&data[at..at + 300], &[b'z'; 300]);
        assert_eq!(data.len(), records[3].text_offset as usize + 4096);
        // Too many for the book.
        assert_eq!(
            load(
                &mut Bytes::new(data.clone()),
                &mut [0u8; 64],
                3,
                &mut Found::default()
            ),
            Err(LoadError::Damaged)
        );
        let mut refuse = Found {
            refuse: true,
            ..Found::default()
        };
        assert_eq!(
            load(
                &mut Bytes::new(data.clone()),
                &mut [0u8; 64],
                8,
                &mut refuse
            ),
            Err(LoadError::NoRoom)
        );
        // Every cut is damage.
        for len in 0..data.len() {
            assert_eq!(
                load(
                    &mut Bytes::new(data[..len].to_vec()),
                    &mut [0u8; 64],
                    8,
                    &mut Found::default()
                ),
                Err(LoadError::Damaged),
                "{len}"
            );
        }
    }

    #[test]
    fn version_one_text_is_clamped_and_skipped_whole() {
        let mut data = vec![1u8, 2, 0];
        for s in [&b"T"[..], b"", b"/p"] {
            data.extend((s.len() as u32).to_le_bytes());
            data.extend(s);
        }
        for (n, len) in [(1u16, 5000u32), (2, 2)] {
            for f in 0..8 {
                data.extend((n * 10 + f).to_le_bytes());
            }
            data.extend(7u32.to_le_bytes());
            data.extend([b'c'; 48]);
            data.extend(len.to_le_bytes());
            data.extend(vec![b'x'; len as usize]);
        }
        let mut found = Found::default();
        assert_eq!(
            load(&mut Bytes::new(data.clone()), &mut [0u8; 32], 8, &mut found),
            Ok(())
        );
        let [a, b] = &found.records[..] else {
            panic!("two records")
        };
        assert_eq!((a.spine, a.paragraph, a.timestamp), (10, 17, 7));
        assert_eq!(
            (a.layout, a.start_offset, a.pending_upload),
            (0, u32::MAX, true)
        );
        assert_eq!(a.text_length, TEXT_MAX);
        assert_eq!(b.text_offset, a.text_offset + 5000 + 68 + 4);
        assert_eq!((b.spine, b.text_length), (20, 2));
        assert_eq!(&a.chapter[..], &[[b'c'; 47].as_slice(), &[0]].concat()[..]);
        // A text past the end.
        data.truncate(data.len() - 1);
        assert_eq!(
            load(
                &mut Bytes::new(data),
                &mut [0u8; 32],
                8,
                &mut Found::default()
            ),
            Err(LoadError::Damaged)
        );
    }

    #[test]
    fn save_fails_without_the_old_text_or_room_for_offsets() {
        let records = List(vec![Record {
            text_length: 4,
            text_offset: 2,
            ..sample(0)
        }]);
        let header: [&[u8]; 3] = [b"", b"", b""];
        let mut old = Bytes::new(b"..abcd".to_vec());
        let mut out = Bytes::new(Vec::new());
        let mut offsets = [0u32; 1];
        let mut records = records;
        let mut buf = [0u8; 3];
        assert!(save(
            &mut out,
            Some((&mut old, &mut buf[..])),
            header,
            1,
            &mut records,
            None,
            &mut offsets
        ));
        assert_eq!(&out.data[offsets[0] as usize..], b"abcd");
        let mut short = Bytes::new(b"..abc".to_vec());
        assert!(!save(
            &mut out,
            Some((&mut short, &mut buf[..])),
            header,
            1,
            &mut records,
            None,
            &mut offsets
        ));
        assert!(!save(
            &mut out,
            None::<(&mut Bytes, &mut [u8])>,
            header,
            1,
            &mut records,
            None,
            &mut offsets
        ));
        assert!(!save(
            &mut out,
            Some((&mut old, &mut buf[..])),
            header,
            1,
            &mut records,
            None,
            &mut []
        ));
        // No room to copy through.
        assert!(!save(
            &mut out,
            Some((&mut old, &mut [][..])),
            header,
            1,
            &mut records,
            None,
            &mut offsets
        ));
        // A header string it would not read back.
        let long = vec![b't'; usize::from(TEXT_MAX) + 1];
        assert!(!save(
            &mut out,
            Some((&mut old, &mut buf[..])),
            [b"", &long, b""],
            1,
            &mut records,
            None,
            &mut offsets
        ));
    }

    #[test]
    fn deletion_journal_skips_known_ids() {
        let mut journal = Vec::new();
        for id in [&b"a"[..], b"kept", b"kept", &[b'g'; 65]] {
            let mut field = [0u8; ID_LEN];
            field[..id.len()].copy_from_slice(id);
            journal.extend(field);
        }
        let mut file = Bytes::new(journal);
        let known = |id: &[u8; ID_LEN]| id.starts_with(b"kept\0");
        let mut id = [1u8; ID_LEN];
        let mut offset = 0;
        assert!(next_deletion(&mut file, &mut offset, &mut id, known));
        assert_eq!((&id[..2], offset), (&b"a\0"[..], 65));
        assert!(next_deletion(&mut file, &mut offset, &mut id, known));
        assert_eq!((&id[..64], id[64], offset), (&[b'g'; 64][..], 0, 260));
        assert!(next_deletion(&mut file, &mut offset, &mut id, known));
        assert_eq!((id[0], offset), (0, 260));
        for bad in [64, 325] {
            offset = bad;
            assert!(!next_deletion(&mut file, &mut offset, &mut id, known));
            assert_eq!(id[0], 0);
        }
    }

    #[test]
    fn any_store_never_panics() {
        let (good, _) = store(&[b"one", b"two", b"", b"four"]);
        let mut seed = 0xc11b_5eed_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut buf = [0u8; 48];
        for _ in 0..3000 {
            let mut bytes = good.clone();
            for _ in 0..=next() % 6 {
                let at = next() as usize % bytes.len();
                bytes[at] = next() as u8;
            }
            if next() % 3 == 0 {
                bytes.truncate(next() as usize % bytes.len());
            }
            let size = (next() % 7 + 1) as usize * 6;
            let mut found = Found::default();
            if load(
                &mut Bytes::new(bytes.clone()),
                &mut buf[..size],
                8,
                &mut found,
            )
            .is_ok()
            {
                // Whatever loads saves again from itself.
                let count = found.records.len() as u16;
                let mut offsets = vec![0u32; found.records.len()];
                let mut old = Bytes::new(bytes.clone());
                let _ = save(
                    &mut Bytes::new(Vec::new()),
                    Some((&mut old, &mut buf[..size])),
                    [&found.title, &found.author, b""],
                    count,
                    &mut List(found.records),
                    None,
                    &mut offsets,
                );
            }
            let mut offset = next() % 400;
            let mut id = [0u8; ID_LEN];
            let _ = next_deletion(&mut Bytes::new(bytes), &mut offset, &mut id, |_| {
                next().is_multiple_of(2)
            });
        }
    }
}
