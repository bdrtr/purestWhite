#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate std;

use crate::anchors::{Anchors, MAX_ANCHORS};
use crate::footnotes::{Footnotes, HREF_LEN};
use bembeyaz_layout::boxes::Segment;
use bembeyaz_layout::pager::{Line, Pager, Pages};
use std::string::String;
use std::vec::Vec;

/// Counts the pages handed on.
#[derive(Default)]
struct Book {
    emitted: usize,
}

impl Pages for Book {
    fn new_page(&mut self) -> bool {
        true
    }
    fn split(&mut self, _: u8, _: i16) -> bool {
        true
    }
    fn emit(&mut self, _: u32) {
        self.emitted += 1;
    }
    fn add_box(&mut self, _: &Segment) {}
}

fn put_line(pager: &mut Pager, book: &mut Book) {
    pager.line_top(book, 16, 0, Line::default());
    pager.line_added(16, 0, false);
}

fn table(anchors: &Anchors) -> Vec<(String, u16)> {
    (0..anchors.len())
        .map(|i| {
            let (name, page) = anchors.get(i).unwrap();
            (String::from_utf8(name.to_vec()).unwrap(), page)
        })
        .collect()
}

#[test]
fn an_id_lands_on_the_page_its_block_starts() {
    let (mut pager, mut book) = (Pager::new(100), Book::default());
    let mut anchors = Anchors::new();
    put_line(&mut pager, &mut book);
    anchors.open(&mut pager, &mut book, b"a", true).unwrap();
    assert!(anchors.has_pending());
    pager.complete_page(&mut book);
    anchors.place_pending(&mut pager, &mut book).unwrap();
    assert!(!anchors.has_pending());
    // A second id while the first waits places the first where it is.
    anchors.open(&mut pager, &mut book, b"b", true).unwrap();
    anchors.open(&mut pager, &mut book, b"c", true).unwrap();
    anchors.record_pending(7).unwrap();
    assert_eq!(
        table(&anchors),
        [("a".into(), 1), ("b".into(), 1), ("c".into(), 7)]
    );
    assert_eq!(anchors.find(b"b"), Some(1));
    assert_eq!(anchors.find(b"d"), None);
    assert_eq!(book.emitted, 1);
}

#[test]
fn an_empty_id_places_the_waiting_one_and_waits_for_nothing() {
    let (mut pager, mut book) = (Pager::new(100), Book::default());
    let mut anchors = Anchors::new();
    anchors.open(&mut pager, &mut book, b"a", true).unwrap();
    anchors.open(&mut pager, &mut book, b"", true).unwrap();
    assert!(!anchors.has_pending());
    anchors.record_pending(5).unwrap();
    assert_eq!(table(&anchors), [("a".into(), 0)]);
}

#[test]
fn find_gives_the_first_of_the_same_name() {
    let (mut pager, mut book) = (Pager::new(100), Book::default());
    let mut anchors = Anchors::new();
    for page in [2, 4] {
        anchors.open(&mut pager, &mut book, b"x", true).unwrap();
        anchors.record_pending(page).unwrap();
    }
    assert_eq!(anchors.find(b"x"), Some(2));
    let (name, _) = anchors.get(1).unwrap();
    // SAFETY: get's name is followed by its NUL in the buffer.
    assert_eq!(unsafe { *name.as_ptr().add(name.len()) }, 0);
}

#[test]
fn a_table_of_contents_id_starts_a_page() {
    let (mut pager, mut book) = (Pager::new(100), Book::default());
    let mut anchors = Anchors::new();
    anchors.add_toc(b"ch2").unwrap();
    // Not on an empty page.
    anchors.open(&mut pager, &mut book, b"ch2", false).unwrap();
    anchors.place_pending(&mut pager, &mut book).unwrap();
    assert_eq!(book.emitted, 0);
    put_line(&mut pager, &mut book);
    anchors.open(&mut pager, &mut book, b"ch2", false).unwrap();
    anchors.place_pending(&mut pager, &mut book).unwrap();
    assert_eq!(book.emitted, 1);
    assert_eq!(table(&anchors), [("ch2".into(), 0), ("ch2".into(), 1)]);
    // A rule or the chapter's end records without the break.
    put_line(&mut pager, &mut book);
    anchors.open(&mut pager, &mut book, b"ch2", true).unwrap();
    anchors.record_pending(pager.completed()).unwrap();
    assert_eq!(book.emitted, 1);
}

#[test]
fn spans_and_ids_past_the_limit_are_dropped_unless_the_table_of_contents_names_them() {
    let (mut pager, mut book) = (Pager::new(100), Book::default());
    let mut anchors = Anchors::new();
    anchors.add_toc(b"toc").unwrap();
    anchors.open(&mut pager, &mut book, b"span", false).unwrap();
    assert!(!anchors.has_pending());
    for i in 0..MAX_ANCHORS {
        anchors
            .open(
                &mut pager,
                &mut book,
                std::format!("id{i}").as_bytes(),
                true,
            )
            .unwrap();
    }
    anchors.record_pending(0).unwrap();
    assert_eq!(anchors.len(), MAX_ANCHORS);
    anchors.open(&mut pager, &mut book, b"late", true).unwrap();
    assert!(!anchors.has_pending());
    anchors.open(&mut pager, &mut book, b"toc", false).unwrap();
    anchors.record_pending(3).unwrap();
    assert_eq!(anchors.find(b"toc"), Some(3));
    assert_eq!(anchors.len(), MAX_ANCHORS + 1);
}

fn taken(notes: &mut Footnotes, words: i32) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut out = Vec::new();
    notes.take(words, |number, href| {
        out.push((number.to_vec(), href.to_vec()));
    });
    out
}

