#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

extern crate std;

use super::{Resolver, Target};
use std::string::String;

const LIMIT: usize = 1 << 20;

fn run(doc: &str, target: Target, chunk: usize) -> (Option<String>, u64, bool) {
    let mut r = Resolver::new(target, 7, LIMIT);
    let mut ok = true;
    for piece in doc.as_bytes().chunks(chunk) {
        ok &= r.feed(piece).is_ok();
    }
    ok &= r.finish().is_ok();
    (
        r.xpath().map(|x| String::from_utf8(x.to_vec()).unwrap()),
        r.visible_codepoints(),
        ok,
    )
}

const NESTED: &str = r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title></head><body><div><section><p>Alpha bravo</p><p>Second <em>nested</em> tail</p></section></div><x:li xmlns:x="u">Item</x:li></body></html>"#;

#[test]
fn finds_paragraphs_offsets_and_counts() {
    assert_eq!(
        run(NESTED, Target::Paragraph(2), 4096).0.as_deref(),
        Some("/body/DocFragment[8]/body/div[1]/section[1]/p[2]")
    );
    assert_eq!(
        run(NESTED, Target::Paragraph(3), 4096).0.as_deref(),
        Some("/body/DocFragment[8]/body/li[1]")
    );
    assert_eq!(run(NESTED, Target::Paragraph(4), 4096).0, None);
    assert_eq!(
        run(
            NESTED,
            Target::Offset {
                target: 6,
                inclusive: false
            },
            4096
        )
        .0
        .as_deref(),
        Some("/body/DocFragment[8]/body/div[1]/section[1]/p[1]/text()[1].6")
    );
    // "Second " then <em>nested</em> then " tail": offset 26 is in the second run of p[2].
    assert_eq!(
        run(
            NESTED,
            Target::Offset {
                target: 26,
                inclusive: false
            },
            4096
        )
        .0
        .as_deref(),
        Some("/body/DocFragment[8]/body/div[1]/section[1]/p[2]/text()[2].2")
    );
    // 11 + 18 + 4 visible codepoints; the title does not count.
    assert_eq!(run(NESTED, Target::Count, 4096).1, 33);
}

#[test]
fn entities_comments_and_hidden_text() {
    let doc = "<html><body><p>a&nbsp;b<!--c-->d&unknown;e<rp>hidden</rp>f</p></body></html>";
    assert_eq!(run(doc, Target::Count, 4096).1, 15);
    assert_eq!(
        run(
            doc,
            Target::Offset {
                target: 3,
                inclusive: false
            },
            4096
        )
        .0
        .as_deref(),
        Some("/body/DocFragment[8]/body/p[1]/text()[2].0")
    );
}

#[test]
fn whitespace_pieces_never_hold_a_position() {
    let doc = "<html><body><p>ab</p>\n  <p>cd</p></body></html>";
    assert_eq!(
        run(
            doc,
            Target::Offset {
                target: 3,
                inclusive: false
            },
            4096
        )
        .0
        .as_deref(),
        Some("/body/DocFragment[8]/body/p[2]/text()[1].0")
    );
    assert_eq!(run(doc, Target::Count, 4096).1, 7);
    assert!(!run("<html><body><p>a</b></body></html>", Target::Count, 4096).2);
}

#[test]
fn corrupted_documents_never_panic() {
    let base = NESTED.as_bytes();
    let mut seed = 0x0BAD_5EED_u32;
    for i in 0..base.len() {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let mut copy = base.to_vec();
        copy[i] = seed.to_le_bytes()[0];
        for target in [
            Target::Paragraph(1 + seed % 4),
            Target::Offset {
                target: u64::from(seed % 40),
                inclusive: seed.is_multiple_of(2),
            },
            Target::Count,
        ] {
            let mut r = Resolver::new(target, 0, LIMIT);
            for piece in copy.chunks(1 + (seed as usize % 13)) {
                let _ = r.feed(piece);
            }
            let _ = r.finish();
            let _ = r.xpath();
        }
    }
}

#[test]
fn a_target_found_only_at_the_end_fails() {
    // The long attribute makes the parser hold the rest back until the end.
    let doc = std::format!(
        r#"<html><body><p title="{}"/><p>end</p></body></html>"#,
        "a".repeat(3000)
    );
    let (xpath, _, ok) = run(&doc, Target::Paragraph(2), 1024);
    assert!(!ok);
    assert_eq!(xpath.as_deref(), Some("/body/DocFragment[8]/body/p[2]"));
    let (_, count, ok) = run(&doc, Target::Count, 1024);
    assert!(ok);
    assert_eq!(count, 3);
}
