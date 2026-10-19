use alloc::vec::Vec;
use core::fmt::Write;
use std::string::String;

use crate::{Package, Sink};

#[derive(Default)]
struct Recorder {
    spine: Vec<Vec<u8>>,
    css: Vec<Vec<u8>>,
}

impl Sink for Recorder {
    fn spine_item(&mut self, href: &[u8]) {
        self.spine.push(href.to_vec());
    }
    fn stylesheet(&mut self, href: &[u8]) {
        self.css.push(href.to_vec());
    }
    fn parse_float(&mut self, text: &[u8]) -> Option<f32> {
        core::str::from_utf8(text).ok()?.parse().ok()
    }
}

fn read(doc: &str, spine: bool, metadata_only: bool) -> (Package, Recorder) {
    let mut package = Package::new(b"OPS/".to_vec(), spine, metadata_only, 1 << 20, 1 << 20);
    let mut recorder = Recorder::default();
    let _ = package.feed(doc.as_bytes(), true, &mut recorder);
    (package, recorder)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn reads_metadata_and_resolves_the_spine() {
    let (package, recorder) = read(
        r##"<package><metadata xmlns:dc="urn:dc">
        <dc:title> The
          Left Hand </dc:title><dc:title>Subtitle</dc:title>
        <dc:creator>Ursula</dc:creator><dc:creator> K. Le Guin</dc:creator>
        <dc:identifier opf:scheme="ISBN">urn:isbn: 978</dc:identifier>
        <meta name="cover" content="c"/>
        <meta property="belongs-to-collection" id="s">Hainish</meta>
        <meta refines="#s" property="collection-type">series</meta>
        <meta refines="#s" property="group-position">4</meta>
        </metadata><manifest>
        <item id="c" href="img/c%20.jpg" media-type="image/jpeg"/>
        <item id="a" href="../a.xhtml" media-type="application/xhtml+xml"/>
        <item id="a" href="later.xhtml"/>
        <item id="n" href="nav.xhtml" properties="scripted nav"/>
        <item id="s" href="s.css" media-type="text/css"/>
        </manifest><spine><itemref idref="a"/><itemref idref="none"/><itemref idref="n"/></spine>
        <guide><reference type="start" href="a.xhtml#x"/></guide></package>"##,
        true,
        false,
    );
    let fields = package.fields();
    assert_eq!(text(&fields.title), "The Left Hand");
    assert_eq!(text(&fields.author), "Ursula, K. Le Guin");
    assert_eq!(text(&fields.isbn), "978");
    assert_eq!(text(&fields.series), "Hainish");
    assert_eq!(fields.series_index, Some(4.0));
    assert_eq!(text(&fields.cover), "OPS/img/c .jpg");
    assert_eq!(text(&fields.toc_nav), "OPS/nav.xhtml");
    assert_eq!(text(&fields.text_reference), "OPS/a.xhtml#x");
    assert_eq!(
        recorder.spine,
        [b"a.xhtml".to_vec(), b"OPS/nav.xhtml".to_vec()]
    );
    assert_eq!(recorder.css, [b"OPS/s.css".to_vec()]);
}

#[test]
fn a_metadata_only_read_stops_at_the_manifest() {
    let mut package = Package::new(Vec::new(), false, true, 1 << 20, 1 << 20);
    let mut recorder = Recorder::default();
    let doc = b"<package><metadata><title>T</title></metadata><manifest/></package>";
    assert_eq!(
        package.feed(doc, true, &mut recorder),
        Err(bembeyaz_xml::Error::Aborted)
    );
    assert_eq!(
        package.feed(b"", true, &mut recorder),
        Err(bembeyaz_xml::Error::Aborted)
    );
    assert_eq!(package.fields().title, b"T");
}

#[test]
fn a_manifest_past_the_limit_fails() {
    let mut doc = String::from("<package><manifest>");
    for i in 0..200 {
        let _ = write!(doc, "<item id=\"i{i}\" href=\"chapter-{i}.xhtml\"/>");
    }
    doc += "</manifest></package>";
    let mut package = Package::new(Vec::new(), true, false, 1 << 20, 4096);
    assert_eq!(
        package.feed(doc.as_bytes(), true, &mut Recorder::default()),
        Err(bembeyaz_xml::Error::NoMemory)
    );
}

#[test]
fn survives_truncations_and_byte_changes() {
    let doc: &[u8] = br##"<?xml version="1.0"?><package><metadata xmlns:dc="urn:dc"><dc:title>T &amp; U</dc:title><dc:creator>A</dc:creator><dc:identifier opf:scheme="ISBN">isbn:1</dc:identifier><meta name="calibre:series_index" content="2"/><meta property="belongs-to-collection" id="s">S</meta><meta refines="#s" property="group-position">1</meta></metadata><manifest><item id="a" href="a%20b/../c.xhtml" media-type="text/css" properties="nav cover-image"/></manifest><spine><itemref idref="a"/></spine><guide><reference type="start" href="a"/></guide></package>"##;
    let mut cases: Vec<Vec<u8>> = (0..doc.len())
        .map(|n| doc.get(..n).unwrap_or_default().to_vec())
        .collect();
    for at in 0..doc.len() {
        for b in [b'<', b'>', b'"', b'/', b':', b'#', b'%', 0, 0xFF] {
            let mut case = doc.to_vec();
            if let Some(slot) = case.get_mut(at) {
                *slot = b;
            }
            cases.push(case);
        }
    }
    for case in &cases {
        for (spine, metadata_only) in [(true, false), (false, true)] {
            let mut package =
                Package::new(b"OPS/".to_vec(), spine, metadata_only, 1 << 16, 1 << 16);
            for piece in case.chunks(7) {
                let _ = package.feed(piece, false, &mut Recorder::default());
            }
            let _ = package.feed(&[], true, &mut Recorder::default());
        }
    }
}
