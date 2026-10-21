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
fn a_target_found_only_at_the_end_is_found() {
    // The long attribute makes the parser hold the rest back until the end.
    let doc = std::format!(
        r#"<html><body><p title="{}"/><p>end</p></body></html>"#,
        "a".repeat(3000)
    );
    let (xpath, _, ok) = run(&doc, Target::Paragraph(2), 1024);
    assert!(ok);
    assert_eq!(xpath.as_deref(), Some("/body/DocFragment[8]/body/p[2]"));
    let (_, count, ok) = run(&doc, Target::Count, 1024);
    assert!(ok);
    assert_eq!(count, 3);
}

mod locate {
    #![allow(clippy::indexing_slicing)]

    use crate::locate::{Locator, Mode, XPath};

    #[allow(clippy::large_types_passed_by_value)]
    fn locate(doc: &str, mode: Mode) -> Locator {
        let mut l = Locator::new(mode);
        l.feed(doc.as_bytes());
        l
    }

    const DOC: &str = "<html><head><title>T</title></head><body><div><p>Alpha bravo</p><ul><li id=\"x\">One</li><li><a id='y'/>Two &amp; three</li></ul></div></body></html>";

    #[test]
    fn reads_paths() {
        let x = XPath::parse(b"/body/DocFragment[3]/body/div[1]/ul/li[2]/text()[1].4");
        assert_eq!(
            (
                x.doc_fragment,
                x.paragraph,
                x.char_offset,
                x.text_node,
                x.step_count
            ),
            (3, -1, 4, 1, 3)
        );
        assert_eq!(&x.steps[2].tag[..3], b"li\0");
        assert_eq!((x.steps[1].index, x.steps[2].index), (0, 2));
        assert!(!x.body_text && !x.chapter_start);
        let start = XPath::parse(b"/body/DocFragment[2]/body/div/text().0");
        assert!(start.chapter_start);
        let body = XPath::parse(b"/body/DocFragment[2]/body/text()[2].7");
        assert!(body.body_text);
        assert_eq!(
            (body.step_count, body.text_node, body.char_offset),
            (0, 2, 7)
        );
        assert_eq!(
            XPath::parse(b"/body/DocFragment[2]/body/p[12].3").paragraph,
            12
        );
        assert_eq!(XPath::parse(b"garbage").doc_fragment, -1);
    }

    #[test]
    fn finds_steps_paragraphs_and_anchors() {
        // The self-closing <a/> ends text node 1 of li[2], so its text is text()[2].
        let x = XPath::parse(b"/body/DocFragment[1]/body/div[1]/ul/li[2]/text()[2].4");
        let l = locate(
            DOC,
            Mode::Steps {
                steps: x.steps,
                count: x.step_count,
                char_offset: x.char_offset,
                text_node: x.text_node,
                relax: false,
            },
        );
        // "Alpha bravo" (11) + "One" (3) + 4 into "Two & three".
        assert!(l.found());
        assert_eq!((l.target_visible(), l.total_visible()), (18, 25));
        assert_eq!(l.anchor(), Some(&b"y"[..]));
        assert_eq!((l.paragraph_at_match(), l.list_item_at_match()), (1, 2));
        let p = locate(
            DOC,
            Mode::Paragraph {
                paragraph: 1,
                char_offset: 6,
                text_node: 1,
            },
        );
        assert!(p.found());
        assert_eq!(p.target_visible(), 6);
        let f = locate(DOC, Mode::Forward { target: 60 });
        assert_eq!(f.paragraph_count(), 1);
    }

    #[test]
    fn corrupted_documents_and_paths_never_panic() {
        let mut seed = 0x5EED_1234_u32;
        let paths: [&[u8]; 4] = [
            b"/body/DocFragment[1]/body/div[1]/ul/li[2]/text()[1].4",
            b"/body/DocFragment[1]/body/text()[3].99999999999",
            b"/body/DocFragment[1]/body/a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r.1",
            b"/body/DocFragment[]/body/.",
        ];
        for i in 0..DOC.len() {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut doc = DOC.as_bytes().to_vec();
            doc[i] = seed.to_le_bytes()[0];
            let mut path = paths[seed as usize % paths.len()].to_vec();
            let at = seed as usize % path.len();
            path[at] = seed.to_le_bytes()[1];
            let x = XPath::parse(&path);
            for mode in [
                Mode::Forward {
                    target: u64::from(seed % 300),
                },
                Mode::Paragraph {
                    paragraph: x.paragraph,
                    char_offset: x.char_offset,
                    text_node: x.text_node,
                },
                Mode::Steps {
                    steps: x.steps,
                    count: x.step_count,
                    char_offset: x.char_offset,
                    text_node: x.text_node,
                    relax: seed.is_multiple_of(2),
                },
                Mode::BodyText {
                    char_offset: x.char_offset,
                    text_node: x.text_node,
                },
            ] {
                let mut l = Locator::new(mode);
                l.feed(&doc);
                let _ = (
                    l.found(),
                    l.anchor(),
                    l.target_visible(),
                    l.paragraph_count(),
                );
            }
        }
    }
}

/// The two halves agree: a path the resolver writes for a visible codepoint is found at that
/// codepoint by the locator, on ordinary chapters.
#[test]
fn written_paths_are_found_again() {
    use crate::locate::{Locator, Mode, XPath};
    const CHAPTERS: [&str; 4] = [
        "<html><body><div><section><p>Alpha bravo</p><p>Second <em>nested</em> tail</p></section></div></body></html>",
        "<html><body><ul><li>One</li><li>Two <b>bold</b> three</li></ul><p>Last line.</p></body></html>",
        "<html><head><title>T</title></head><body><div><div><p>Deep \u{e7}i\u{e7}ek &amp; more</p></div></div></body></html>",
        "<html><body><h1>Title</h1><p>a</p><p>bb</p><p>ccc <i>dd</i> e</p></body></html>",
    ];
    for doc in CHAPTERS {
        let total = run(doc, Target::Count, 4096).1;
        for target in 0..total {
            let (xpath, _, ok) = run(
                doc,
                Target::Offset {
                    target,
                    inclusive: false,
                },
                4096,
            );
            assert!(ok);
            let xpath = xpath.unwrap();
            let x = XPath::parse(xpath.as_bytes());
            assert!(x.step_count > 0, "{xpath}");
            let mut l = Locator::new(Mode::Steps {
                steps: x.steps,
                count: x.step_count,
                char_offset: x.char_offset,
                text_node: x.text_node,
                relax: false,
            });
            l.feed(doc.as_bytes());
            assert!(l.found(), "{doc} {xpath}");
            assert_eq!(l.target_visible(), target, "{doc} {xpath}");
        }
    }
}
