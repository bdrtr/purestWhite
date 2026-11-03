//! The SDK's own output as the oracle: testdata/hid-corpus.txt holds seeded scenarios run through
//! the SDK's decoder (`test/ble_hid_decoder/make_corpus.py`), each step's events after it. Every
//! case is replayed here through a fresh `Decoder`, and its events must be the same, in order.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use super::{Decoder, MapHints};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../../testdata/hid-corpus.txt");

fn bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// The events `decoder` has, as the driver prints them.
fn drain(decoder: &mut Decoder) -> Vec<String> {
    let mut lines = Vec::new();
    while let Some(k) = decoder.pop_key() {
        lines.push(format!(
            "  key {} {} {} {}",
            k.keycode, k.mods, k.special as u8, k.ch
        ));
    }
    while let Some(r) = decoder.pop_raw() {
        lines.push(format!(
            "  raw {:06x} {} {} {} {} {}",
            r.code,
            u8::from(r.pressed),
            r.keycode,
            r.mods,
            u8::from(r.was_rest),
            r.at_ms
        ));
    }
    lines
}

/// One case's replay: its first difference from the corpus, if any.
fn replay(case: &str) -> Result<(), String> {
    let lines: Vec<&str> = case.lines().collect();
    let mut hints = MapHints::default();
    let mut reports: Vec<u8> = Vec::new();
    let mut decoder: Option<Decoder> = None;
    let mut i = 0;
    while i < lines.len() {
        let words: Vec<&str> = lines[i].split(' ').collect();
        i += 1;
        match words[0] {
            "map" if words[1] != "-" => hints = MapHints::of(&bytes(words[1])),
            "report" => reports.push(words[1].parse().unwrap()),
            "frame" => {
                let decoder = decoder.get_or_insert_with(|| Decoder::new(hints));
                let index: usize = words[2].parse().unwrap();
                decoder.ingest(&bytes(words[3]), reports[index], words[1].parse().unwrap());
            }
            "poll" => decoder
                .get_or_insert_with(|| Decoder::new(hints))
                .poll(words[1].parse().unwrap()),
            "drain" => {
                let mut expected = Vec::new();
                while i < lines.len() && lines[i].starts_with("  ") {
                    expected.push(String::from(lines[i]));
                    i += 1;
                }
                let got = decoder.as_mut().map(drain).unwrap_or_default();
                if got != expected {
                    let at = lines
                        .get(i.saturating_sub(expected.len() + 2))
                        .unwrap_or(&"");
                    return Err(format!(
                        " after `{at}`:\n expected {expected:?}\n got      {got:?}"
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[test]
fn every_case_of_the_sdks_corpus_gives_the_same_events() {
    let cases: Vec<&str> = CORPUS.split("case ").filter(|c| !c.is_empty()).collect();
    assert!(cases.len() >= 250, "the corpus is there");
    let mut failed = Vec::new();
    for case in &cases {
        if let Err(diff) = replay(case) {
            failed.push(format!("case {}{diff}", case.lines().next().unwrap_or("")));
        }
    }
    assert!(
        failed.is_empty(),
        "{} of {} cases differ; first:\n{}",
        failed.len(),
        cases.len(),
        failed.first().map_or("", String::as_str)
    );
}
