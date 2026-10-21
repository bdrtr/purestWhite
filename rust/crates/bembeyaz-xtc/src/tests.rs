#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

extern crate std;

use super::{Book, Error, Source, chapters, is_book_magic, load_page, open, page, stream_page};
use std::string::String;
use std::vec;
use std::vec::Vec;

struct File(Vec<u8>);

/// A book before `open`, as `XtcParser` starts it.
fn fresh() -> Book {
    Book {
        page_table_offset: 0,
        data_offset: 0,
        page_count: 0,
        width: 480,
        height: 800,
        bit_depth: 1,
        has_chapters: false,
        title_len: 0,
        author_len: 0,
        title: [0; 127],
        author: [0; 63],
    }
}

impl Source for File {
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Option<usize> {
        let start = usize::try_from(offset)
            .ok()
            .filter(|&o| o <= self.0.len())?;
        let n = buf.len().min(self.0.len() - start);
        buf[..n].copy_from_slice(&self.0[start..start + n]);
        Some(n)
    }
    fn size(&mut self) -> u64 {
        self.0.len() as u64
    }
}

fn put(v: &mut Vec<u8>, at: usize, value: u64, bytes: usize) {
    if v.len() < at + bytes {
        v.resize(at + bytes, 0);
    }
    for i in 0..bytes {
        v[at + i] = (value >> (8 * i)) as u8;
    }
}

/// A 1-bit book of `pages` 8x2 pages with a title and the given chapters (name, first, last).
fn book(pages: usize, chapters: &[(&str, u16, u16)]) -> Vec<u8> {
    let mut v = vec![0u8; 0xF8];
    v[..4].copy_from_slice(b"XTC\0");
    v[4] = 1;
    put(&mut v, 6, pages as u64, 2);
    v[9] = 1;
    v[0x38..0x3D].copy_from_slice(b"Title");
    v[0xB8..0xBA].copy_from_slice(b"Me");
    if !chapters.is_empty() {
        v[0x0B] = 1;
        let len = v.len() as u64;
        put(&mut v, 0x30, len, 4);
        for (name, first, last) in chapters {
            let at = v.len();
            v.resize(at + 96, 0);
            v[at..at + name.len()].copy_from_slice(name.as_bytes());
            put(&mut v, at + 0x50, u64::from(*first), 2);
            put(&mut v, at + 0x52, u64::from(*last), 2);
        }
    }
    let table = v.len();
    put(&mut v, 0x18, table as u64, 8);
    v.resize(table + 16 * pages, 0);
    for i in 0..pages {
        let at = v.len();
        put(&mut v, table + 16 * i, at as u64, 8);
        put(&mut v, table + 16 * i + 12, 8, 2);
        put(&mut v, table + 16 * i + 14, 2, 2);
        v.extend_from_slice(b"XTG\0");
        put(&mut v, at + 4, 8, 2);
        put(&mut v, at + 6, 2, 2);
        put(&mut v, at + 10, 2, 4);
        v.resize(at + 22, 0);
        v.extend_from_slice(&[i as u8, 0xFF]);
    }
    v
}

#[test]
fn opens_reads_pages_and_chapters() {
    let mut file = File(book(3, &[("One", 1, 2), ("", 0, 0)]));
    let mut b = fresh();
    assert_eq!(open(&mut file, &mut b), Ok(()));
    assert_eq!((b.page_count, b.width, b.height, b.bit_depth), (3, 8, 2, 1));
    assert_eq!(&b.title[..usize::from(b.title_len)], b"Title");
    assert_eq!(&b.author[..usize::from(b.author_len)], b"Me");
    assert!(b.has_chapters);
    assert_eq!(page(&mut file, &b, 2).unwrap().width, 8);
    assert_eq!(page(&mut file, &b, 3), None);
    let mut buf = [0u8; 4];
    assert_eq!(load_page(&mut file, &b, 1, &mut buf), Ok(2));
    assert_eq!(buf[..2], [1, 0xFF]);
    assert_eq!(
        load_page(&mut file, &b, 1, &mut buf[..1]),
        Err(Error::MemoryError)
    );
    assert_eq!(
        load_page(&mut file, &b, 3, &mut buf),
        Err(Error::PageOutOfRange)
    );
    let mut pieces = Vec::new();
    let mut chunk = [0u8; 1];
    assert_eq!(
        stream_page(&mut file, &b, 2, &mut chunk, |data, at| pieces
            .push((data[0], at))),
        Ok(())
    );
    assert_eq!(pieces, [(2, 0), (0xFF, 1)]);
    let mut names = Vec::new();
    assert_eq!(
        chapters(
            &mut file,
            &b,
            |n| assert_eq!(n, 2),
            |name, first, last| names.push((
                String::from_utf8(name.to_vec()).unwrap(),
                first,
                last
            ))
        ),
        Ok(true)
    );
    assert_eq!(names, [(String::from("One"), 0, 1)]);
}

#[test]
fn refuses_bad_books() {
    let good = book(1, &[]);
    let check = |data: Vec<u8>| open(&mut File(data), &mut fresh());
    let mut magic = good.clone();
    magic[0] = b'Q';
    assert_eq!(check(magic), Err(Error::InvalidMagic));
    let mut version = good.clone();
    version[5] = 3;
    assert_eq!(check(version), Err(Error::InvalidVersion));
    assert_eq!(check(good[..50].to_vec()), Err(Error::ReadError));
    let mut table = good.clone();
    put(&mut table, 0x18, u64::MAX, 8);
    assert_eq!(check(table), Err(Error::CorruptedHeader));
    assert!(is_book_magic(b"XTCH"));
    assert!(!is_book_magic(b"XTC"));
}

#[test]
fn corrupted_books_never_panic() {
    let base = book(3, &[("One", 1, 2), ("Two", 3, 9)]);
    let mut seed = 0x1357_9BDF_u32;
    for i in 0..base.len() {
        for _ in 0..4 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut copy = base.clone();
            copy[i] = seed.to_le_bytes()[0];
            if seed.is_multiple_of(5) {
                copy.truncate(seed as usize % copy.len());
            }
            let mut file = File(copy);
            let mut b = fresh();
            let _ = open(&mut file, &mut b);
            let mut buf = [0u8; 8];
            for index in 0..4 {
                let _ = load_page(&mut file, &b, index, &mut buf);
                let _ = stream_page(&mut file, &b, index, &mut buf[..3], |_, _| {});
            }
            let _ = chapters(&mut file, &b, |_| {}, |_, _, _| {});
        }
    }
}
