//! `BookmarkFile`'s bookmark layers replayed: for every step of testdata/bookmark-file-corpus.txt
//! (written by `test/bookmark_file/make_corpus.py`), what the file held (`read`) and what save
//! wrote (`write`), as the driver worked them out. The C++ side's decisions on them are checked by
//! `test/bookmark_file/check_shim.sh`.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::bookmarks::{Bookmark, read, write};
use crate::corpus::{bytes, fuzz, hex};
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/bookmark-file-corpus.txt");

fn entry(b: &Bookmark) -> String {
    let offset = b.visible_text_offset.map_or("-".into(), |o| o.to_string());
    format!(
        "{},{},{},{:08x},{},{},{},{offset}",
        hex(&b.xpath),
        hex(&b.summary),
        hex(&b.name),
        b.percentage.to_bits(),
        b.spine_index,
        b.chapter_page_count,
        b.chapter_progress
    )
}

/// The driver's read layer for `file`.
fn read_layer(file: &[u8]) -> String {
    match read(file) {
        Ok(list) => {
            let entries: Vec<String> = list.iter().map(entry).collect();
            [list.len().to_string()]
                .into_iter()
                .chain(entries)
                .collect::<Vec<_>>()
                .join(" ")
        }
        Err(e) => format!("err {}", e.name()),
    }
}

fn bookmark_of(item: &str) -> Bookmark {
    let f: Vec<&str> = item.split(',').collect();
    Bookmark {
        xpath: bytes(f[0]),
        summary: bytes(f[1]),
        name: bytes(f[2]),
        percentage: f32::from_bits(u32::from_str_radix(f[3], 16).unwrap()),
        spine_index: f[4].parse().unwrap(),
        chapter_page_count: f[5].parse().unwrap(),
        chapter_progress: f[6].parse().unwrap(),
        visible_text_offset: (f[7] != "-").then(|| f[7].parse().unwrap()),
    }
}

/// The file's text for the driver's save layer.
fn written(layer: &str) -> String {
    let list: Vec<Bookmark> = layer.split(' ').skip(1).map(bookmark_of).collect();
    String::from_utf8(write(&list).unwrap()).unwrap()
}

#[test]
fn every_layer_of_the_corpus_is_the_files() {
    let mut lines = CORPUS.lines();
    let (mut reads, mut saves) = (0, 0);
    while let Some(line) = lines.next() {
        let answer = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let (Some(at_read), Some(at_save)) = (answer.rfind(" | read "), answer.rfind(" | save "))
        else {
            continue; // a long step
        };
        let (read_want, save_want) = (&answer[at_read + 8..at_save], &answer[at_save + 8..]);
        if let Some(file) = line.strip_prefix("load ") {
            assert_eq!(read_layer(&bytes(file)), read_want, "{line}");
            reads += 1;
        }
        if save_want != "-" {
            let json_at = answer.find(" | ").unwrap() + 3;
            assert_eq!(written(save_want), answer[json_at..at_read], "{line}");
            saves += 1;
        }
    }
    assert!(reads > 700 && saves > 700, "{reads} reads, {saves} saves");
}

#[test]
fn broken_bookmark_files_are_read_without_a_fault() {
    fuzz(CORPUS, 20, |input| {
        let _ = read(input);
    });
}
