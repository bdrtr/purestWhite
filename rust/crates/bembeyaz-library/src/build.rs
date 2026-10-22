//! Building the Library index from lib/LibraryIndex's stage file: the sort keys read from the
//! staged books, the vote on one author spelling per person, and the records, permutations and
//! name blobs written after the header and folders. The builder keeps the walk that writes the
//! stage, the memory, the sorts and the files.
//!
//! The stage holds one [`STAGE_STRIDE`]-byte entry per book, in walk order (`StagedEntry`): the
//! book's 128-byte record, its path hash, its file name, its own (source) author spelling, and the
//! title it gives itself. `order` maps title positions to stage entries.

use alloc::vec::Vec;
use bembeyaz_io::{Out, Sink};

use crate::format::{
    Header, RECORD_AUTHOR_KEY, RECORD_AUTHOR_KEY_LEN, RECORD_FIRST_SEEN, RECORD_FOLD,
    RECORD_FOLD_LEN, RECORD_MODIFICATION_TIME, RECORD_NAME_LEN, RECORD_NAME_OFF, Source,
};

pub const STAGE_STRIDE: u32 = 776;
pub const STAGE_PATH_HASH: u32 = 128;
pub const STAGE_NAME: u32 = 136;
pub const STAGE_AUTHOR_LEN: u32 = 391;
pub const STAGE_AUTHOR: u32 = 392;
pub const STAGE_TITLE_LEN: u32 = 520;
pub const STAGE_TITLE: u32 = 521;
pub const STAGE_AUTHOR_BYTES: usize = 128;
const RECORD_BYTES: usize = 128;
const KEY_BYTES: usize = 12;
/// Sorts after every folded byte: books without an author go last.
const NO_KEY: [u8; KEY_BYTES] = [0xFF; KEY_BYTES];

/// A sort key's first bytes and the position it belongs to (`bb_library_sort_key`). Ties fall
/// back to the ordinal, so the builder's sort is total.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SortKey {
    pub key: [u8; KEY_BYTES],
    pub ordinal: u16,
}

/// One author spelling in a vote (`bb_library_spelling`).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Spelling {
    pub text: [u8; STAGE_AUTHOR_BYTES],
    pub ordinal: u16,
    pub count: u16,
    pub len: u8,
}

/// Which key [`stage_keys`] reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    /// The fold of book `i` in walk order (`order` unused).
    Title,
    /// The author key of the book at title position `i`.
    Author,
    /// The surname of the chosen spelling of the book at title position `i`: the source spelling of
    /// the book at `canonical[i]`.
    Surname,
}

fn entry_at(order: &[u16], position: u16) -> Option<u32> {
    order
        .get(usize::from(position))
        .map(|&entry| u32::from(entry) * STAGE_STRIDE)
}

fn key_of(bytes: &[u8]) -> [u8; KEY_BYTES] {
    let mut key = [0; KEY_BYTES];
    for (k, b) in key.iter_mut().zip(bytes) {
        *k = *b;
    }
    key
}

// A staged entry's source author spelling, into `text`; its length.
fn read_author(
    stage: &mut impl Source,
    at: u32,
    text: &mut [u8; STAGE_AUTHOR_BYTES],
) -> Option<usize> {
    let mut len = [0u8];
    if !stage.read_at(at + STAGE_AUTHOR_LEN, &mut len) {
        return None;
    }
    let len = usize::from(len[0]).min(STAGE_AUTHOR_BYTES);
    if len > 0 && !stage.read_at(at + STAGE_AUTHOR, text.get_mut(..len)?) {
        return None;
    }
    Some(len)
}

