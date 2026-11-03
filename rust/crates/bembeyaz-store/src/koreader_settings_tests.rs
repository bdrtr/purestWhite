//! `KOReaderCredentialStore`'s decisions replayed: for every successful load of
//! testdata/koreader-credentials-corpus.txt (written by `test/koreader_credentials/make_corpus.py`)
//! that follows a step with settings, the settings [`load`] makes of the file, with the drivers'
//! stand-in decoder, are the C++ store's, and so is whether it saved the file again; for every step
//! but `long`, [`base_url`] of the store's server URL is its `getBaseUrl()`. Then the branches the
//! corpus reaches only by chance, by hand.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::Error;
use crate::corpus::{bytes, deobfuscate, hex};
use crate::koreader_settings::{
    LEGACY_DEFAULT_SERVER_URL, Settings, base_url, load, match_method, server_type, sync_behavior,
};
use crate::password::{Decoded, Secret};
use core::fmt::Write as _;
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/koreader-credentials-corpus.txt");

#[allow(clippy::unnecessary_wraps, reason = "a decoder's shape")]
fn stand_in(text: &[u8], most: usize) -> Result<Decoded, Error> {
    Ok(match deobfuscate(text) {
        None => Decoded::default(),
        Some(text) if text.len() > most => Decoded {
            too_long: true,
            ..Decoded::default()
        },
        Some(text) => Decoded {
            text: Secret::new(text),
            ok: true,
            too_long: false,
        },
    })
}

/// The driver's settings for `s`, the header slots past the file's taken from `before`, and its
/// base URL.
fn settings_of(s: &Settings, before: &[&str]) -> String {
    let mut out = format!(
        "{},{},{} {},{},{},{},{}",
        hex(&s.username),
        hex(&s.password),
        hex(&s.server_url),
        s.server_type,
        s.match_method,
        u8::from(s.send_metadata),
        u8::from(s.sync_clippings),
        s.sync_behavior
    );
    for slot in 0..2 {
        let _ = match s.headers.get(slot) {
            Some(h) => write!(out, " {},{}", hex(&h.name), hex(&h.value)),
            None => write!(out, " {}", before[2 + slot]),
        };
    }
    out + " " + &hex(&base_url(&s.server_url).unwrap())
}

/// What a check noted agrees with the setting it changed.
fn check_notes(s: &Settings, line: &str) {
    let notes = s.notes;
    if notes.pinned_legacy_url {
        assert_eq!(s.server_url, LEGACY_DEFAULT_SERVER_URL, "{line}");
    }
    let resets = [
        (
            notes.server_type_reset,
            s.server_type,
            server_type::KOSYNC,
            2,
        ),
        (
            notes.match_method_reset,
            s.match_method,
            match_method::FILENAME,
            1,
        ),
        (
            notes.sync_behavior_reset,
            s.sync_behavior,
            sync_behavior::ASK_EVERY_TIME,
            1,
        ),
    ];
    for (reset, value, to, most) in resets {
        if let Some(read) = reset {
            assert!(read > most && value == to, "{line}");
        }
    }
}

#[test]
fn every_load_settles_as_the_cpp_store_and_every_base_url_is_its() {
    let mut lines = CORPUS.lines();
    let mut before: Option<Vec<&str>> = None;
    let (mut loads, mut resaves, mut urls) = (0, 0, 0);
    while let Some(line) = lines.next() {
        let answer = lines.next().unwrap().strip_prefix("  = ").unwrap();
        if line.starts_with("long ") {
            before = None; // its answer has no settings
            continue;
        }
        let (status, rest) = answer.split_once(' ').unwrap();
        let (settings, rest) = rest.split_once(" | ").unwrap();
        let (written, rest) = rest.split_once(" | ").unwrap();
        let fields: Vec<&str> = settings.split(' ').collect();
        let url = bytes(fields[0].split(',').nth(2).unwrap());
        assert_eq!(hex(&base_url(&url).unwrap()), fields[4], "{line}");
        urls += 1;
        if let (Some(file), "ok", Some(before)) = (line.strip_prefix("load "), status, &before) {
            let settled = load(&bytes(file), stand_in).unwrap();
            assert_eq!(settings_of(&settled, before), settings, "{line}");
            assert_eq!(settled.resave, written != "-", "{line}");
            let read_layer: Vec<&str> = rest.strip_prefix("read ").unwrap().split(' ').collect();
            assert_eq!(
                format!("{}", settled.headers.len()),
                read_layer[7],
                "{line}"
            );
            check_notes(&settled, line);
            loads += 1;
            resaves += usize::from(settled.resave);
        }
        before = Some(fields);
    }
    assert!(
        loads > 600 && resaves > 300 && urls > 900,
        "{loads} loads, {resaves} resaves, {urls} URLs"
    );
}

