use std::string::ToString;
use std::vec::Vec;

use crate::number::{parse_f32, parse_i32};

#[test]
fn reads_numbers_as_from_chars_does() {
    assert_eq!(parse_f32(b"1.5"), Some(1.5));
    assert_eq!(parse_f32(b".5"), Some(0.5));
    assert_eq!(parse_f32(b"5."), Some(5.0));
    assert_eq!(
        parse_f32(b"-0").map(f32::to_bits),
        Some((-0.0f32).to_bits())
    );
    assert_eq!(parse_f32(b"1e2"), Some(100.0));
    assert_eq!(parse_f32(b"inf"), Some(f32::INFINITY));
    assert!(parse_f32(b"nan(abc)").is_some_and(f32::is_nan));
    for bad in [
        &b""[..],
        b".",
        b"-",
        b"+1",
        b"1e",
        b"1e+",
        b"0x10",
        b"1.2.3",
        b"--1",
        b"infx",
        b"nan(",
        b" 1",
    ] {
        assert_eq!(
            parse_f32(bad),
            None,
            "{}",
            std::string::String::from_utf8_lossy(bad)
        );
    }
    // Out of range: rounds to infinity, or to zero from non-zero digits.
    assert_eq!(parse_f32(b"3.4028235e38"), Some(f32::MAX));
    assert_eq!(parse_f32(b"3.40282357e38"), None);
    assert_eq!(parse_f32(b"1e-45"), Some(f32::from_bits(1)));
    assert_eq!(parse_f32(b"7e-46"), None);
    assert_eq!(parse_f32(b"0e999"), Some(0.0));

    assert_eq!(parse_i32(b"700"), Some(700));
    assert_eq!(parse_i32(b"-2147483648"), Some(i32::MIN));
    assert_eq!(parse_i32(b"2147483648"), None);
    assert_eq!(parse_i32(b"+7"), None);
    assert_eq!(parse_i32(b"7e2"), None);
}

/// The text of `x`'s exact value, for numbers that sit halfway between two floats.
fn exact_decimal(mantissa: u64, exponent: i32) -> std::string::String {
    // mantissa × 2^exponent = mantissa × 5^-exponent / 10^-exponent for negative exponents.
    let mut digits: Vec<u32> = mantissa
        .to_string()
        .bytes()
        .map(|b| u32::from(b - b'0'))
        .collect();
    let mut point = digits.len() as i32;
    if exponent >= 0 {
        for _ in 0..exponent {
            let mut carry = 0;
            for d in digits.iter_mut().rev() {
                let v = *d * 2 + carry;
                *d = v % 10;
                carry = v / 10;
            }
            if carry > 0 {
                digits.insert(0, carry);
                point += 1;
            }
        }
    } else {
        for _ in 0..-exponent {
            // Divide by 2: append a digit.
            digits.push(0);
            let mut rem = 0;
            for d in &mut digits {
                let v = rem * 10 + *d;
                *d = v / 2;
                rem = v % 2;
            }
        }
    }
    let mut s: std::string::String = digits.iter().map(|d| char::from(b'0' + *d as u8)).collect();
    if point <= 0 {
        s = std::format!("0.{}{}", "0".repeat((-point) as usize), s);
    } else if (point as usize) < s.len() {
        s.insert(point as usize, '.');
    }
    s
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Rust's own parser rounds correctly too; `from_chars` also rejects what overflows or underflows.
fn reference(s: &str) -> Option<f32> {
    let v: f32 = s.parse().ok()?;
    let digits_non_zero = s
        .bytes()
        .take_while(|b| !matches!(b, b'e' | b'E'))
        .any(|b| matches!(b, b'1'..=b'9'));
    if v.is_infinite() || (v == 0.0 && digits_non_zero) {
        None
    } else {
        Some(v)
    }
}

#[test]
fn rounds_like_the_reference_parser() {
    let mut rng = Rng(0x1234_5678_9ABC_DEF1);
    for _ in 0..300_000 {
        let long = rng.below(4) == 0;
        let digits = 1 + rng.below(if long { 140 } else { 12 }) as usize;
        let mut s = std::string::String::new();
        let point = rng.below(digits as u64 + 2) as usize;
        for i in 0..digits {
            if i == point {
                s.push('.');
            }
            s.push(char::from(b'0' + rng.below(10) as u8));
        }
        if rng.below(3) == 0 {
            s.push_str(&std::format!("e{}", rng.below(100) as i64 - 60));
        }
        let got = parse_f32(s.as_bytes()).map(f32::to_bits);
        assert_eq!(got, reference(&s).map(f32::to_bits), "{s}");
    }
}

#[test]
fn rounds_halfway_points_to_even() {
    let mut rng = Rng(0xDEAD_BEEF_1234_5678);
    for _ in 0..20_000 {
        // An odd multiple of half an ulp, anywhere from the subnormals to the top.
        let mantissa = (rng.below(1 << 24) << 1) | 1;
        let exponent = rng.below(254 + 24) as i32 - 150 - 24 + 24;
        let text = exact_decimal(mantissa, exponent);
        let expected = reference(&text).map(f32::to_bits);
        assert_eq!(
            parse_f32(text.as_bytes()).map(f32::to_bits),
            expected,
            "{text}"
        );
        // Just above and below: a digit past the exact value.
        for nudge in [
            "1",
            "0000000000000000000000000000000000000000000000000000000000001",
        ] {
            let above = if text.contains('.') {
                std::format!("{text}{nudge}")
            } else {
                std::format!("{text}.{nudge}")
            };
            assert_eq!(
                parse_f32(above.as_bytes()).map(f32::to_bits),
                reference(&above).map(f32::to_bits),
                "{above}"
            );
        }
    }
}

// ---- The whole parser ----

use crate::cache::{self, Load, Source, Status};
use crate::{Ancestor, Parse, Query, Rules, Style, Tokenizer, parse_inline};

struct Bytes<'a>(&'a [u8]);

