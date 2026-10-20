#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::panic)]

extern crate std;

use std::borrow::ToOwned;
use std::string::String;
use std::vec::Vec;
use std::{format, vec};

use super::{Kind, MAX_LABEL, Sink, Toc, whole_characters};

#[derive(Default)]
struct Entries(Vec<String>);

impl Sink for Entries {
    fn entry(&mut self, label: &[u8], href: &[u8], anchor: &[u8], level: u8) {
        self.0.push(format!(
            "{level}|{}|{}|{}",
            String::from_utf8_lossy(label),
            String::from_utf8_lossy(href),
            String::from_utf8_lossy(anchor)
        ));
    }
}

/// The entries of `doc` fed in pieces of `chunk` bytes, and whether every piece was accepted.
fn read(kind: Kind, doc: &[u8], chunk: usize) -> (Vec<String>, bool) {
    let mut toc = Toc::new(kind, b"OEBPS/".to_vec(), 1 << 20);
    let mut entries = Entries::default();
    let pieces: Vec<&[u8]> = if doc.is_empty() {
        vec![&[][..]]
    } else {
        doc.chunks(chunk.max(1)).collect()
    };
    let mut ok = true;
    for (i, piece) in pieces.iter().enumerate() {
        ok &= toc.feed(piece, i + 1 == pieces.len(), &mut entries).is_ok();
    }
    (entries.0, ok)
}

const NCX: &str = r#"<?xml version="1.0"?><ncx><navMap>
<navPoint><navLabel><text>One &amp; <![CDATA[<1>]]></text></navLabel><content src="../Text/c%201.xhtml#s%202"/>
  <navPoint><navLabel><text>One A</text></navLabel><content src="c1.xhtml#a"/></navPoint>
</navPoint>
<navPoint><content src="none.xhtml"/></navPoint>
<navPoint><navLabel><text>Two</text></navLabel><content src="./c2.xhtml"/></navPoint>
</navMap></ncx>"#;

const NAV: &str = r#"<html xmlns:epub="x"><body>
<nav epub:type="landmarks"><ol><li><a href="l.xhtml">Landmark</a></li></ol></nav>
<nav epub:type="toc"><ol>
  <li><a href="c1.xhtml"><span>1.</span>&nbsp;Bir &mdash; &bogus;</a>
    <ol><li><a href="../c1.xhtml#x%20y">Sub</a></li></ol></li>
  <li><span>Heading</span></li>
  <li><a>No href</a></li>
</ol></nav></body></html>"#;

#[test]
fn reads_an_ncx() {
    let (entries, ok) = read(Kind::Ncx, NCX.as_bytes(), NCX.len());
    assert!(ok);
    assert_eq!(
        entries,
        [
            "1|One & <1>|Text/c 1.xhtml|s 2",
            "2|One A|OEBPS/c1.xhtml|a",
            "1|Two|OEBPS/./c2.xhtml|"
        ]
    );
}

#[test]
fn reads_a_nav_and_expands_html_entities() {
    let (entries, ok) = read(Kind::Nav, NAV.as_bytes(), NAV.len());
    assert!(ok);
    assert_eq!(
        entries,
        [
            "1|1.\u{a0}Bir \u{2014} &bogus;|OEBPS/c1.xhtml|",
            "2|Sub|c1.xhtml|x y"
        ]
    );
}

#[test]
fn an_undeclared_entity_stops_an_ncx() {
    let doc = NCX.replace("One A", "One&nbsp;A");
    let (entries, ok) = read(Kind::Ncx, doc.as_bytes(), doc.len());
    assert!(!ok);
    assert_eq!(entries, ["1|One & <1>|Text/c 1.xhtml|s 2"]);
}

#[test]
fn entries_do_not_depend_on_how_the_input_is_split() {
    for (kind, doc) in [(Kind::Ncx, NCX), (Kind::Nav, NAV)] {
        let whole = read(kind, doc.as_bytes(), doc.len());
        for chunk in 1..12 {
            assert_eq!(read(kind, doc.as_bytes(), chunk), whole, "{kind:?} {chunk}");
        }
    }
}

#[test]
fn truncated_and_corrupted_input_never_panics() {
    let mut seed = 0x2545_F491_u32;
    for (kind, doc) in [(Kind::Ncx, NCX.as_bytes()), (Kind::Nav, NAV.as_bytes())] {
        for end in 0..=doc.len() {
            let _ = read(kind, &doc[..end], 5);
        }
        for i in 0..doc.len() {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let mut copy = doc.to_vec();
            copy[i] = seed.to_le_bytes()[0];
            let _ = read(kind, &copy, 1 + (seed as usize % 7));
        }
    }
}

#[test]
fn a_label_keeps_at_most_max_label_bytes_on_a_whole_character() {
    for (fill, piece) in [("a", "b"), ("a", "é"), ("aa", "€"), ("a", "😀"), ("", "😀")] {
        let long = fill.to_owned() + &piece.repeat(MAX_LABEL);
        let ncx = format!(
            r#"<ncx><navMap><navPoint><navLabel><text>{long}</text></navLabel><content src="a.xhtml"/></navPoint></navMap></ncx>"#
        );
        let (entries, ok) = read(Kind::Ncx, ncx.as_bytes(), 97);
        assert!(ok);
        let label = entries[0].split('|').nth(1).unwrap().to_owned();
        assert!(
            label.len() <= MAX_LABEL && label.len() > MAX_LABEL - 4,
            "{fill}{piece}"
        );
        assert!(long.starts_with(&label), "{fill}{piece}");
    }
    // Entities count too, in a nav label.
    let nav = format!(
        r#"<html><body><nav epub:type="toc"><ol><li><a href="a.xhtml">{}</a></li></ol></nav></body></html>"#,
        "&mdash;".repeat(MAX_LABEL)
    );
    let (entries, _) = read(Kind::Nav, nav.as_bytes(), 64);
    assert_eq!(
        entries[0].split('|').nth(1).unwrap(),
        "\u{2014}".repeat(MAX_LABEL / 3)
    );
}

#[test]
fn whole_characters_drops_only_a_cut_sequence() {
    assert_eq!(whole_characters(b""), 0);
    assert_eq!(whole_characters(b"ab"), 2);
    assert_eq!(whole_characters("aé".as_bytes()), 3);
    assert_eq!(whole_characters(&"aé".as_bytes()[..2]), 1);
    assert_eq!(whole_characters(&"€".as_bytes()[..2]), 0);
    assert_eq!(whole_characters(&"a😀".as_bytes()[..4]), 1);
    assert_eq!(whole_characters("a😀".as_bytes()), 5);
    assert_eq!(whole_characters(b"a\xff"), 2);
}
