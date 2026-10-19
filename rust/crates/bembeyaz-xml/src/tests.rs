use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use crate::{Error, Flow, Handler, Parser, StartTag, Str};

/// Events as `test/xml_parser_utils/XmlCorpusTest.cpp` writes them.
#[derive(Default)]
struct Recorder {
    out: String,
    text: Vec<u8>,
}

impl Recorder {
    fn escape(&mut self, s: &[u8]) {
        for &b in s {
            if !(0x20..0x7F).contains(&b) || b == b'\\' {
                let _ = write!(self.out, "\\{b:02X}");
            } else {
                self.out.push(char::from(b));
            }
        }
    }
    fn flush(&mut self) {
        if !self.text.is_empty() {
            let text = core::mem::take(&mut self.text);
            self.out.push('T');
            self.escape(&text);
            self.out.push('\n');
        }
    }
    fn line(&mut self, tag: char, body: &[u8]) {
        self.flush();
        self.out.push(tag);
        self.escape(body);
        self.out.push('\n');
    }
}

impl Handler for Recorder {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        self.line('S', tag.name().bytes());
        for (name, value) in tag.attributes().iter() {
            self.out.push('A');
            self.escape(name.bytes());
            self.out.push('=');
            self.escape(value.bytes());
            self.out.push('\n');
        }
        Flow::Continue(())
    }
    fn end_element(&mut self, name: Str<'_>) -> Flow {
        self.line('E', name.bytes());
        Flow::Continue(())
    }
    fn text(&mut self, text: &[u8]) -> Flow {
        self.text.extend_from_slice(text);
        Flow::Continue(())
    }
    fn comment(&mut self, text: Str<'_>) -> Flow {
        self.line('C', text.bytes());
        Flow::Continue(())
    }
    fn processing_instruction(&mut self, target: Str<'_>, data: Str<'_>) -> Flow {
        let mut body = target.bytes().to_vec();
        body.push(b' ');
        body.extend_from_slice(data.bytes());
        self.line('P', &body);
        Flow::Continue(())
    }
    fn cdata_start(&mut self) -> Flow {
        self.line('[', b"");
        Flow::Continue(())
    }
    fn cdata_end(&mut self) -> Flow {
        self.line(']', b"");
        Flow::Continue(())
    }
    fn skipped_entity(&mut self, name: Str<'_>) -> Flow {
        self.line('K', name.bytes());
        Flow::Continue(())
    }
}

fn report(doc: &[u8], chunk: usize) -> String {
    let mut rec = Recorder::default();
    let mut parser = Parser::new(1 << 20);
    let step = if chunk == 0 { doc.len().max(1) } else { chunk };
    let mut at = 0;
    let result = loop {
        let end = (at + step).min(doc.len());
        let result = parser.feed(
            doc.get(at..end).unwrap_or_default(),
            end == doc.len(),
            &mut rec,
        );
        at = end;
        if result.is_err() || at == doc.len() {
            break result;
        }
    };
    rec.flush();
    rec.out
        .push_str(if result.is_ok() { "ok\n" } else { "error\n" });
    rec.out
}

#[test]
fn reports_a_small_document() {
    let doc = b"<?xml version='1.0'?><!DOCTYPE a [<!ENTITY e 'v'><!ATTLIST a d CDATA 'x'>]><a k='1 &amp; 2'>t&e;<b/><![CDATA[c]]><!--n--><?p d?></a>";
    assert_eq!(
        report(doc, 0),
        "Sa\nAk=1 & 2\nAd=x\nTt&e;\nSb\nEb\n[\nTc\n]\nCn\nPp d\nEa\nok\n"
    );
    for chunk in 1..8 {
        assert_eq!(report(doc, chunk), report(doc, 0));
    }
}

#[test]
fn fails_where_expat_fails() {
    assert!(report(b"<a>", 0).ends_with("error\n"));
    assert!(report(b"<a></b>", 0).ends_with("error\n"));
    assert!(report(b"", 0).ends_with("error\n"));
    assert_eq!(Error::NoElements.code(), 3);
}