impl Source for Bytes<'_> {
    fn read(&mut self, buf: &mut [u8]) -> bool {
        let Some((head, rest)) = self.0.split_at_checked(buf.len()) else {
            self.0 = &[];
            return false;
        };
        buf.copy_from_slice(head);
        self.0 = rest;
        true
    }
    fn remaining(&mut self) -> usize {
        self.0.len()
    }
    fn skip(&mut self, len: usize) -> bool {
        self.0.get(len..).map(|rest| self.0 = rest).is_some()
    }
}

fn tokenizer() -> std::boxed::Box<Tokenizer> {
    let mut slot = std::boxed::Box::new_uninit();
    Tokenizer::init(&mut slot);
    // SAFETY: init set it up.
    unsafe { slot.assume_init() }
}

fn load(rules: &mut Rules, css: &[u8], chunk: usize) -> Parse {
    let mut t = tokenizer();
    for piece in css.chunks(chunk.max(1)) {
        t.feed(rules, piece);
    }
    t.finish(rules)
}

fn saved(rules: &Rules) -> Vec<u8> {
    let mut out = Vec::new();
    assert!(cache::save(rules, true, |b| {
        out.extend_from_slice(b);
        true
    }));
    out
}

#[test]
fn reads_a_stylesheet() {
    let mut rules = Rules::new();
    let css = b"P { text-align: center; }\n.Note { font-weight: bold; text-align: right; }\n\
                p.note { font-style: italic; text-align: justify; }\n.note { margin-top: 2em; }\n\
                div.poem > p::first-letter { float: left; initial-letter: 3 }";
    assert_eq!(load(&mut rules, css, 7), Parse::Complete);
    assert_eq!(rules.len(), 5);
    let q = Query {
        tag: b"p",
        classes: b"NOTE",
        id: b"",
        ancestors: &[],
        first_letter: false,
    };
    let style = rules.resolve(&q);
    assert_eq!(style.text_align, crate::TextAlign::Justify);
    assert_eq!(style.font_weight, crate::FontWeight::Bold);
    assert_eq!(
        style.length(crate::LengthField::MarginTop),
        crate::Length {
            value: 2.0,
            unit: crate::Unit::Em
        }
    );

    let ancestors = [Ancestor::new(b"div", b"poem", b"")];
    let q = Query {
        tag: b"p",
        classes: b"",
        id: b"",
        ancestors: &ancestors,
        first_letter: true,
    };
    assert_eq!(rules.resolve(&q).initial_letter, 3);

    let mut copy = Rules::new();
    assert_eq!(
        cache::load(&mut copy, &mut Bytes(&saved(&rules))),
        Load::Complete
    );
    assert_eq!(saved(&copy), saved(&rules));
    assert_eq!(
        parse_inline(b"font-style: oblique; ;margin: 1px 2px").font_style,
        crate::FontStyle::Italic
    );
}

