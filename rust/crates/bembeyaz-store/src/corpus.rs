//! What the stores' corpus tests share: the hex of the transcripts, the replay of every step and
//! the truncated and corrupted reads.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use core::fmt::Write as _;
use std::format;
use std::string::String;
use std::vec::Vec;

/// `hex`'s bytes (`-` is none).
pub(crate) fn bytes(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

/// `bytes` in hex (`-` for none).
pub(crate) fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "-".into();
    }
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

/// Every step of `corpus` (a step line, then `  = <answer>`) answered by `step` as the C++ did;
/// more than `least` of them.
pub(crate) fn replay(corpus: &str, least: usize, mut step: impl FnMut(&[&str]) -> String) {
    let mut lines = corpus.lines();
    let (mut steps, mut failed) = (0, Vec::new());
    while let Some(line) = lines.next() {
        let want = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let a: Vec<&str> = line.split(' ').collect();
        let got = step(&a);
        steps += 1;
        if got != want {
            failed.push(format!("{line}\n  want {want}\n  got  {got}"));
        }
    }
    assert!(steps > least, "{steps} steps");
    assert!(
        failed.is_empty(),
        "{} of {steps} differ:\n{}",
        failed.len(),
        failed[..failed.len().min(4)].join("\n")
    );
}

/// Every `every`th file of `corpus` (the last word of a `from` or `load` line, or of a
/// `<store> load` one), cut at each byte and with each byte changed, read by `read` without a
/// fault.
pub(crate) fn fuzz(corpus: &str, every: usize, mut read: impl FnMut(&[u8])) {
    for line in corpus
        .lines()
        .filter(|l| l.starts_with("from ") || l.starts_with("load ") || l.contains(" load "))
        .step_by(every)
    {
        let input = bytes(line.split(' ').next_back().unwrap());
        for len in 0..input.len() {
            read(&input[..len]);
            let mut bad = input.clone();
            bad[len] ^= 0x5A;
            read(&bad);
        }
    }
}

/// The drivers' stand-in for the device's obfuscation (`test/json_oracle`'s `ObfuscationFake.cpp`):
/// `~` and the bytes in hex, each pair read as its `strtol(pair, &end, 16)` reads it (a space
/// before the digit, or a sign, taken too); anything else does not decode.
pub(crate) fn deobfuscate(text: &[u8]) -> Option<Vec<u8>> {
    let digits = text.strip_prefix(b"~")?;
    if digits.len() % 2 != 0 {
        return None;
    }
    digits
        .chunks(2)
        .map(|pair| strtol_pair(pair[0], pair[1]))
        .collect()
}

/// A two-character `strtol(..., 16)` that must read both characters, cast to a byte.
fn strtol_pair(first: u8, second: u8) -> Option<u8> {
    let hex = |c: u8| {
        char::from(c)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let space = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r');
    match (first, hex(first), hex(second)) {
        (_, Some(high), Some(low)) => Some(high * 16 + low),
        (c, None, Some(low)) if space(c) || c == b'+' => Some(low),
        (b'-', None, Some(low)) => Some(0u8.wrapping_sub(low)),
        _ => None,
    }
}

/// Every step of `corpus` (a step line, the log lines before its answer, each `  ! ` and the
/// line in hex, then `  = <answer>`): the line, its log lines (decoded), and its answer.
pub(crate) fn steps(corpus: &str) -> Vec<(&str, Vec<Vec<u8>>, &str)> {
    let mut lines = corpus.lines();
    let mut out = Vec::new();
    while let Some(line) = lines.next() {
        let mut logs = Vec::new();
        let answer = loop {
            let next = lines.next().unwrap();
            match next.strip_prefix("  ! ") {
                Some(log) => logs.push(bytes(log)),
                None => break next.strip_prefix("  = ").unwrap(),
            }
        };
        out.push((line, logs, answer));
    }
    out
}

#[test]
fn the_stand_in_reads_each_pair_as_strtol_does() {
    let cases: [(&[u8], Option<&[u8]>); 7] = [
        (b"~6162", Some(b"ab")),
        (b"~ 1\t2", Some(b"\x01\x02")),
        (b"~+f", Some(b"\x0f")),
        (b"~-1", Some(b"\xff")),
        (b"~0x", None),
        (b"~1 ", None),
        (b"~616", None),
    ];
    for (text, want) in cases {
        assert_eq!(deobfuscate(text).as_deref(), want, "{text:?}");
    }
}