/// Documents covering each part of the parser, for the tests that cut and mutate input.
const SAMPLES: &[&[u8]] = &[
    b"<?xml version='1.0' encoding='UTF-8' standalone='no'?>\n<!DOCTYPE html PUBLIC '-//W3C//DTD XHTML 1.1//EN' 'x.dtd'>\n<html><body><p class='c' id=\"i\">a&nbsp;b &amp; &#x2014; \xc3\xa9</p><br/></body></html>\n",
    b"<!DOCTYPE a [<!ENTITY e 'v'><!ATTLIST a t NMTOKENS ' x  y ' c CDATA #FIXED 'f'><!ELEMENT a (#PCDATA|b)*><!NOTATION n SYSTEM 's'><!-- c --><?p d?>]><a t=' p  q '>&e;<![CDATA[<&>]]]]><?q?><!--x--></a><!--after-->",
    b"<a>line\r\nline\rline\n]] ] &#9;&#10;&#13;</a>\r\n",
    b"\xef\xbb\xbf<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><r v='\xe9'>caf\xe9</r>",
    b"\xff\xfe<\0a\0 \0x\0=\0'\0\xe9\0'\0>\0=\xd8\0\xde<\0/\0a\0>\0",
    b"\xfe\xff\0<\0a\0/\0>",
    b"<a x='1'y='2'/>",
    b"<a><b></a>",
];

fn feed_all(
    parser: &mut Parser,
    doc: &[u8],
    cuts: &[usize],
    handler: &mut impl Handler,
) -> Result<(), Error> {
    let mut at = 0;
    for &cut in cuts.iter().chain(core::iter::once(&doc.len())) {
        let cut = cut.clamp(at, doc.len());
        parser.feed(
            doc.get(at..cut).unwrap_or_default(),
            cut == doc.len(),
            handler,
        )?;
        at = cut;
        if at == doc.len() {
            break;
        }
    }
    Ok(())
}

/// Text splits where the input is cut, as in expat; merged, it is the same.
#[test]
fn cutting_a_valid_document_only_splits_text() {
    for &doc in SAMPLES {
        let whole = report(doc, 0);
        for chunk in 1..=17 {
            assert_eq!(report(doc, chunk), whole, "{chunk}-byte pieces of {doc:?}");
        }
    }
}

#[test]
fn a_long_start_tag_survives_any_cut() {
    let mut doc = b"<r><f v='".to_vec();
    doc.extend(core::iter::repeat_n(b'x', 70_000));
    doc.extend_from_slice(b"'/></r>");
    let whole = report(&doc, 0);
    assert!(whole.starts_with("Sr\nSf\nAv=xxx"));
    assert!(whole.ends_with("Ef\nEr\nok\n"));
    assert_eq!(report(&doc, 1024), whole);
    assert_eq!(report(&doc, 999), whole);
}

#[test]
fn a_long_text_run_comes_out_before_its_end() {
    #[derive(Default)]
    struct Count(usize);
    impl Handler for Count {
        fn text(&mut self, text: &[u8]) -> Flow {
            self.0 += text.len();
            Flow::Continue(())
        }
    }
    let mut parser = Parser::new(64 * 1024);
    let mut count = Count::default();
    assert_eq!(parser.feed(b"<a>", false, &mut count), Ok(()));
    let line = [b'z'; 1000];
    for _ in 0..200 {
        assert_eq!(parser.feed(&line, false, &mut count), Ok(()));
    }
    assert!(count.0 > 150_000, "{}", count.0);
    assert_eq!(parser.feed(b"</a>", true, &mut count), Ok(()));
    assert_eq!(count.0, 200_000);
}