const CSS_BYTES: &[u8] = b"{}{};:,.#>*/ \n\t@!%+-~[]()abcdeEpP0123456789\x1f\xc3\xa9";
const WORDS: [&str; 24] = [
    "p",
    "div",
    ".a",
    "#x",
    " > ",
    "::first-letter",
    "{",
    "}",
    ";",
    ":",
    "margin",
    "border-top",
    "font-size",
    "1.5em",
    "inherit",
    "/*",
    "*/",
    "@media",
    "!important",
    "background",
    "#ccc",
    "rgb(9,9,9)",
    "initial-letter",
    ",",
];

fn random_css(rng: &mut Rng) -> Vec<u8> {
    let mut css = Vec::new();
    for _ in 0..rng.below(200) {
        if rng.below(2) == 0 {
            css.extend_from_slice(WORDS[rng.below(WORDS.len() as u64) as usize].as_bytes());
        } else {
            css.push(CSS_BYTES[rng.below(CSS_BYTES.len() as u64) as usize]);
        }
    }
    css
}

/// Random text, cut anywhere: no panic, and the chunk size never changes the rules.
#[test]
fn survives_random_stylesheets() {
    let mut rng = Rng(0x0123_4567_89AB_CDEF);
    for _ in 0..5000 {
        let css = random_css(&mut rng);
        let mut whole = Rules::new();
        let result = load(&mut whole, &css, css.len());
        let mut chunked = Rules::new();
        assert_eq!(load(&mut chunked, &css, 1 + rng.below(9) as usize), result);
        assert_eq!(saved(&whole), saved(&chunked));
        let _ = parse_inline(&css);

        let ancestors = [
            Ancestor::new(b"div", b"a b", b"x"),
            Ancestor::new(b"p", b"", b""),
        ];
        for first_letter in [false, true] {
            let q = Query {
                tag: b"p",
                classes: b"a A b",
                id: b"x",
                ancestors: &ancestors,
                first_letter,
            };
            let _ = whole.resolve(&q);
        }
    }
}

/// Every truncation and many single-byte changes of a cache: no panic, and what loads saves the
/// same again.
#[test]
fn survives_damaged_caches() {
    let mut rng = Rng(0xFEDC_BA98_7654_3210);
    for _ in 0..200 {
        let mut rules = Rules::new();
        load(&mut rules, &random_css(&mut rng), 64);
        let mut file = saved(&rules);
        if rng.below(2) == 0 {
            file[1] = 1; // partial
        }
        for len in 0..=file.len() {
            let _ = cache::inspect(&mut Bytes(&file[..len]));
            let mut back = Rules::new();
            if cache::load(&mut back, &mut Bytes(&file[..len])) == Load::Complete {
                assert_eq!(len, file.len());
            }
        }
        for _ in 0..50 {
            let mut bad = file.clone();
            let at = rng.below(bad.len() as u64) as usize;
            bad[at] ^= 1 + rng.below(255) as u8;
            let _ = cache::inspect(&mut Bytes(&bad));
            let mut back = Rules::new();
            if cache::load(&mut back, &mut Bytes(&bad)) == Load::Complete {
                let again = saved(&back);
                let mut twice = Rules::new();
                assert_eq!(cache::load(&mut twice, &mut Bytes(&again)), Load::Complete);
            } else {
                assert!(back.is_empty());
            }
        }
    }
    assert_eq!(cache::inspect(&mut Bytes(&[])), Status::Invalid);
    assert_eq!(
        cache::load(&mut Rules::new(), &mut Bytes(&[])),
        Load::StaleVersion
    );
}

#[test]
fn default_style_matches_the_cpp_one() {
    let style = Style::default();
    assert_eq!(style.text_align, crate::TextAlign::Left);
    assert_eq!(style.defined.0, 0);
}