/// Fills `keys` (one per book, `ordinal` its position): `Title` from the stage in walk order,
/// `Author` and `Surname` through `order`; `Surname` reads the spelling the book at
/// `canonical[i]` shows. False when an entry cannot be read.
pub fn stage_keys(
    stage: &mut impl Source,
    which: Key,
    order: &[u16],
    canonical: &[u16],
    keys: &mut [SortKey],
) -> bool {
    let mut record = [0u8; RECORD_BYTES];
    let mut author = [0u8; STAGE_AUTHOR_BYTES];
    let mut surname = Vec::new();
    for (i, slot) in (0u16..).zip(keys.iter_mut()) {
        let key = match which {
            Key::Title | Key::Author => {
                let at = if which == Key::Title {
                    u32::from(i) * STAGE_STRIDE
                } else {
                    let Some(at) = entry_at(order, i) else {
                        return false;
                    };
                    at
                };
                if !stage.read_at(at, &mut record) {
                    return false;
                }
                let (len_at, bytes_at) = if which == Key::Title {
                    (RECORD_FOLD_LEN, RECORD_FOLD)
                } else {
                    (RECORD_AUTHOR_KEY_LEN, RECORD_AUTHOR_KEY)
                };
                let len = usize::from(record.get(len_at).copied().unwrap_or(0));
                if which == Key::Author && len == 0 {
                    NO_KEY
                } else {
                    key_of(
                        record
                            .get(bytes_at..)
                            .unwrap_or(&[])
                            .get(..len.min(KEY_BYTES))
                            .unwrap_or(&[]),
                    )
                }
            }
            Key::Surname => {
                let at = canonical
                    .get(usize::from(i))
                    .and_then(|&from| entry_at(order, from));
                let Some(len) = at.and_then(|at| read_author(stage, at, &mut author)) else {
                    return false;
                };
                surname.clear();
                // Out of memory, the book sorts as one without an author.
                if len > 0
                    && crate::surname_key(author.get(..len).unwrap_or(&[]), &mut surname).is_err()
                {
                    surname.clear();
                }
                if surname.is_empty() {
                    NO_KEY
                } else {
                    key_of(&surname)
                }
            }
        };
        *slot = SortKey { key, ordinal: i };
    }
    true
}

/// One spelling per person: within each run of equal author keys in `keys` (sorted), every book
/// takes, in `canonical`, the title position of the spelling most of them carry, ties going to the
/// shorter and then the first in byte order. Runs without an author key, and runs of one, are left
/// alone; `scratch` bounds the distinct spellings counted per run. False when an entry cannot be
/// read.
pub fn vote_spellings(
    stage: &mut impl Source,
    order: &[u16],
    keys: &[SortKey],
    canonical: &mut [u16],
    scratch: &mut [Spelling],
) -> bool {
    let mut author = [0u8; STAGE_AUTHOR_BYTES];
    let mut run_start = 0;
    while let Some(first) = keys.get(run_start) {
        let run_len = keys
            .get(run_start..)
            .unwrap_or(&[])
            .iter()
            .take_while(|k| k.key == first.key)
            .count();
        let run = keys.get(run_start..run_start + run_len).unwrap_or(&[]);
        run_start += run_len;
        if first.key[0] == NO_KEY[0] || run.len() < 2 {
            continue;
        }
        let mut count = 0;
        for member in run {
            let Some(len) =
                entry_at(order, member.ordinal).and_then(|at| read_author(stage, at, &mut author))
            else {
                return false;
            };
            if len == 0 {
                continue;
            }
            let text = author.get(..len).unwrap_or(&[]);
            let seen = scratch.get_mut(..count).unwrap_or(&mut []);
            if let Some(spelling) = seen
                .iter_mut()
                .find(|s| s.text.get(..usize::from(s.len)) == Some(text))
            {
                spelling.count += 1;
            } else if let Some(spelling) = scratch.get_mut(count) {
                spelling.text = author;
                spelling.ordinal = member.ordinal;
                spelling.count = 1;
                // len is at most STAGE_AUTHOR_BYTES.
                spelling.len = u8::try_from(len).unwrap_or(u8::MAX);
                count += 1;
            }
        }
        let mut best: Option<&Spelling> = None;
        for spelling in scratch.get(..count).unwrap_or(&[]) {
            let better = best.is_none_or(|b| {
                (spelling.count, core::cmp::Reverse(spelling.len))
                    > (b.count, core::cmp::Reverse(b.len))
                    || (spelling.count == b.count
                        && spelling.len == b.len
                        && spelling_text(spelling) < spelling_text(b))
            });
            if better {
                best = Some(spelling);
            }
        }
        let chosen = best.map_or(first.ordinal, |b| b.ordinal);
        for member in run {
            if let Some(slot) = canonical.get_mut(usize::from(member.ordinal)) {
                *slot = chosen;
            }
        }
    }
    true
}