#[test]
fn tokens_past_the_limit_fail_without_growing() {
    let mut parser = Parser::new(1000);
    let mut rec = Recorder::default();
    let mut doc = b"<a><!--".to_vec();
    doc.extend(core::iter::repeat_n(b'c', 5000));
    doc.extend_from_slice(b"--></a>");
    assert_eq!(
        feed_all(&mut parser, &doc, &[100, 600, 1200, 2400], &mut rec),
        Err(Error::NoMemory)
    );
    assert_eq!(parser.feed(b"", true, &mut rec), Err(Error::NoMemory));
    // Open elements count too.
    let mut deep = Vec::new();
    for _ in 0..400 {
        deep.extend_from_slice(b"<abcd>");
    }
    assert_eq!(
        Parser::new(1000).feed(&deep, true, &mut rec),
        Err(Error::NoMemory)
    );
}

#[test]
fn a_handler_can_stop() {
    struct Stop(Vec<u8>);
    impl Handler for Stop {
        fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
            self.0.extend_from_slice(tag.name().bytes());
            if tag.name().bytes() == b"b" {
                Flow::Break(())
            } else {
                Flow::Continue(())
            }
        }
        fn end_element(&mut self, name: Str<'_>) -> Flow {
            self.0.push(b'/');
            self.0.extend_from_slice(name.bytes());
            Flow::Continue(())
        }
    }
    let mut stop = Stop(Vec::new());
    let mut parser = Parser::new(1 << 16);
    assert_eq!(
        parser.feed(b"<a><b/><c/></a>", true, &mut stop),
        Err(Error::Aborted)
    );
    // As in expat, the end of an empty element still follows its start.
    assert_eq!(stop.0, b"ab/b");
    assert_eq!(parser.feed(b"", true, &mut stop), Err(Error::Aborted));
}

#[test]
fn start_tags_know_where_they_are_in_the_input() {
    struct Offsets(Vec<u64>);
    impl Handler for Offsets {
        fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
            self.0.push(tag.offset());
            Flow::Continue(())
        }
    }
    let cases: &[(&[u8], &[u64])] = &[
        (b"<a><b/></a>", &[0, 3]),
        (b"\xef\xbb\xbf<a>\xc3\xa9<b/></a>", &[3, 8]),
        (
            b"<?xml version='1.0' encoding='ISO-8859-1'?><a>\xe9\xe9<b/></a>",
            &[43, 48],
        ),
        (
            b"\xff\xfe<\0a\0>\0=\xd8\0\xde\xe9\0<\0b\0/\0>\0<\0/\0a\0>\0",
            &[2, 14],
        ),
    ];
    for &(doc, expected) in cases {
        for chunk in [0, 1, 3] {
            let mut offsets = Offsets(Vec::new());
            let mut parser = Parser::new(1 << 16);
            let step = if chunk == 0 { doc.len() } else { chunk };
            let cuts: Vec<usize> = (step..doc.len()).step_by(step).collect();
            assert_eq!(
                feed_all(&mut parser, doc, &cuts, &mut offsets),
                Ok(()),
                "{doc:?}"
            );
            assert_eq!(offsets.0, expected, "{doc:?} in {chunk}-byte pieces");
        }
    }
}

#[test]
fn reads_the_encodings_expat_knows() {
    assert_eq!(
        report(
            b"<?xml version='1.0' encoding='iso-8859-1'?><a b='\xe9'>\xe0</a>",
            0
        ),
        "Sa\nAb=\\C3\\A9\nT\\C3\\A0\nEa\nok\n"
    );
    assert_eq!(
        report(b"\xff\xfe<\0a\0>\0\xe9\0<\0/\0a\0>\0", 0),
        "Sa\nT\\C3\\A9\nEa\nok\n"
    );
    assert_eq!(report(b"\0<\0a\0/\0>", 0), "Sa\nEa\nok\n");
    assert_eq!(
        report(b"<?xml version='1.0' encoding='US-ASCII'?><a>\xe9</a>", 0),
        "Sa\nerror\n"
    );
    for (doc, error) in [
        (
            &b"<?xml version='1.0' encoding='windows-1252'?><a/>"[..],
            Error::UnknownEncoding,
        ),
        (
            b"<?xml version='1.0' encoding='UTF-16'?><a/>",
            Error::IncorrectEncoding,
        ),
        (b"<?xml encoding='UTF-8'?><a/>", Error::XmlDecl),
        (b" <?xml version='1.0'?><a/>", Error::MisplacedXmlPi),
    ] {
        let mut parser = Parser::new(1 << 16);
        assert_eq!(
            parser.feed(doc, true, &mut Recorder::default()),
            Err(error),
            "{doc:?}"
        );
    }
}

