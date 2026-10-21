#![allow(clippy::unwrap_used)]

use super::{
    author_key, clean_person_name, fold, folded_group_initial, matches_query, surname_key,
};
use std::string::String;
use std::vec::Vec;

fn s(f: fn(&[u8], &mut Vec<u8>) -> Result<(), super::OutOfMemory>, text: &str) -> String {
    let mut out = Vec::new();
    f(text.as_bytes(), &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

#[test]
fn folds_both_forms_alike() {
    assert_eq!(s(fold, "L'\u{c9}n\u{e9}ide"), "l'eneide");
    assert_eq!(s(fold, "Le\u{301}nei\u{308}de"), "leneide");
    assert_eq!(
        s(fold, "  S\u{f8}ren -- Kierkegaard!! "),
        "soren kierkegaard"
    );
    assert_eq!(
        s(fold, "\u{3a3}\u{3bf}\u{3c6}\u{3af}\u{3b1} 1984"),
        "\u{3a3}\u{3bf}\u{3c6}\u{3b9}\u{3b1} 1984"
    );
    let mut cut = Vec::new();
    fold(b"abc\xe2\x82", &mut cut).unwrap();
    assert_eq!(cut, b"abc");
    assert_eq!(folded_group_initial("\u{e9}mile".as_bytes()), 0xE9);
    assert_eq!(folded_group_initial(b"1984"), 0);
}

#[test]
fn people() {
    assert_eq!(s(clean_person_name, "Austen, Jane"), "Jane Austen");
    assert_eq!(
        s(clean_person_name, "George Sand [Sand, George]"),
        "George Sand"
    );
    assert_eq!(s(clean_person_name, "Herbert G_ Wells_"), "Herbert G Wells");
    assert_eq!(s(clean_person_name, "H_G_ Wells"), "H.G Wells");
    assert_eq!(s(clean_person_name, "Smith, John, Jr."), "Smith, John, Jr");
    assert_eq!(s(author_key, "Lu, Xun"), s(author_key, "Xun Lu"));
    assert_eq!(s(author_key, "Wollstonecraft, Mary"), "mary wollsto");
    assert_eq!(s(surname_key, "Herman Melville"), "melville herman");
    assert!(matches_query(b"wuthering heights emily bronte", b"wut emi"));
    assert!(matches_query(b"l'eneide", b"eneide"));
    assert!(!matches_query(b"wuthering", b"heights"));
    assert!(matches_query(b"x", b""));
}

#[test]
fn any_bytes_never_panic() {
    let mut seed = 0x1234_5678_u32;
    for _ in 0..3000 {
        let mut text = Vec::new();
        for _ in 0..(seed % 40) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            text.push(seed.to_le_bytes()[0]);
        }
        let mut out = Vec::new();
        let _ = fold(&text, &mut out);
        let _ = clean_person_name(&text, &mut out);
        let _ = author_key(&text, &mut out);
        let _ = surname_key(&text, &mut out);
        let _ = folded_group_initial(&text);
        let _ = matches_query(&text, &out);
    }
}

#[test]
fn cut_and_corrupted_samples_never_panic() {
    let samples: [&[u8]; 4] = [
        "L'\u{c9}n\u{e9}ide".as_bytes(),
        "Wollstonecraft, Mary [W., M.]; Shelley".as_bytes(),
        "S\u{f8}ren Kierkegaard \u{4e2d}\u{6587} 1984".as_bytes(),
        "Herbert G_ Wells_ (ed.)".as_bytes(),
    ];
    let mut seed = 0x00DD_BA11_u32;
    for sample in samples {
        for end in 0..=sample.len() {
            for flip in 0..sample.len() {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let mut text = sample.get(..end).unwrap().to_vec();
                if let Some(byte) = text.get_mut(flip) {
                    *byte = seed.to_le_bytes()[0];
                }
                let mut out = Vec::new();
                let _ = fold(&text, &mut out);
                let _ = author_key(&text, &mut out);
                let _ = clean_person_name(&text, &mut out);
                let _ = surname_key(&text, &mut out);
                let _ = folded_group_initial(&text);
                let _ = matches_query(&out, &text);
            }
        }
    }
}
