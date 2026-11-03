//! ArduinoJson's own answers replayed: every `json` and `long` step of the golden corpus
//! (testdata/kosync-corpus.txt, written by `test/kosync_messages/make_corpus.py`) parsed and
//! written back as ArduinoJson parsed and wrote it.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::parse;
use core::fmt::Write as _;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/kosync-corpus.txt");

pub(crate) fn bytes(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::from("-");
    }
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// `ok <length of the JSON written>`, or `err <why>`.
fn round_trip(input: &[u8]) -> String {
    match parse(input) {
        Ok(value) => std::format!("ok {}", value.to_json().len()),
        Err(e) => std::format!("err {}", e.name()),
    }
}

#[test]
fn every_json_step_of_the_corpus_parses_and_writes_as_arduinojson() {
    let mut lines = CORPUS.lines();
    let (mut steps, mut failed) = (0, Vec::new());
    while let Some(line) = lines.next() {
        let answer = lines.next().unwrap().strip_prefix("  = ").unwrap();
        if let Some(n) = line.strip_prefix("long ") {
            let n: usize = n.parse().unwrap();
            let value = round_trip(&[b"[\"", "a".repeat(n).as_bytes(), b"\"]"].concat());
            let key = round_trip(&[b"{", "k".repeat(n).as_bytes(), b":1}"].concat());
            let open = round_trip(&[b"[\"", "a".repeat(n).as_bytes()].concat());
            let mut doc = crate::Value::Object(Vec::new());
            doc.set(b"p", crate::Value::string("a".repeat(n).as_bytes()));
            let set = doc.to_json().len();
            assert_eq!(
                std::format!("{value} {key} {open} set {set}"),
                answer,
                "{line}"
            );
            continue;
        }
        let Some(input) = line.strip_prefix("json ") else {
            continue;
        };
        steps += 1;
        let got = match parse(&bytes(input)) {
            Ok(value) => std::format!("ok {}", hex(&value.to_json())),
            Err(e) => std::format!("err {}", e.name()),
        };
        if got != answer {
            failed.push(std::format!(
                "{}\n  want {answer}\n  got  {got}",
                String::from_utf8_lossy(&bytes(input))
            ));
        }
    }
    assert!(steps > 1500, "{steps} steps");
    assert!(
        failed.is_empty(),
        "{} of {steps} differ:\n{}",
        failed.len(),
        failed[..failed.len().min(8)].join("\n")
    );
}

#[test]
fn broken_input_is_read_without_a_fault() {
    // Every 7th JSON step cut at each byte and with each byte changed: an answer, never a panic.
    let mut lines = CORPUS.lines();
    let mut seen = 0;
    while let Some(line) = lines.next() {
        lines.next();
        let Some(input) = line.strip_prefix("json ") else {
            continue;
        };
        let input = bytes(input);
        seen += 1;
        if seen % 7 != 0 {
            continue;
        }
        for len in 0..input.len() {
            let _ = parse(&input[..len]);
            let mut bad = input.clone();
            bad[len] ^= 0x5A;
            let _ = parse(&bad);
            if let Ok(value) = parse(&bad) {
                let _ = value.to_json();
            }
        }
    }
    assert!(seen > 1000);
}

#[test]
fn an_integer_is_a_long_long_within_its_range_and_a_float_is_not() {
    use crate::{Value, is_i64, is_number};
    assert!(is_i64(Some(&Value::U64(i64::MAX.cast_unsigned()))));
    assert!(
        !is_i64(Some(&Value::U64(1 << 63))),
        "past i64: ArduinoJson's canConvertNumber"
    );
    assert!(is_i64(Some(&Value::I32(-1))) && is_i64(Some(&Value::U32(7))));
    assert!(!is_i64(Some(&Value::F32(1.0))) && is_number(Some(&Value::F32(1.0))));
    assert!(!is_number(Some(&Value::Bool(true))) && !is_i64(None));
}

#[test]
fn a_string_far_past_the_limit_is_refused_once_it_ends() {
    let long = "a".repeat(1 << 20);
    let closed = [b"[\"", long.as_bytes(), b"\"]"].concat();
    assert_eq!(parse(&closed), Err(crate::Error::NoMemory));
    let open = [b"[\"", long.as_bytes()].concat();
    assert_eq!(parse(&open), Err(crate::Error::IncompleteInput));
    let escaped = [b"[\"", "\\u00e7".repeat(1 << 16).as_bytes(), b"\"]"].concat();
    assert_eq!(parse(&escaped), Err(crate::Error::NoMemory));
}

#[test]
fn the_writer_writes_what_a_document_writes() {
    use crate::{Value, Writer};
    let long = "x".repeat(65_536);
    let mut doc = Value::Object(Vec::new());
    doc.set(b"a\"b", Value::string(b"q\"\\\x08\x0c\n\r\t\0z"));
    doc.set(
        b"list",
        Value::Array(std::vec![
            Value::unsigned(u64::MAX),
            Value::signed(i64::MIN),
            Value::F32(-6.871_31e-34),
            Value::Bool(true),
            Value::Null,
            Value::Object(Vec::new()),
            Value::Array(Vec::new()),
            Value::string(long.as_bytes()),
        ]),
    );
    doc.set(b"last", Value::Bool(false));
    let mut w = Writer::with_capacity(16);
    w.object(|w| {
        w.key(b"a\"b");
        w.string(b"q\"\\\x08\x0c\n\r\t\0z");
        w.key(b"list");
        w.array(|w| {
            w.unsigned(u64::MAX);
            w.signed(i64::MIN);
            w.f32(-6.871_31e-34);
            w.bool(true);
            w.null();
            w.object(|_| {});
            w.array(|_| {});
            w.string(long.as_bytes());
        });
        w.key(b"last");
        w.bool(false);
    });
    assert_eq!(w.finish().unwrap(), doc.to_json());
}

#[test]
fn a_strings_written_length_is_counted_exactly() {
    let s = b"q\"\\\x08\x0c\n\r\t\0z\x01";
    let mut out = Vec::new();
    crate::write::string(s, &mut out);
    assert_eq!(crate::write::string_len(s), out.len());
}