// A record field's offset, for the stage reads.
fn offset(field: usize) -> u32 {
    u32::try_from(field).unwrap_or(u32::MAX)
}

fn spelling_text(s: &Spelling) -> &[u8] {
    s.text.get(..usize::from(s.len)).unwrap_or(&[])
}

/// The modification time of the book at each title position into `out`. False when an entry
/// cannot be read.
pub fn stage_modification_times(stage: &mut impl Source, order: &[u16], out: &mut [u32]) -> bool {
    for (i, slot) in (0u16..).zip(out.iter_mut()) {
        let mut b = [0u8; 4];
        match entry_at(order, i) {
            Some(at) if stage.read_at(at + offset(RECORD_MODIFICATION_TIME), &mut b) => {
                *slot = u32::from_le_bytes(b);
            }
            _ => return false,
        }
    }
    true
}

/// What [`write_body`] writes from.
pub struct Body<'a> {
    /// Title position to stage entry, one per book.
    pub order: &'a [u16],
    /// The title position whose spelling each book shows; `None`: its own.
    pub canonical: Option<&'a [u16]>,
    /// Each stage entry's arrival number; `None`: the staged one.
    pub first_seen: Option<&'a [u16]>,
    /// Records by author; `None`: in title order.
    pub author: Option<&'a [SortKey]>,
    /// Records by arrival.
    pub arrival: &'a [u16],
}

// Reads the stage entry at title position `position` into `buf`.
fn fetch(stage: &mut impl Source, order: &[u16], position: u16, buf: &mut [u8]) -> bool {
    entry_at(order, position).is_some_and(|at| stage.read_at(at, buf))
}

fn field(entry: &[u8], at: u32, len: usize) -> &[u8] {
    entry
        .get(usize::try_from(at).unwrap_or(usize::MAX)..)
        .and_then(|rest| rest.get(..len))
        .unwrap_or(&[])
}

fn byte(entry: &[u8], at: u32) -> usize {
    usize::from(field(entry, at, 1).first().copied().unwrap_or(0))
}

// A name blob's bytes: path hash, name, then the shown author, title and source author, each
// behind a length byte.
fn blob_bytes(entry: &[u8], canonical: &[u8]) -> u32 {
    let name = byte(entry, offset(RECORD_NAME_LEN));
    let total = 8
        + name
        + 1
        + byte(canonical, STAGE_AUTHOR_LEN)
        + 1
        + byte(entry, STAGE_TITLE_LEN)
        + 1
        + byte(entry, STAGE_AUTHOR_LEN);
    // At most 8 + 255 * 4 + 3.
    u32::try_from(total).unwrap_or(u32::MAX)
}

fn pad_to(w: &mut Out<'_, impl Sink>, target: u32) {
    while let Some(at) = w.position().filter(|&at| at < target) {
        let gap = usize::try_from((target - at).min(64)).unwrap_or(64);
        w.push([0u8; 64].get(..gap).unwrap_or(&[]));
    }
}

