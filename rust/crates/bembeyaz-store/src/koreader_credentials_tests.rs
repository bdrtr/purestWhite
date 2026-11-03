//! `KOReaderCredentialStore`'s stored-text layers replayed: for every step of
//! testdata/koreader-credentials-corpus.txt (written by
//! `test/koreader_credentials/make_corpus.py`), what the file held (`read`) and what the store
//! wrote (`write`), as the driver worked them out. The C++ store's decisions on them are checked by
//! `test/koreader_credentials/check_shim.sh`.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, fuzz, hex};
use crate::field::Number;
use crate::headers::{HEADERS, Header, Headers};
use crate::koreader_credentials::{Saved, read, write};
use core::fmt::Write as _;
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/koreader-credentials-corpus.txt");

fn number(n: Number) -> String {
    format!("{}:{}:{}", n.kind as u8, n.value, n.byte)
}

/// The driver's read layer for `file`.
fn read_layer(file: &[u8]) -> String {
    let s = match read(file) {
        Ok(s) => s,
        Err(e) => return format!("err {}", e.name()),
    };
    let texts = [&s.username, &s.password_obf, &s.password, &s.server_url].map(|t| hex(t));
    let mut out = texts.join(",");
    for n in [
        s.cfg_version,
        s.server_type,
        s.match_method,
        s.sync_behavior,
    ] {
        let _ = write!(out, " {}", number(n));
    }
    let flags = [s.send_metadata, s.sync_clippings].map(u8::from);
    let _ = write!(out, " {} {} {}", flags[0], flags[1], s.headers.count);
    for h in &s.headers.slots[..s.headers.count] {
        let _ = write!(out, " {},{}", hex(&h.name), hex(&h.value_obf));
    }
    out
}

/// `koreader.json`'s text for the driver's save layer.
fn written(layer: &str) -> String {
    let parts: Vec<&str> = layer.split(' ').collect();
    let texts: Vec<Vec<u8>> = parts[0].split(',').map(bytes).collect();
    let n: Vec<u8> = parts[1].split(',').map(|v| v.parse().unwrap()).collect();
    let mut slots: [Header; HEADERS] = Default::default();
    for (slot, part) in slots.iter_mut().zip(&parts[2..]) {
        let f: Vec<&str> = part.split(',').collect();
        *slot = Header {
            name: bytes(f[0]),
            value_obf: bytes(f[1]),
        };
    }
    let saved = Saved {
        cfg_version: n[0],
        username: texts[0].clone(),
        password_obf: texts[1].clone(),
        server_url: texts[2].clone(),
        server_type: n[1],
        match_method: n[2],
        send_metadata: n[3] == 1,
        sync_clippings: n[4] == 1,
        sync_behavior: n[5],
        headers: Headers {
            slots,
            count: HEADERS,
        },
    };
    String::from_utf8(write(&saved).unwrap()).unwrap()
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
        let read_want = &answer[at_read + 8..at_save];
        let save_want = &answer[at_save + 8..];
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
    assert!(reads > 700 && saves > 300, "{reads} reads, {saves} saves");
}

#[test]
fn broken_koreader_files_are_read_without_a_fault() {
    fuzz(CORPUS, 20, |input| {
        let _ = read(input);
    });
}
