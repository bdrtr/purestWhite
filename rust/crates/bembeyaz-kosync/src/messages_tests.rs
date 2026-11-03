//! The C++ messages' answers replayed: every `user`, `body` and `parse` step of the golden corpus
//! (bembeyaz-jsondoc's testdata/kosync-corpus.txt, written by `test/kosync_messages/make_corpus.py`
//! from KOReaderSyncMessages.cpp and ArduinoJson).
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::messages::{
    Metadata, Position, Received, Update, create_user_body, parse_progress, progress_body,
};
use core::fmt::Write as _;
use std::string::{String, ToString};
use std::vec::Vec;

const CORPUS: &str = include_str!("../../bembeyaz-jsondoc/testdata/kosync-corpus.txt");

fn bytes(hex: &str) -> Vec<u8> {
    if hex == "-" {
        return Vec::new();
    }
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::from("-");
    }
    bytes.iter().fold(String::new(), |mut out, b| {
        let _ = write!(out, "{b:02x}");
        out
    })
}

fn float(bits: &str) -> f32 {
    f32::from_bits(u32::from_str_radix(bits, 16).unwrap())
}

/// The strings a `body` step names, kept for the borrows an `Update` makes.
struct Owned {
    words: Vec<Vec<u8>>,
}

fn body(a: &[&str]) -> String {
    let meta: Vec<&str> = if a[6] == "-" {
        Vec::new()
    } else {
        a[6].split(',').collect()
    };
    let pos: Vec<&str> = if a[7] == "-" {
        Vec::new()
    } else {
        a[7].split(',').collect()
    };
    let owned = Owned {
        words: [a[3], a[4]]
            .into_iter()
            .chain(meta.iter().take(7).copied())
            .chain(pos.get(5).copied())
            .map(bytes)
            .collect(),
    };
    let w = &owned.words;
    let metadata = (!meta.is_empty()).then(|| Metadata {
        filename: &w[2],
        title: &w[3],
        authors: &w[4],
        extra_json: &w[5],
        isbn: &w[6],
        asin: &w[7],
        series: &w[8],
        series_index: (meta[7] != "-").then(|| float(meta[7])),
    });
    let at = if meta.is_empty() { 2 } else { 9 };
    let position = (!pos.is_empty()).then(|| Position {
        pct_q: pos[0].parse().unwrap(),
        spine: pos[1].parse().unwrap(),
        page: pos[2].parse().unwrap(),
        pages: pos[3].parse().unwrap(),
        para: (pos[4] != "-").then(|| pos[4].parse().unwrap()),
        xpath: &w[at],
    });
    let update = Update {
        document: &w[0],
        progress: &w[1],
        percentage: float(a[5]),
        metadata,
        position,
    };
    hex(&progress_body(&update, a[1] == "1", a[2] == "1"))
}

fn parsed(a: &[&str]) -> String {
    let document = hex(&bytes(a[2]));
    match parse_progress(&bytes(a[3]), a[1] == "1") {
        Err(e) => std::format!("err {}", e.name()),
        Ok(Received {
            progress,
            percentage,
            device,
            device_id,
            timestamp,
            position,
        }) => {
            let pos = position.map_or(String::from("-"), |p| {
                let para = p.para.map_or(String::from("-"), |n| n.to_string());
                let xpath = hex(&p.xpath);
                let (pct_q, spine, page, pages) = (p.pct_q, p.spine, p.page, p.pages);
                std::format!("{pct_q},{spine},{page},{pages},{para},{xpath}")
            });
            std::format!(
                "ok {document} {} {:08x} {} {} {timestamp} {pos}",
                hex(&progress),
                percentage.to_bits(),
                hex(&device),
                hex(&device_id)
            )
        }
    }
}

#[test]
fn every_message_step_of_the_corpus_answers_as_the_c_code() {
    let mut lines = CORPUS.lines();
    let (mut steps, mut failed) = (0, Vec::new());
    while let Some(line) = lines.next() {
        let want = lines.next().unwrap().strip_prefix("  = ").unwrap();
        let a: Vec<&str> = line.split(' ').collect();
        let got = match a[0] {
            "user" => hex(&create_user_body(&bytes(a[1]), &bytes(a[2]))),
            "body" => body(&a),
            "parse" => parsed(&a),
            _ => continue,
        };
        steps += 1;
        if got != want {
            failed.push(std::format!("{line}\n  want {want}\n  got  {got}"));
        }
    }
    assert!(steps > 1800, "{steps} steps");
    let shown = failed.len().min(6);
    assert!(
        failed.is_empty(),
        "{} of {steps} differ:\n{}",
        failed.len(),
        failed[..shown].join("\n")
    );
}