#[test]
fn a_reference_number_drops_spaces_and_brackets() {
    let mut notes = Footnotes::new();
    notes.open(b"#n1").unwrap();
    assert_eq!(notes.href(), b"#n1\0");
    notes.text(b"   [  12  ]   ");
    notes.close(3).unwrap();
    notes.open(b"#n2").unwrap();
    notes.text(b"   turn to 256  ");
    notes.text(b"]");
    notes.close(5).unwrap();
    assert_eq!(taken(&mut notes, 2), []);
    assert_eq!(
        taken(&mut notes, 3),
        [(b"12\0".to_vec(), b"#n1\0".to_vec())]
    );
    assert!(notes.has_pending());
    let mut all = Vec::new();
    notes.take_all(|number, href| all.push((number.to_vec(), href.to_vec())));
    assert_eq!(all, [(b"turn to 256\0".to_vec(), b"#n2\0".to_vec())]);
    assert!(!notes.has_pending());
}

#[test]
fn references_left_after_a_take_keep_their_text() {
    let mut notes = Footnotes::new();
    for (word, number) in [(1, "1"), (2, "22"), (2, "333"), (4, "4444")] {
        notes.open(std::format!("#n{number}").as_bytes()).unwrap();
        notes.text(number.as_bytes());
        notes.close(word).unwrap();
    }
    assert_eq!(taken(&mut notes, 1), [(b"1\0".to_vec(), b"#n1\0".to_vec())]);
    notes.open(b"#late").unwrap();
    notes.text(b"5");
    notes.close(4).unwrap();
    assert_eq!(
        taken(&mut notes, 3),
        [
            (b"22\0".to_vec(), b"#n22\0".to_vec()),
            (b"333\0".to_vec(), b"#n333\0".to_vec())
        ]
    );
    assert_eq!(
        taken(&mut notes, 9),
        [
            (b"4444\0".to_vec(), b"#n4444\0".to_vec()),
            (b"5\0".to_vec(), b"#late\0".to_vec())
        ]
    );
}

/// Random calls never panic and keep what a plain model keeps.
#[test]
fn mini_fuzz() {
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move |n: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % n
    };
    for _ in 0..200 {
        let (mut pager, mut book) = (Pager::new(64), Book::default());
        let mut anchors = Anchors::new();
        let mut notes = Footnotes::new();
        let mut model: Vec<(Vec<u8>, Vec<u8>, i32)> = Vec::new();
        let mut open: Option<(Vec<u8>, Vec<u8>)> = None;
        for _ in 0..300 {
            let len = usize::try_from(next(6)).unwrap();
            let bytes: Vec<u8> = (0..len)
                .map(|_| b" [x]\0\n9"[usize::try_from(next(7)).unwrap()])
                .collect();
            match next(8) {
                0 => anchors.add_toc(&bytes).unwrap(),
                1 => anchors
                    .open(&mut pager, &mut book, &bytes, next(2) == 0)
                    .unwrap(),
                2 => anchors.place_pending(&mut pager, &mut book).unwrap(),
                3 => put_line(&mut pager, &mut book),
                4 => {
                    notes.open(&bytes).unwrap();
                    let href: Vec<u8> = bytes.split(|&b| b == 0).next().unwrap().to_vec();
                    open = Some((Vec::new(), href));
                }
                5 => {
                    notes.text(&bytes);
                    if let Some((number, _)) = &mut open {
                        let start = bytes
                            .iter()
                            .position(|&b| !matches!(b, b' ' | b'\n' | b'['))
                            .unwrap_or(bytes.len());
                        let end = bytes
                            .iter()
                            .rposition(|&b| !matches!(b, b' ' | b'\n' | b']'))
                            .map_or(0, |e| e + 1);
                        if start < end {
                            number.extend_from_slice(&bytes[start..end]);
                        }
                        number.truncate(31);
                    }
                }
                6 => {
                    let word = i32::try_from(next(10)).unwrap();
                    notes.close(word).unwrap();
                    if let Some((number, href)) = &open {
                        let number: Vec<u8> = number.split(|&b| b == 0).next().unwrap().to_vec();
                        if !number.is_empty() && !href.is_empty() {
                            model.push((number, href.clone(), word));
                        }
                    }
                }
                _ => {
                    let words = i32::try_from(next(12)).unwrap();
                    let ready = model.iter().take_while(|m| m.2 <= words).count();
                    let expected: Vec<_> = model
                        .drain(..ready)
                        .map(|(n, h, _)| ([n, std::vec![0]].concat(), [h, std::vec![0]].concat()))
                        .collect();
                    assert_eq!(taken(&mut notes, words), expected);
                }
            }
            for i in 0..anchors.len() {
                assert!(anchors.get(i).is_some());
            }
        }
    }
}

#[test]
fn a_reference_needs_a_number_and_a_target() {
    let mut notes = Footnotes::new();
    notes.open(b"").unwrap();
    assert_eq!(notes.href(), b"\0");
    notes.text(b"1");
    notes.close(0).unwrap();
    notes.open(b"#x").unwrap();
    notes.text(b" [ ] ");
    notes.close(0).unwrap();
    assert!(!notes.has_pending());
}

#[test]
fn numbers_and_targets_are_cut_to_their_arrays() {
    let mut notes = Footnotes::new();
    let long = [b'h'; 400];
    notes.open(&long).unwrap();
    assert_eq!(notes.href().len(), HREF_LEN);
    notes.text(&[b'7'; 20]);
    notes.text(&[b'8'; 20]);
    notes.close(0).unwrap();
    let out = taken(&mut notes, 0);
    assert_eq!(out[0].0.len(), 32, "31 bytes and the NUL");
    assert_eq!(&out[0].0[..31], [&[b'7'; 20][..], &[b'8'; 11][..]].concat());
}
