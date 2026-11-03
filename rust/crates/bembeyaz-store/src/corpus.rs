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
