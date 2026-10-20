#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

extern crate std;

use core::mem::MaybeUninit;
use std::boxed::Box;
use std::string::String;
use std::vec::Vec;
use std::{format, vec};

use super::{Events, MAX_NESTING, Parser, TOKEN_BUF_SIZE, Token};

#[derive(Default)]
struct Recorder(Vec<String>);

impl Recorder {
    fn token(&mut self, kind: &str, token: Token<'_>) {
        assert_eq!(token.with_nul().last(), Some(&0));
        self.0
            .push(format!("{kind}:{}", String::from_utf8_lossy(token.bytes())));
    }
}

impl Events for Recorder {
    fn key(&mut self, key: Token<'_>) {
        self.token("key", key);
    }
    fn string(&mut self, value: Token<'_>) {
        self.token("str", value);
    }
    fn number(&mut self, value: Token<'_>) {
        self.token("num", value);
    }
    fn boolean(&mut self, value: bool) {
        self.0.push(format!("bool:{value}"));
    }
    fn null(&mut self) {
        self.0.push("null".into());
    }
    fn object_start(&mut self) {
        self.0.push("{".into());
    }
    fn object_end(&mut self) {
        self.0.push("}".into());
    }
    fn array_start(&mut self) {
        self.0.push("[".into());
    }
    fn array_end(&mut self) {
        self.0.push("]".into());
    }
}

fn parser() -> Box<Parser> {
    let mut slot = Box::new(MaybeUninit::<Parser>::uninit());
    Parser::init(&mut slot);
    // SAFETY: init set it up.
    unsafe { Box::from_raw(Box::into_raw(slot).cast::<Parser>()) }
}

/// Events for `json` fed in pieces of `chunk` bytes, and whether it stopped on an error.
fn read(json: &[u8], chunk: usize) -> (Vec<String>, bool) {
    let mut p = parser();
    let mut events = Recorder::default();
    for piece in json.chunks(chunk.max(1)) {
        p.feed(piece, &mut events);
    }
    (events.0, p.has_error())
}

fn events(json: &str) -> Vec<String> {
    let (events, error) = read(json.as_bytes(), json.len());
    assert!(!error, "{json}");
    events
}

#[test]
fn reads_an_object_in_order() {
    assert_eq!(
        events(r#"{"a": "x", "b": [1, -2.5e3, true, false, null], "c": {}}"#),
        [
            "{",
            "key:a",
            "str:x",
            "key:b",
            "[",
            "num:1",
            "num:-2.5e3",
            "bool:true",
            "bool:false",
            "null",
            "]",
            "key:c",
            "{",
            "}",
            "}"
        ]
    );
}

#[test]
fn strings_in_an_array_are_values() {
    assert_eq!(events(r#"["a", "b"]"#), ["[", "str:a", "str:b", "]"]);
}

#[test]
fn decodes_escapes() {
    assert_eq!(
        events(r#"{"k": "q\" b\\ s\/ \b\f\n\r\t \x"}"#),
        ["{", "key:k", "str:q\" b\\ s/ \u{8}\u{c}\n\r\t \\x", "}"]
    );
}

#[test]
fn decodes_unicode_escapes_and_surrogate_pairs() {
    assert_eq!(
        events(r#"["Aé€", "😀", "\ud83d!", "\ude00", "\ud83dA"]"#),
        [
            "[",
            "str:Aé€",
            "str:😀",
            "str:\u{fffd}!",
            "str:\u{fffd}",
            "str:\u{fffd}A",
            "]"
        ]
    );
}

#[test]
fn a_broken_unicode_escape_stops_the_document() {
    for json in [r#"["\u12G4"]"#, r#"["\u12"]"#] {
        let (events, error) = read(json.as_bytes(), json.len());
        assert!(error, "{json}");
        assert_eq!(events, ["["]);
    }
}

#[test]
fn a_broken_literal_stops_the_document() {
    let (events, error) = read(b"[tru, 1]", 64);
    assert!(error);
    assert_eq!(events, ["["]);
}

#[test]
fn a_number_ends_at_the_next_byte_only() {
    assert_eq!(events("[12"), ["["]);
    assert_eq!(events("[12]"), ["[", "num:12", "]"]);
}

#[test]
fn a_token_longer_than_the_buffer_is_dropped() {
    let fits = "a".repeat(TOKEN_BUF_SIZE - 1);
    let long = "a".repeat(TOKEN_BUF_SIZE);
    let json = format!(r#"["{fits}", "{long}", "next"]"#);
    assert_eq!(
        events(&json),
        [
            "[".into(),
            format!("str:{fits}"),
            "str:next".into(),
            "]".into()
        ]
    );
}

#[test]
fn nesting_deeper_than_the_limit_stops_the_document() {
    let ok = "[".repeat(MAX_NESTING) + &"]".repeat(MAX_NESTING);
    assert_eq!(events(&ok).len(), 2 * MAX_NESTING);
    let deep = "[".repeat(MAX_NESTING + 1);
    let (events, error) = read(deep.as_bytes(), deep.len());
    assert!(error);
    assert_eq!(events.len(), MAX_NESTING);
}

#[test]
fn reset_starts_a_new_document() {
    let mut p = parser();
    let mut events = Recorder::default();
    p.feed(br#"["\u12G"#, &mut events);
    assert!(p.has_error());
    p.reset();
    assert!(!p.has_error());
    p.feed(br#"{"a": 1}"#, &mut events);
    assert_eq!(events.0, ["[", "{", "key:a", "num:1", "}"]);
}

#[test]
fn skips_what_it_does_not_expect() {
    // A lenient scanner: stray bytes and a value without a key are skipped.
    assert_eq!(events(r#"{x "a" 1 : 2 ,}"#), ["{", "key:a", "num:2", "}"]);
}

const SAMPLE: &str = r#"{"metadata": {"title": "Kitâp \"1\"", "n": -1.5e+3},
  "links": [{"href": "https://a/b?c=d", "rel": "self"}, null, true, false, "😀"],
  "x": [[], {}, [1, 2, [3]]], "esc": "a\\b\/c\n"}"#;

#[test]
fn events_do_not_depend_on_how_the_input_is_split() {
    let whole = read(SAMPLE.as_bytes(), SAMPLE.len());
    for chunk in 1..16 {
        assert_eq!(read(SAMPLE.as_bytes(), chunk), whole, "chunk {chunk}");
    }
}

#[test]
fn truncated_and_corrupted_input_never_panics() {
    let bytes = SAMPLE.as_bytes();
    for end in 0..=bytes.len() {
        let _ = read(&bytes[..end], 7);
    }
    let mut seed = 0x9E37_79B9_u32;
    for i in 0..bytes.len() {
        for _ in 0..4 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut copy = vec![0; bytes.len()];
            copy.copy_from_slice(bytes);
            copy[i] = seed.to_le_bytes()[0];
            let _ = read(&copy, 1 + (seed as usize % 9));
        }
    }
}