/// Writes the records (title order, with their name offsets and arrival numbers), both
/// permutations and the name blobs to `sink`, from the header's record start (the header and
/// folders before it are the caller's). `entries` holds two stage entries. The name blobs' length,
/// or `None` when an entry cannot be read, `entries` is too small or a write fails.
pub fn write_body(
    stage: &mut impl Source,
    header: &Header,
    body: &Body<'_>,
    entries: &mut [u8],
    sink: &mut impl Sink,
) -> Option<u32> {
    let stride = STAGE_STRIDE as usize;
    let (entry, canonical) = entries.get_mut(..stride * 2)?.split_at_mut(stride);
    let header = *header;
    let n = header.book_count;
    let mut w = Out::new(sink, header.record_start);
    let canonical_of = |i: u16| {
        body.canonical
            .map_or(Some(i), |c| c.get(usize::from(i)).copied())
    };

    let mut cursor = 0u32;
    for i in 0..n {
        if !fetch(stage, body.order, i, entry)
            || !fetch(stage, body.order, canonical_of(i)?, canonical)
        {
            return None;
        }
        let mut record = [0u8; RECORD_BYTES];
        record.copy_from_slice(entry.get(..RECORD_BYTES)?);
        record
            .get_mut(RECORD_NAME_OFF..RECORD_NAME_OFF + 4)?
            .copy_from_slice(&cursor.to_le_bytes());
        if let Some(first_seen) = body.first_seen {
            let walk = usize::from(*body.order.get(usize::from(i))?);
            record
                .get_mut(RECORD_FIRST_SEEN..RECORD_FIRST_SEEN + 2)?
                .copy_from_slice(&first_seen.get(walk)?.to_le_bytes());
        }
        cursor = cursor.wrapping_add(blob_bytes(entry, canonical));
        w.push(&record);
    }
    pad_to(&mut w, header.perm_start);
    for k in 0..n {
        let ordinal = match body.author {
            Some(author) => author.get(usize::from(k))?.ordinal,
            None => k,
        };
        w.push(&ordinal.to_le_bytes());
    }
    for k in 0..n {
        w.push(&body.arrival.get(usize::from(k))?.to_le_bytes());
    }
    pad_to(&mut w, header.name_start);

    let mut written = 0u32;
    for i in 0..n {
        if !fetch(stage, body.order, i, entry)
            || !fetch(stage, body.order, canonical_of(i)?, canonical)
        {
            return None;
        }
        w.push(field(entry, STAGE_PATH_HASH, 8));
        w.push(field(
            entry,
            STAGE_NAME,
            byte(entry, offset(RECORD_NAME_LEN)),
        ));
        for (from, len_at, text_at) in [
            (&*canonical, STAGE_AUTHOR_LEN, STAGE_AUTHOR),
            (&*entry, STAGE_TITLE_LEN, STAGE_TITLE),
            (&*entry, STAGE_AUTHOR_LEN, STAGE_AUTHOR),
        ] {
            let len = byte(from, len_at);
            w.push(field(from, len_at, 1));
            w.push(field(from, text_at, len));
        }
        written = written.wrapping_add(blob_bytes(entry, canonical));
    }
    w.finish().map(|_| written)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        clippy::unwrap_used
    )]

    use super::{
        Body, Key, NO_KEY, STAGE_STRIDE, SortKey, Spelling, stage_keys, stage_modification_times,
        vote_spellings, write_body,
    };
    use crate::format::{self, Header, Record, Source};
    use std::vec;
    use std::vec::Vec;

    struct Stage {
        data: Vec<u8>,
        failed: bool,
    }

    impl Source for Stage {
        fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
            let at = offset as usize;
            match self.data.get(at..at + buf.len()) {
                Some(src) if !self.failed => {
                    buf.copy_from_slice(src);
                    true
                }
                _ => {
                    self.failed = true;
                    false
                }
            }
        }
        fn fail(&mut self) {
            self.failed = true;
        }
    }

    struct Book<'a> {
        name: &'a str,
        fold: &'a str,
        key: &'a str,
        author: &'a str,
        title: &'a str,
        mtime: u32,
    }

    fn stage(books: &[Book<'_>]) -> Stage {
        let mut data = Vec::new();
        for (i, b) in books.iter().enumerate() {
            let mut e = vec![0u8; STAGE_STRIDE as usize];
            e[8..10].copy_from_slice(&(100 + i as u16).to_le_bytes());
            e[12] = b.name.len() as u8;
            e[13] = b.fold.len() as u8;
            e[14] = b.key.len() as u8;
            e[16..16 + b.fold.len()].copy_from_slice(b.fold.as_bytes());
            e[112..112 + b.key.len()].copy_from_slice(b.key.as_bytes());
            e[124..128].copy_from_slice(&b.mtime.to_le_bytes());
            e[128..136].copy_from_slice(&(0x1111 * (i as u64 + 1)).to_le_bytes());
            e[136..136 + b.name.len()].copy_from_slice(b.name.as_bytes());
            e[391] = b.author.len() as u8;
            e[392..392 + b.author.len()].copy_from_slice(b.author.as_bytes());
            e[520] = b.title.len() as u8;
            e[521..521 + b.title.len()].copy_from_slice(b.title.as_bytes());
            data.extend(e);
        }
        Stage {
            data,
            failed: false,
        }
    }

    const BOOKS: [Book<'static>; 5] = [
        Book {
            name: "a.epub",
            fold: "zebra",
            key: "lu xun",
            author: "Xun Lu",
            title: "Zebra",
            mtime: 5,
        },
        Book {
            name: "b.epub",
            fold: "apple",
            key: "lu xun",
            author: "Lu Xun",
            title: "",
            mtime: 1,
        },
        Book {
            name: "c.epub",
            fold: "mango",
            key: "lu xun",
            author: "Lu Xun",
            title: "Mango",
            mtime: 3,
        },
        Book {
            name: "d.txt",
            fold: "kiwi",
            key: "",
            author: "???",
            title: "",
            mtime: 2,
        },
        Book {
            name: "e.epub",
            fold: "fig",
            key: "hugo victor",
            author: "Victor Hugo",
            title: "Fig",
            mtime: 4,
        },
    ];

    fn keys(n: usize) -> Vec<SortKey> {
        vec![
            SortKey {
                key: [0; 12],
                ordinal: 0
            };
            n
        ]
    }

    #[test]
    fn fills_keys_and_votes_one_spelling_per_person() {
        let mut s = stage(&BOOKS);
        let mut title = keys(5);
        assert!(stage_keys(&mut s, Key::Title, &[], &[], &mut title));
        assert_eq!(&title[1].key[..5], b"apple");
        assert_eq!(title[1].ordinal, 1);
        // Title order: apple, fig, kiwi, mango, zebra.
        let order = [1u16, 4, 3, 2, 0];
        let mut author = keys(5);
        assert!(stage_keys(&mut s, Key::Author, &order, &[], &mut author));
        assert_eq!(author[2].key, NO_KEY);
        assert_eq!(&author[0].key[..6], b"lu xun");
        author.sort_by_key(|k| (k.key, k.ordinal));
        let mut canonical: Vec<u16> = (0..5).collect();
        let mut scratch = [Spelling {
            text: [0; 128],
            ordinal: 0,
            count: 0,
            len: 0,
        }; 16];
        assert!(vote_spellings(
            &mut s,
            &order,
            &author,
            &mut canonical,
            &mut scratch
        ));
        // "Lu Xun" (positions 0 and 3) outvotes "Xun Lu" (4): all three show position 0's
        // spelling; the others keep their own.
        assert_eq!(canonical, [0, 1, 2, 0, 0]);
        let mut surname = keys(5);
        assert!(stage_keys(
            &mut s,
            Key::Surname,
            &order,
            &canonical,
            &mut surname
        ));
        assert_eq!(surname[2].key, NO_KEY);
        assert_eq!(surname[0].key, surname[4].key);
        let mut times = [0u32; 5];
        assert!(stage_modification_times(&mut s, &order, &mut times));
        assert_eq!(times, [1, 4, 2, 3, 5]);
        // A cut stage.
        s.data.truncate(STAGE_STRIDE as usize * 4 + 10);
        assert!(!stage_keys(&mut s, Key::Title, &[], &[], &mut keys(5)));
    }

    #[test]
    fn writes_records_permutations_and_blobs_the_reader_reads() {
        let mut s = stage(&BOOKS);
        let order = [1u16, 4, 3, 2, 0];
        let canonical = [0u16, 1, 2, 0, 0];
        let first_seen = [7u16, 8, 9, 10, 11];
        let author: Vec<SortKey> = [4u16, 3, 0, 1, 2]
            .iter()
            .map(|&ordinal| SortKey {
                key: [0; 12],
                ordinal,
            })
            .collect();
        let arrival = [0u16, 2, 3, 1, 4];
        let mut h = Header {
            book_count: 5,
            folder_count: 1,
            ..Header::default()
        };
        format::layout_sections(&mut h, 3, 0);
        let mut out = bembeyaz_io::testing::Bytes::new(vec![0u8; h.record_start as usize]);
        let body = Body {
            order: &order,
            canonical: Some(&canonical),
            first_seen: Some(&first_seen),
            author: Some(&author),
            arrival: &arrival,
        };
        let mut entries = vec![0u8; STAGE_STRIDE as usize * 2];
        let names = write_body(&mut s, &h, &body, &mut entries, &mut out).unwrap();
        format::layout_sections(&mut h, 3, names);
        assert_eq!(out.data.len(), h.self_size as usize);

        let mut file = File(out.data);
        let mut r = Record::from_bytes(&[0; 128]);
        let mut text = Vec::new();
        let expect = [
            ("b.epub", "Lu Xun", "", "Lu Xun", 8u16),
            ("e.epub", "Victor Hugo", "Fig", "Victor Hugo", 11),
            ("d.txt", "???", "", "???", 10),
            ("c.epub", "Lu Xun", "Mango", "Lu Xun", 9),
            ("a.epub", "Lu Xun", "Zebra", "Xun Lu", 7),
        ];
        for (i, (name, shown, title, source, seen)) in expect.iter().enumerate() {
            assert!(format::read_record(&h, i as u16, &mut file, &mut r));
            assert_eq!({ r.first_seen }, *seen);
            assert!(format::read_name(&h, &r, &mut file, &mut text).unwrap());
            assert_eq!(text, name.as_bytes());
            for (field, want) in [shown, title, source].iter().enumerate() {
                assert!(
                    format::read_blob_field(&h, &r, field as u8, &mut file, &mut text).unwrap()
                );
                assert_eq!(text, want.as_bytes(), "{i} {field}");
            }
        }
        for (k, want) in [4u16, 3, 0, 1, 2].iter().enumerate() {
            assert_eq!(
                format::ordinal_for_row(&h, format::Order::AuthorAsc, k as u16, &mut file),
                *want
            );
        }
        assert_eq!(
            format::ordinal_for_row(&h, format::Order::RecentAsc, 3, &mut file),
            1
        );
        // Too little room for two entries, or a stage cut short.
        assert!(
            write_body(
                &mut s,
                &h,
                &body,
                &mut entries[..STAGE_STRIDE as usize],
                &mut out_sink()
            )
            .is_none()
        );
        s.data.truncate(100);
        assert!(write_body(&mut s, &h, &body, &mut entries, &mut out_sink()).is_none());
    }

    // A written index.
    struct File(Vec<u8>);
    impl Source for File {
        fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
            let at = offset as usize;
            self.0
                .get(at..at + buf.len())
                .map(|b| buf.copy_from_slice(b))
                .is_some()
        }
        fn fail(&mut self) {}
    }

    fn out_sink() -> bembeyaz_io::testing::Bytes {
        bembeyaz_io::testing::Bytes::new(Vec::new())
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn any_stage_never_panics() {
        let good = stage(&BOOKS).data;
        let mut seed = 0x5ca1_ab1e_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut scratch = [Spelling {
            text: [0; 128],
            ordinal: 0,
            count: 0,
            len: 0,
        }; 3];
        let mut entries = vec![0u8; STAGE_STRIDE as usize * 2];
        for _ in 0..2000 {
            let mut data = good.clone();
            for _ in 0..=next() % 8 {
                let at = next() as usize % data.len();
                data[at] = next() as u8;
            }
            if next() % 3 == 0 {
                data.truncate(next() as usize % data.len());
            }
            let n = (next() % 7) as u16;
            let order: Vec<u16> = (0..n).map(|_| (next() % 7) as u16).collect();
            let canonical: Vec<u16> = (0..n).map(|_| (next() % 7) as u16).collect();
            let mut s = Stage {
                data,
                failed: false,
            };
            let mut k = keys(usize::from(n));
            for which in [Key::Title, Key::Author, Key::Surname] {
                s.failed = false;
                let _ = stage_keys(&mut s, which, &order, &canonical, &mut k);
            }
            s.failed = false;
            let mut c = canonical.clone();
            let _ = vote_spellings(&mut s, &order, &k, &mut c, &mut scratch);
            let mut h = Header {
                book_count: n,
                ..Header::default()
            };
            format::layout_sections(&mut h, next() % 600, 0);
            let body = Body {
                order: &order,
                canonical: (next() % 2 == 0).then_some(&canonical[..]),
                first_seen: (next() % 2 == 0).then_some(&canonical[..]),
                author: (next() % 2 == 0).then_some(&k[..]),
                arrival: &order,
            };
            s.failed = false;
            let _ = write_body(&mut s, &h, &body, &mut entries, &mut out_sink());
        }
    }
}
