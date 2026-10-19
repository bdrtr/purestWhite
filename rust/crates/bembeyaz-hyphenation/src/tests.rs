use super::*;
use std::string::String;
use std::vec::Vec;

fn lang(tag: &str) -> Option<Language> {
    language(tag.as_bytes())
}

fn collect(word: &str, tag: &str, fallback: bool) -> Vec<Break> {
    let mut out = Vec::new();
    breaks(word.as_bytes(), lang(tag), fallback, |b| out.push(b));
    out
}

/// The word with `-` at hyphenated breaks and `|` at the others.
fn show(word: &str, tag: &str, fallback: bool) -> String {
    let found = collect(word, tag, fallback);
    let mut out = String::new();
    for (i, c) in word.char_indices() {
        if let Some(b) = found.iter().find(|b| b.offset == i) {
            out.push(if b.inserted_hyphen { '-' } else { '|' });
        }
        out.push(c);
    }
    out
}

#[test]
fn picks_languages_by_tag() {
    for (tag, want) in [
        ("en", Some("en")),
        ("EN-us", Some("en")),
        ("eng", Some("en")),
        ("fre", Some("fr")),
        ("ger", Some("de")),
        ("de_AT", Some("de")),
        ("ukr", Some("uk")),
        ("pol", None),
        ("por", None),
        ("", None),
        ("-en", None),
        ("english", None),
        ("tr", None),
    ] {
        assert_eq!(lang(tag), want.and_then(lang), "{tag}");
    }
    assert_eq!(language(b"en\0x"), None);
    for i in 0..11 {
        assert_eq!(Language::from_index(i).map(Language::index), Some(i));
    }
    assert_eq!(Language::from_index(11), None);
}

#[test]
fn breaks_by_the_patterns() {
    assert_eq!(
        show("Quadratkilometer", "de", false),
        "Qua-drat-ki-lo-me-ter"
    );
    // English keeps three letters on each side.
    assert_eq!(show("hyphenation", "en", false), "hyphen-ation");
    assert_eq!(
        show("«Quadratkilometer».", "de", false),
        "«Qua-drat-ki-lo-me-ter»."
    );
    assert_eq!(show("Quadratkilometer", "", false), "Quadratkilometer");
}

#[test]
fn breaks_compounds_at_their_hyphens_and_inside_their_parts() {
    assert_eq!(
        show("US-Satellitensystems", "de", false),
        "US-|Sa-tel-li-ten-sys-tems"
    );
    // A soft hyphen is invisible: the break draws one.
    assert_eq!(show("Satel\u{AD}liten", "de", false), "Sa-tel\u{AD}-li-ten");
    // No fallback inside the parts of a hyphenated word.
    assert_eq!(show("ab-cdefgh", "", true), "ab-|cdefgh");
}

#[test]
fn breaks_long_contractions_at_the_apostrophe() {
    assert_eq!(show("all'improvviso", "it", false), "all'|im-prov-vi-so");
    assert_eq!(show("l'improvviso", "it", false), "l'im-prov-vi-so");
    // Too short on both sides for English, even with fallback.
    assert_eq!(show("they've", "en", true), "they've");
}

#[test]
fn falls_back_to_every_letter() {
    assert_eq!(show("xyzzyx", "", false), "xyzzyx");
    assert_eq!(show("xyzzyx", "", true), "xy-z-z-yx");
    assert_eq!(show("xqxqxq", "en", true), "xqx-qxq");
    // CJK characters break without a hyphen.
    assert_eq!(show("中文字符", "", true), "中文|字符");
}

#[test]
fn trims_footnotes_and_punctuation() {
    // Two digits or more in brackets are a footnote mark.
    assert_eq!(show("xyzzy[12].", "", true), "xy-z-zy[12].");
    // One digit is not; the closing bracket is trimmed as punctuation.
    assert_eq!(show("xyzzy[1]", "", true), "xy-z-z-y-[1]");
}

