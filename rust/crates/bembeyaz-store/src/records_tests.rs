//! The token stores' record layers replayed: for every step of testdata/token-stores-corpus.txt
//! (written by `test/token_stores/make_corpus.py`), what the file held (`read`) and what the store
//! wrote (`write`), as the driver worked them out; and for every load, the records [`load`] keeps,
//! with the drivers' stand-in decoder, are the ones the C++ store saved after it. The C++ stores'
//! answers through the shims are checked by `test/token_stores/check_shim.sh`.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, deobfuscate, fuzz, hex};
use crate::password::{Decoded, Secret};
use crate::records::{Shape, Store, load, read, write};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/token-stores-corpus.txt");

fn store_of(store: &str) -> Store {
    match store {
        "tokens" => Store::OpdsTokens,
        "pass" => Store::LcpPassphrases,
        _ => Store::LcpDevice,
    }
}

fn shape(store: &str) -> Shape<'static, &'static [u8]> {
    store_of(store).shape()
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

/// The drivers' stand-in for the obfuscation, as a decoder: too long past `most`.
#[allow(clippy::unnecessary_wraps, reason = "a decoder's shape")]
fn stand_in(text: &[u8], most: usize) -> Result<Decoded, crate::Error> {
    Ok(match deobfuscate(text) {
        Some(plain) if plain.len() > most => Decoded {
            too_long: true,
            ..Decoded::default()
        },
        Some(plain) => Decoded {
            text: Secret::new(plain),
            ok: true,
            too_long: false,
        },
        None => Decoded::default(),
    })
}

/// The save layer of `records` kept, their secrets obfuscated again by the stand-in.
fn saved_layer(store: Store, records: &[Vec<Secret>]) -> String {
    let secrets: Vec<usize> = store.rule().secrets.iter().map(|s| s.field).collect();
    let mut out = format!("{}", records.len());
    for record in records {
        let fields: Vec<String> = record
            .iter()
            .enumerate()
            .map(|(i, f)| match (secrets.contains(&i), f.is_empty()) {
                (true, false) => hex(format!("~{}", hex(f)).as_bytes()),
                _ => hex(f),
            })
            .collect();
        out.push(' ');
        out.push_str(&fields.join(","));
    }
    out
}

#[test]
fn every_load_keeps_the_records_the_cpp_store_kept() {
    let mut lines = CORPUS.lines();
    let mut loads = 0;
    while let Some(line) = lines.next() {
        let answer = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let a: Vec<&str> = line.split(' ').collect();
        let (Some(at_save), true) = (answer.rfind(" | save "), a[1] == "load") else {
            continue;
        };
        if !answer.starts_with("ok,") {
            continue; // the store kept what it had
        }
        let store = store_of(a[0]);
        let kept = load(store, &bytes(a[2]), stand_in).unwrap();
        assert_eq!(saved_layer(store, &kept), answer[at_save + 8..], "{line}");
        loads += 1;
    }
    assert!(loads > 400, "{loads} loads");
}

/// The fields of a token entry and a passphrase, as the files hold them.
fn token(url: &str, access: &str) -> String {
    format!(r#"{{"url":"{url}","access_obf":"{access}"}}"#)
}

fn passphrase(provider: &str, key: &str) -> String {
    format!(r#"{{"provider":"{provider}","key_obf":"{key}"}}"#)
}

#[test]
fn the_limit_counts_only_the_records_kept_and_none_past_it_is_decoded() {
    let calls = core::cell::Cell::new(0);
    let counting = |text: &[u8], most: usize| {
        calls.set(calls.get() + 1);
        stand_in(text, most)
    };
    // Two tokens without a URL, then ten: eight kept, each with its two secrets decoded.
    let mut items = std::vec![token("", "~61"), token("", "~61")];
    items.extend((0..10).map(|i| token(&format!("u{i}"), "~61")));
    let file = format!(r#"{{"tokens":[{}]}}"#, items.join(","));
    let kept = load(Store::OpdsTokens, file.as_bytes(), counting).unwrap();
    assert_eq!((kept.len(), calls.get()), (8, 16));
    // Passphrases: no provider (not decoded), a hash that does not decode (decoded, skipped).
    calls.set(0);
    let mut items = std::vec![passphrase("", "~61"), passphrase("p", "plain")];
    items.extend((0..10).map(|i| passphrase(&format!("p{i}"), "~61")));
    let file = format!(r#"{{"passphrases":[{}]}}"#, items.join(","));
    let kept = load(Store::LcpPassphrases, file.as_bytes(), counting).unwrap();
    assert_eq!((kept.len(), calls.get()), (8, 9));
    assert_eq!(&*kept[0][0], b"p0");
}

#[test]
fn a_decoder_out_of_memory_on_any_store_is_an_error() {
    let files: [(Store, &[u8]); 3] = [
        (Store::OpdsTokens, br#"{"tokens":[{"url":"a","access_obf":"~61"},{"url":"b","access_obf":"~62"}]}"#),
        (Store::LcpPassphrases, br#"{"passphrases":[{"provider":"a","key_obf":"~61"},{"provider":"b","key_obf":"~62"}]}"#),
        (Store::LcpDevice, br#"{"device_id":"d","priv_obf":"~61"}"#),
    ];
    for (store, file) in files {
        // Each record decodes fine until the last call, so records kept before it do not count.
        let total = core::cell::Cell::new(0);
        let _ = load(store, file, |t: &[u8], m: usize| {
            total.set(total.get() + 1);
            stand_in(t, m)
        });
        let calls = core::cell::Cell::new(0);
        let failing_last = |text: &[u8], most: usize| {
            calls.set(calls.get() + 1);
            if calls.get() == total.get() {
                Err(crate::Error::NoMemory)
            } else {
                stand_in(text, most)
            }
        };
        assert_eq!(
            load(store, file, failing_last),
            Err(crate::Error::NoMemory),
            "{store:?}"
        );
    }
}

#[test]
fn a_load_that_fails_fails_as_the_cpp_store_did() {
    let mut lines = CORPUS.lines();
    let mut failed = 0;
    while let Some(line) = lines.next() {
        let answer = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let a: Vec<&str> = line.split(' ').collect();
        let (Some(at_read), true) = (answer.rfind(" | read "), a[1] == "load") else {
            continue;
        };
        let Some(error) = answer[at_read + 8..]
            .split(" | ")
            .next()
            .unwrap()
            .strip_prefix("err ")
        else {
            continue;
        };
        let got = load(store_of(a[0]), &bytes(a[2]), stand_in).unwrap_err();
        assert_eq!(got.name(), error, "{line}");
        failed += 1;
    }
    assert!(failed > 30, "{failed} failed loads");
}