#[test]
fn reads_the_internal_subset_as_expat_without_general_entities() {
    // A declared entity reads as its reference; an undeclared one fails, unless an external
    // subset might declare it; attribute defaults apply, tokenized values collapse spaces.
    assert_eq!(
        report(b"<!DOCTYPE a [<!ENTITY e 'v'><!ATTLIST a d CDATA 'x' t NMTOKENS ' 1  2 '>]><a k=' &e; '>&e;</a>", 0),
        "Sa\nAk= &e; \nAd=x\nAt=1 2\nT&e;\nEa\nok\n"
    );
    assert_eq!(report(b"<a>&e;</a>", 0), "Sa\nerror\n");
    assert_eq!(
        report(b"<!DOCTYPE a SYSTEM 's'><a x='&e;'>&e;</a>", 0),
        "Sa\nAx=\nKe\nEa\nok\n"
    );
    assert_eq!(
        report(
            b"<?xml version='1.0' standalone='yes'?><!DOCTYPE a SYSTEM 's'><a>&e;</a>",
            0
        ),
        "Sa\nerror\n"
    );
    assert_eq!(
        report(
            b"<!DOCTYPE a [<!NOTATION n SYSTEM 's'><!ENTITY e SYSTEM 's' NDATA n>]><a>&e;</a>",
            0
        ),
        "Sa\nerror\n"
    );
}

#[test]
fn says_where_it_failed_and_stays_failed() {
    let mut parser = Parser::new(1 << 16);
    let mut rec = Recorder::default();
    assert_eq!(
        parser.feed(b"<a>\n<b></c>", false, &mut rec),
        Err(Error::TagMismatch)
    );
    assert_eq!(parser.error_offset(), 7);
    assert_eq!(
        parser.feed(b"</a>", true, &mut rec),
        Err(Error::TagMismatch)
    );
    let mut done = Parser::new(1 << 16);
    assert_eq!(done.feed(b"<a/>", true, &mut rec), Ok(()));
    assert_eq!(done.feed(b"", true, &mut rec), Err(Error::Finished));
}

/// Cuts and byte changes of the samples: no panic, and a document that parses whole and in pieces
/// reports the same either way (a broken one may differ before its error, as in expat).
#[test]
fn survives_broken_input() {
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for &doc in SAMPLES {
        let same_when_valid = |doc: &[u8], chunk: usize| {
            let (whole, cut) = (report(doc, 0), report(doc, chunk));
            if whole.ends_with("ok\n") && cut.ends_with("ok\n") {
                assert_eq!(cut, whole, "{doc:?} in {chunk}-byte pieces");
            }
        };
        for cut in 0..=doc.len() {
            same_when_valid(doc.get(..cut).unwrap_or_default(), 1);
        }
        for _ in 0..300 {
            let mut doc = doc.to_vec();
            for _ in 0..3 {
                let at = usize::try_from(next() % doc.len() as u64).unwrap_or(0);
                if let Some(b) = doc.get_mut(at) {
                    #[allow(clippy::cast_possible_truncation)]
                    let byte = next() as u8;
                    *b = byte;
                }
            }
            same_when_valid(&doc, usize::try_from(next() % 7).unwrap_or(0) + 1);
        }
    }
}