#[test]
fn composes_accents_before_looking_up_patterns() {
    let composed = collect("Fu\u{DF}g\u{E4}nger\u{FC}berweg", "de", false);
    let decomposed = collect("Fu\u{DF}ga\u{308}ngeru\u{308}berweg", "de", false);
    assert!(!composed.is_empty());
    // Same breaks, each a byte later per decomposed letter before it.
    assert_eq!(composed.len(), decomposed.len());
}

#[test]
fn words_of_any_length() {
    let long: String = "ab".repeat(300);
    let found = collect(&long, "", true);
    assert_eq!(found.len(), long.len() - 3);
    assert_eq!(found.first().map(|b| b.offset), Some(2));
    // Too long for the patterns, which stop at 68 letters.
    let word = "Quadratkilometer".repeat(5);
    assert!(collect(&word, "de", false).is_empty());
    assert!(!collect(&word[..64], "de", false).is_empty());
}

#[test]
fn stops_at_a_nul() {
    assert_eq!(
        collect("xyzzy\0xyzzy", "", true),
        collect("xyzzy", "", true)
    );
    assert!(collect("\0xyzzy", "", true).is_empty());
    assert!(collect("", "", true).is_empty());
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 32) as u32
    }
}

/// Random bytes and text through every language: no panic, breaks in order inside the word.
#[test]
fn survives_any_input() {
    let pieces: [&[u8]; 16] = [
        b"a",
        b"Q",
        b"-",
        b"'",
        b"\xE2\x80\x99",
        b"\xC2\xAD",
        b"[",
        b"]",
        b"7",
        b".",
        b"\xC3\xA9",
        b"\xD0\x96",
        b"\xE4\xB8\xAD",
        b"\xCC\x81",
        b"\x80",
        b"\0",
    ];
    let mut rng = Rng(0x2545_F491_4F6C_DD1D);
    for round in 0..20_000 {
        let mut word = Vec::new();
        let len = rng.next() % if round % 50 == 0 { 400 } else { 24 };
        for _ in 0..len {
            if rng.next().is_multiple_of(4) {
                word.push(rng.next() as u8);
            } else {
                word.extend_from_slice(pieces[rng.next() as usize % pieces.len()]);
            }
        }
        for language in (0..12).map(Language::from_index) {
            let mut last = 0;
            breaks(&word, language, round % 2 == 0, |b| {
                assert!(b.offset > last && b.offset < word.len(), "{word:?}");
                last = b.offset;
            });
        }
    }
}

#[test]
fn every_trie_has_a_root() {
    for i in 0..11 {
        let language = Language::from_index(i).unwrap();
        // A word every pattern file breaks somewhere, letters only.
        let word: &str = if matches!(i, 3 | 8) {
            "переносить"
        } else {
            "abandonnerait"
        };
        let cps: Vec<u32> = word.chars().map(u32::from).collect();
        assert!(
            !matches!(language.pattern_breaks(&cps), Breaks::None),
            "language {i}"
        );
    }
}

/// Every truncation and every single-byte change of some words: no panic, breaks in order.
#[test]
fn survives_truncations_and_byte_changes() {
    let words = [
        "Donaudampfschifffahrtsgesellschaft",
        "US-Satellitensystems[12].",
        "«all’improvviso»",
        "Satel\u{AD}li\u{308}ten",
        "переносить—слово",
    ];
    let check = |word: &[u8]| {
        for language in (0..12).map(Language::from_index) {
            for fallback in [false, true] {
                let mut last = 0;
                breaks(word, language, fallback, |b| {
                    assert!(b.offset > last && b.offset < word.len(), "{word:?}");
                    last = b.offset;
                });
            }
        }
    };
    for word in words {
        let bytes = word.as_bytes();
        for end in 0..=bytes.len() {
            check(&bytes[..end]);
        }
        for at in 0..bytes.len() {
            for value in [0x00, 0x2D, 0x27, 0x80, 0xC3, 0xE2, 0xFF] {
                let mut changed = bytes.to_vec();
                changed[at] = value;
                check(&changed);
            }
        }
    }
}