#[test]
fn a_header_value_that_does_not_decode_is_empty() {
    let s = load(
        br#"{"cfgVersion":3,"serverType":1,"headers":[{"name":"X","value_obf":"plain"}]}"#,
        stand_in,
    )
    .unwrap();
    assert_eq!(
        (s.headers.len(), s.headers[0].name.as_slice()),
        (1, b"X".as_slice())
    );
    assert!(s.headers[0].value.is_empty());
}

#[test]
fn a_v1_config_pins_the_old_server_only_for_a_whole_account() {
    let user_only = load(br#"{"cfgVersion":1,"username":"u"}"#, stand_in).unwrap();
    assert!(user_only.server_url.is_empty() && !user_only.notes.pinned_legacy_url);
    assert_eq!(user_only.server_type, server_type::CROSSPOINT); // the default server
    assert!(user_only.resave);
    let account = load(
        br#"{"cfgVersion":1,"username":"u","password_obf":"~70"}"#,
        stand_in,
    )
    .unwrap();
    assert_eq!(account.server_url, LEGACY_DEFAULT_SERVER_URL);
    assert!(account.notes.pinned_legacy_url && account.resave);
    assert_eq!(account.server_type, server_type::KOSYNC);
}

#[test]
fn a_legacy_password_is_taken_and_saved_again() {
    let s = load(
        br#"{"cfgVersion":3,"serverType":1,"syncBehavior":1,"password_obf":"~zz","password":"p"}"#,
        stand_in,
    )
    .unwrap();
    assert_eq!(&*s.password, b"p");
    assert!(s.resave);
    let whole = br#"{"cfgVersion":3,"serverType":1,"syncBehavior":1,"password_obf":"~70"}"#;
    assert!(!load(whole, stand_in).unwrap().resave);
}

#[test]
fn a_v3_config_without_a_server_type_gets_one_from_its_url() {
    let default = load(
        br#"{"cfgVersion":3,"syncBehavior":1,"serverUrl":"https://sync.crosspointreader.com//"}"#,
        stand_in,
    )
    .unwrap();
    assert_eq!(default.server_type, server_type::CROSSPOINT);
    assert!(default.resave);
    let other = load(
        br#"{"cfgVersion":3,"syncBehavior":1,"serverUrl":"kosync.example"}"#,
        stand_in,
    )
    .unwrap();
    assert_eq!(other.server_type, server_type::KOSYNC);
}

#[test]
fn a_setting_out_of_range_is_reset_and_noted_with_the_value_read() {
    let s = load(
        br#"{"cfgVersion":3,"serverType":9,"matchMethod":5,"syncBehavior":7}"#,
        stand_in,
    )
    .unwrap();
    assert_eq!(
        (s.server_type, s.match_method, s.sync_behavior),
        (
            server_type::KOSYNC,
            match_method::FILENAME,
            sync_behavior::ASK_EVERY_TIME
        )
    );
    let n = s.notes;
    assert_eq!(
        (
            n.server_type_reset,
            n.match_method_reset,
            n.sync_behavior_reset
        ),
        (Some(9), Some(5), Some(7))
    );
    assert!(s.resave && !n.pinned_legacy_url);
}

#[test]
fn a_decoder_out_of_memory_is_an_error_not_an_empty_password() {
    let failing = |_: &[u8], _: usize| Err(Error::NoMemory);
    assert_eq!(
        load(br#"{"password_obf":"~70"}"#, failing),
        Err(Error::NoMemory)
    );
}

#[test]
fn a_base_url_is_the_default_or_gets_http_and_loses_trailing_slashes() {
    // As C++'s getBaseUrl, odd results included: "://" anywhere counts as a scheme.
    let cases: [(&[u8], &[u8]); 9] = [
        (b"", b"https://sync.crosspointreader.com"),
        (b"sync.example.org/", b"http://sync.example.org"),
        (b"https://h//", b"https://h"),
        (b"https://", b"https:"),
        (b"/", b"http:"),
        (b"///", b"http:"),
        (b"a\0://b/", b"a\0://b"),
        (b"host/x://y", b"host/x://y"),
        (b"x:/y", b"http://x:/y"),
    ];
    for (url, want) in cases {
        assert_eq!(base_url(url).unwrap(), want, "{url:?}");
    }
}
