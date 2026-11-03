//! `WifiCredentialStore`'s stored-text layers replayed: for every step of
//! testdata/wifi-credentials-corpus.txt (written by `test/wifi_credentials/make_corpus.py`), what
//! the file held (`read`) and what the store wrote (`write`), as the driver worked them out. The
//! C++ store's decisions on them are checked by `test/wifi_credentials/check_shim.sh`.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, fuzz, hex};
use crate::field::Number;
use crate::wifi_credentials::{Saved, read, write};
use core::fmt::Write as _;
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/wifi-credentials-corpus.txt");

fn number(n: Number) -> String {
    format!("{}:{}", n.kind as u8, n.value)
}

/// The driver's read layer for `file`.
fn read_layer(file: &[u8]) -> String {
    let stored = match read(file) {
        Ok(stored) => stored,
        Err(e) => return format!("err {}", e.name()),
    };
    let mut out = format!(
        "{} {}",
        hex(&stored.last_connected_ssid),
        stored.credentials.len()
    );
    for c in &stored.credentials {
        let texts = [&c.ssid, &c.password_obf, &c.password].map(|t| hex(t));
        let _ = write!(
            out,
            " {},{},{}",
            texts.join(","),
            number(c.password_len),
            number(c.password_crc32)
        );
    }
    out
}

/// `wifi.json`'s text for the driver's save layer.
fn written(layer: &str) -> String {
    let mut items = layer.split(' ');
    let last = bytes(items.next().unwrap());
    let saved: Vec<Saved<Vec<u8>>> = items
        .map(|item| {
            let f: Vec<&str> = item.split(',').collect();
            Saved {
                ssid: bytes(f[0]),
                password_obf: bytes(f[1]),
                password_len: f[2].parse().unwrap(),
                password_crc32: f[3].parse().unwrap(),
            }
        })
        .collect();
    String::from_utf8(write(&last, &saved).unwrap()).unwrap()
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
    assert!(reads > 700 && saves > 200, "{reads} reads, {saves} saves");
}

#[test]
fn broken_wifi_files_are_read_without_a_fault() {
    fuzz(CORPUS, 20, |input| {
        let _ = read(input);
    });
}