#[test]
fn finds_the_first_duplicate_among_many_attributes() {
    let tag = |names: &[usize], bad: Option<usize>| {
        let mut doc = b"<a".to_vec();
        for (i, n) in names.iter().enumerate() {
            let value: &[u8] = if Some(i) == bad { b"&#0;" } else { b"v" };
            doc.extend_from_slice(alloc::format!(" n{n}='").as_bytes());
            doc.extend_from_slice(value);
            doc.push(b'\'');
        }
        doc.extend_from_slice(b"/>");
        let mut parser = Parser::new(1 << 16);
        parser.feed(&doc, true, &mut Recorder::default())
    };
    let distinct: Vec<usize> = (0..40).rev().collect();
    assert_eq!(tag(&distinct, None), Ok(()));
    let twice: Vec<usize> = distinct
        .iter()
        .enumerate()
        .map(|(i, &n)| match i {
            30 => 34,
            35 => 37,
            _ => n,
        })
        .collect();
    assert_eq!(tag(&twice, None), Err(Error::DuplicateAttribute));
    // As in expat, attributes are checked in order: a bad value before the duplicate wins.
    assert_eq!(tag(&twice, Some(20)), Err(Error::BadCharRef));
    assert_eq!(tag(&twice, Some(31)), Err(Error::DuplicateAttribute));
}

#[test]
fn a_large_internal_subset_stays_fast() {
    // Thousands of entity and attribute declarations (half a megabyte), as expat's hash tables
    // handled them.
    let mut doc = b"<!DOCTYPE r [".to_vec();
    for i in 0..10_000 {
        doc.extend_from_slice(
            alloc::format!("<!ENTITY e{i} 'v'><!ATTLIST x{i} a{i} CDATA 'd{i}'>").as_bytes(),
        );
    }
    doc.extend_from_slice(b"<!ENTITY e0 'again'><!ATTLIST x0 a0 CDATA 'again'>]><r>");
    for i in (0..10_000).step_by(997) {
        doc.extend_from_slice(alloc::format!("&e{i};<x{i}/>").as_bytes());
    }
    doc.extend_from_slice(b"</r>");
    let out = report(&doc, 0);
    assert!(
        out.ends_with("Er\nok\n"),
        "{}",
        out.get(out.len().saturating_sub(200)..).unwrap_or("")
    );
    // The first declaration of an attribute counts.
    assert!(out.contains("T&e0;\nSx0\nAa0=d0\nEx0\n"));
    assert!(out.contains("T&e997;\nSx997\nAa997=d997\nEx997\n"));
}

#[test]
fn input_past_the_limit_is_refused_before_it_is_copied() {
    let mut parser = Parser::new(1000);
    let big = [b'a'; 1001];
    assert_eq!(
        parser.feed(&big, false, &mut Recorder::default()),
        Err(Error::NoMemory)
    );
}

#[test]
fn can_skip_entities_nobody_declared() {
    let doc = b"<!DOCTYPE html><p a='x&nbsp;y'>a&nbsp;b&amp;&e;</p>";
    assert_eq!(report(doc, 0), "error\n");
    let mut rec = Recorder::default();
    let mut parser = Parser::new(1 << 16);
    parser.skip_undefined_entities();
    assert_eq!(parser.feed(doc, true, &mut rec), Ok(()));
    rec.flush();
    assert_eq!(rec.out, "Sp\nAa=xy\nTa\nKnbsp\nTb&\nKe\nEp\n");
    // Declared entities and errors other than undefined entities stay as they were.
    let mut parser = Parser::new(1 << 16);
    parser.skip_undefined_entities();
    let doc = b"<!DOCTYPE p [<!ENTITY d 'v'>]><p>&d;&#0;</p>";
    let mut rec = Recorder::default();
    assert_eq!(parser.feed(doc, true, &mut rec), Err(Error::BadCharRef));
    rec.flush();
    assert_eq!(rec.out, "Sp\nT&d;\n");
}
