//! The token stores' record layers replayed: for every step of testdata/token-stores-corpus.txt
//! (written by `test/token_stores/make_corpus.py`), what the file held (`read`) and what the store
//! wrote (`write`), as the driver worked them out. The C++ stores' decisions on them are checked by
//! `test/token_stores/check_shim.sh`.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, fuzz, hex};
use crate::records::{Shape, Store, read, write};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/token-stores-corpus.txt");

fn shape(store: &str) -> Shape<'static, &'static [u8]> {
    match store {
        "tokens" => Store::OpdsTokens,
        "pass" => Store::LcpPassphrases,
        _ => Store::LcpDevice,
    }
    .shape()
}

/// The driver's layer for `file`.
fn read_layer(file: &[u8], store: &str) -> String {
    let records = match read(file, &shape(store)) {
        Ok(records) => records,
        Err(e) => return format!("err {}", e.name()),
    };
    let mut out = format!("{}", records.len());
    for r in &records {
        out.push(' ');
        out.push_str(&r.iter().map(|f| hex(f)).collect::<Vec<_>>().join(","));
    }
    out
}

/// The store's text for the driver's save layer.
fn written(layer: &str, store: &str) -> String {
    let fields: Vec<Vec<u8>> = layer
        .split(' ')
        .skip(1)
        .flat_map(|r| r.split(',').map(bytes))
        .collect();
    String::from_utf8(write(&shape(store), &fields).unwrap()).unwrap()
}

#[test]
fn every_layer_of_the_corpus_is_the_stores() {
    let mut lines = CORPUS.lines();
    let (mut reads, mut saves) = (0, 0);
    while let Some(line) = lines.next() {
        let answer = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let (Some(at_read), Some(at_save)) = (answer.rfind(" | read "), answer.rfind(" | save "))
        else {
            continue; // a long step
        };
        let a: Vec<&str> = line.split(' ').collect();
        let (read_want, save_want) = (&answer[at_read + 8..at_save], &answer[at_save + 8..]);
        if a[1] == "load" {
            assert_eq!(read_layer(&bytes(a[2]), a[0]), read_want, "{line}");
            reads += 1;
        }
        if save_want != "-" {
            let json_at = answer.find(" | ").unwrap() + 3;
            assert_eq!(written(save_want, a[0]), answer[json_at..at_read], "{line}");
            saves += 1;
        }
    }
    assert!(reads > 450 && saves > 600, "{reads} reads, {saves} saves");
}

#[test]
fn broken_token_files_are_read_without_a_fault() {
    for store in ["tokens", "pass", "device"] {
        fuzz(CORPUS, 30, |input| {
            let _ = read(input, &shape(store));
        });
    }
}
