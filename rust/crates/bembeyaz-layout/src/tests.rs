use crate::text::{cjk_character_break_offsets, explicit_hyphen_break_offsets};
use alloc::vec::Vec;

#[test]
fn splits_cjk_and_hyphens() {
    let mut out = Vec::new();
    assert!(cjk_character_break_offsets("日本語。".as_bytes(), &mut out));
    assert_eq!(out, [3, 6]);
    assert!(explicit_hyphen_break_offsets(
        b"well-known--x-",
        false,
        &mut out
    ));
    assert_eq!(out, [5, 12]);
}

// The four boundary states, as the C++ TokenBoundary.h tests pinned them.
#[test]
fn boundary_states() {
    use crate::text::{allows_break, is_justifiable_gap};
    // Ordinary word gap.
    assert!(allows_break(false, false) && is_justifiable_gap(false, false, false));
    // CJK zero-width gap.
    assert!(allows_break(false, true) && is_justifiable_gap(false, true, false));
    // Attached text.
    assert!(!allows_break(true, false) && !is_justifiable_gap(true, false, false));
    // Breakable attachment after an explicit hyphen.
    assert!(allows_break(true, true) && !is_justifiable_gap(true, true, false));
    // A literal attached space stays justifiable.
    assert!(is_justifiable_gap(true, false, true));
}

#[test]
fn visible_hyphens_allow_breaks() {
    use crate::text::allows_break_after_explicit_hyphen as allows;
    for cp in [
        0x2D, 0x058A, 0x2010, 0x2012, 0x2013, 0x2014, 0x2015, 0x2043, 0x207B, 0x208B, 0x2212,
        0x2E17, 0x2E3A, 0x2E3B, 0xFE58, 0xFE63, 0xFF0D, 0x5F, 0x2026,
    ] {
        assert!(allows(cp), "U+{cp:04X}");
    }
    assert!(!allows(0x00AD) && !allows(0x2011) && !allows(u32::from(b'.')));
}

struct Fixture;

impl crate::Metrics for Fixture {
    fn text_advance(&mut self, _: i32, text: &[u8], style: u8, spacing: i8) -> i32 {
        i32::try_from(text.len()).unwrap_or(0) * (5 + i32::from(style & 3)) + i32::from(spacing)
    }
    fn kerning(&mut self, _: i32, left: u32, right: u32, _: u8, spacing: i8) -> i32 {
        i32::try_from((left ^ right) % 5).unwrap_or(0) - 2 + i32::from(spacing)
    }
    fn space_advance(&mut self, _: i32, _: u32, _: u32, _: u8) -> i32 {
        4
    }
    fn space_width(&mut self, _: i32, _: u8) -> i32 {
        4
    }
    fn ascender(&mut self, _: i32) -> i32 {
        12
    }
    fn is_sd_font(&mut self, _: i32) -> bool {
        true
    }
    fn prewarm(&mut self, _: i32, _: &[&[u8]], _: bool, _: bool, _: u8, _: bool) {}
}

/// Every cut and many one-byte changes of a mixed paragraph, in every mode: no panic.
#[test]
fn survives_truncations_and_byte_changes() {
    use crate::{Align, BlockStyle, Paragraph, Params};
    let text: &[u8] =
        "Fuß-gänger\u{AD}weg — 日本語の「文」 한국어 مرحبا שלום e\u{301}te 12,5 x² ".as_bytes();
    let mut cases: Vec<Vec<u8>> = (0..text.len())
        .map(|n| text.get(..n).unwrap_or_default().to_vec())
        .collect();
    for at in 0..text.len() {
        for b in [0u8, 0x80, 0xC2, 0xE3, 0xFF, b'-', b' '] {
            let mut case = text.to_vec();
            if let Some(slot) = case.get_mut(at) {
                *slot = b;
            }
            cases.push(case);
        }
    }
    for (n, case) in cases.iter().enumerate() {
        let mut paragraph = Paragraph::new(n % 2 == 0, n % 3 == 0, 2, None);
        for (i, word) in case.split(|&b| b == b' ').enumerate() {
            let flags = u8::try_from(i % 7).unwrap_or(0);
            paragraph.add_word(
                word,
                flags * 17,
                i % 4 == 0,
                i % 3 == 0,
                1000 * u32::from(flags),
                flags % 3,
                flags % 4,
                i % 5 == 0,
            );
        }
        paragraph.set_ruby_group(1, 3, "ルビ".as_bytes());
        for width in [1, 30, 200] {
            let mut block = BlockStyle {
                align: Align::from_index(u8::try_from(n % 5).unwrap_or(0)),
                text_align_defined: n % 2 == 1,
                direction_defined: false,
                is_rtl: n % 7 == 0,
                text_indent_defined: true,
                text_indent: -3,
            };
            let params = Params {
                font: 0,
                slot_fonts: [0, 3, 4, 5],
                slot_count: 4,
                width,
                include_last_line: width == 200,
                character_spacing: 1,
                word_spacing: 120,
                max_lines: 3,
                language: bembeyaz_hyphenation::Language::from_index(0),
                link_count: 2,
            };
            paragraph.layout(&mut Fixture, &mut block, &params, &mut |_| true);
        }
    }
}
