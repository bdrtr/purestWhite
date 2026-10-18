//! The Unicode conformance tests for the algorithm (UAX #9 §4.3): `BidiTest.txt` by bidi class,
//! `BidiCharacterTest.txt` by codepoint. They are large and not in git: `./fetch-unicode.sh` puts
//! them in `unicode/`, then
//!
//!     cargo test --release -p bembeyaz-bidi conformance -- --ignored --nocapture
//!
//! prints how many cases pass. `BidiCharacterTest` lines are used only where every character has
//! the same class here as in Unicode (the tables cover a few scripts on purpose) and no bracket
//! that the tables pair differently. `BIDI_CONFORMANCE_SHOW=<n>` prints the first n failures.

extern crate std;

use super::*;
use std::collections::BTreeMap;
use std::eprintln;
use std::fs;
use std::path::PathBuf;
use std::string::String;
use std::vec::Vec;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("unicode")
}

fn class_named(name: &str) -> Class {
    use Class::{
        AL, AN, B, BN, CS, EN, ES, ET, FSI, L, LRE, LRI, LRO, NSM, ON, PDF, PDI, R, RLE, RLI, RLO,
        S, WS,
    };
    match name {
        "L" => L,
        "R" => R,
        "AL" => AL,
        "EN" => EN,
        "ES" => ES,
        "ET" => ET,
        "AN" => AN,
        "CS" => CS,
        "NSM" => NSM,
        "BN" => BN,
        "B" => B,
        "S" => S,
        "WS" => WS,
        "ON" => ON,
        "LRE" => LRE,
        "LRO" => LRO,
        "RLE" => RLE,
        "RLO" => RLO,
        "PDF" => PDF,
        "LRI" => LRI,
        "RLI" => RLI,
        "FSI" => FSI,
        "PDI" => PDI,
        _ => panic!("class {name}"),
    }
}

/// Removed by rule X9: no level, no place in the visual order.
fn removed(class: Class) -> bool {
    matches!(
        class,
        Class::RLE | Class::LRE | Class::RLO | Class::LRO | Class::PDF | Class::BN
    )
}

/// Levels (`None` for removed characters), paragraph level and visual order of the kept ones.
fn run(line: &[Char], auto: bool, level: u8) -> (Vec<Option<u8>>, u8, Vec<usize>) {
    let mut levels = [0u8; MAX_LINE];
    let paragraph = reorder::resolve(line, auto, level, &mut levels).unwrap_or(0);
    let shown = line
        .iter()
        .zip(levels)
        .map(|(c, l)| (!removed(c.class)).then_some(l))
        .collect();
    let mut visual: Vec<Char> = line.to_vec();
    reorder::reorder(&mut visual, auto, level);
    let order = visual
        .iter()
        .filter(|c| !removed(c.class))
        .map(|c| usize::from(c.index))
        .collect();
    (shown, paragraph, order)
}

fn chars(classes: &[(u32, Class)]) -> Vec<Char> {
    (0u16..)
        .zip(classes)
        .map(|(index, &(cp, class))| Char {
            cp,
            index,
            joiners: 0,
            class,
        })
        .collect()
}

struct Tally {
    name: &'static str,
    total: usize,
    passed: usize,
    show: usize,
    by_first_class: BTreeMap<String, usize>,
}

impl Tally {
    fn new(name: &'static str) -> Self {
        let show = std::env::var("BIDI_CONFORMANCE_SHOW")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        Self {
            name,
            total: 0,
            passed: 0,
            show,
            by_first_class: BTreeMap::new(),
        }
    }
    fn record(&mut self, ok: bool, describe: impl FnOnce() -> String, key: String) {
        self.total += 1;
        if ok {
            self.passed += 1;
        } else {
            *self.by_first_class.entry(key).or_insert(0) += 1;
            if self.show > 0 {
                self.show -= 1;
                eprintln!("{}: {}", self.name, describe());
            }
        }
    }
    fn report(&self) {
        eprintln!(
            "{}: {} of {} pass ({:.1}%)",
            self.name,
            self.passed,
            self.total,
            100.0 * self.passed as f64 / self.total.max(1) as f64
        );
        let mut keys: Vec<_> = self.by_first_class.iter().collect();
        keys.sort_by(|a, b| b.1.cmp(a.1));
        for (key, count) in keys.iter().take(12) {
            eprintln!("  {count:7} failing contain {key}");
        }
    }
}

/// The classes a failing case contains, as a coarse key for grouping.
fn key(classes: impl Iterator<Item = Class>) -> String {
    let mut seen: Vec<String> = classes.map(|c| std::format!("{c:?}")).collect();
    seen.sort();
    seen.dedup();
    seen.join(" ")
}

#[test]
#[ignore = "needs ./fetch-unicode.sh"]
fn conformance_classes() {
    let Ok(text) = fs::read_to_string(dir().join("BidiTest.txt")) else {
        eprintln!("no unicode/BidiTest.txt: run ./fetch-unicode.sh");
        return;
    };
    let mut tally = Tally::new("BidiTest");
    let mut levels: Vec<Option<u8>> = Vec::new();
    let mut order: Vec<usize> = Vec::new();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some(rest) = line.strip_prefix("@Levels:") {
            levels = rest.split_whitespace().map(|t| t.parse().ok()).collect();
        } else if let Some(rest) = line.strip_prefix("@Reorder:") {
            order = rest
                .split_whitespace()
                .map(|t| t.parse().unwrap())
                .collect();
        } else if let Some((classes, bits)) = line.split_once(';') {
            let classes: Vec<(u32, Class)> = classes
                .split_whitespace()
                .map(|n| (0, class_named(n)))
                .collect();
            let bits: u32 = bits.trim().parse().unwrap();
            let line = chars(&classes);
            for (bit, auto, level) in [(1, true, 0), (2, false, 0), (4, false, 1)] {
                if bits & bit == 0 {
                    continue;
                }
                let (got_levels, _, got_order) = run(&line, auto, level);
                let ok = got_levels == levels && got_order == order;
                tally.record(
                    ok,
                    || {
                        std::format!(
                            "{:?} para {} want {:?} {:?} got {:?} {:?}",
                            classes.iter().map(|c| c.1).collect::<Vec<_>>(),
                            ["auto", "LTR", "", "RTL"][bit as usize - 1],
                            levels,
                            order,
                            got_levels,
                            got_order
                        )
                    },
                    key(classes.iter().map(|c| c.1)),
                );
            }
        }
    }
    tally.report();
}

#[test]
#[ignore = "needs ./fetch-unicode.sh"]
#[allow(clippy::too_many_lines, reason = "parses three files")]
fn conformance_characters() {
    let (Ok(text), Ok(derived), Ok(brackets)) = (
        fs::read_to_string(dir().join("BidiCharacterTest.txt")),
        fs::read_to_string(dir().join("DerivedBidiClass.txt")),
        fs::read_to_string(dir().join("BidiBrackets.txt")),
    ) else {
        eprintln!("no unicode/BidiCharacterTest.txt etc.: run ./fetch-unicode.sh");
        return;
    };
    // Unicode's classes and bracket pairs, to keep only the lines the tables agree on.
    let mut unicode_class: BTreeMap<u32, Class> = BTreeMap::new();
    for line in derived.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((range, name)) = line.split_once(';') else {
            continue;
        };
        let (first, last) = range
            .trim()
            .split_once("..")
            .unwrap_or((range.trim(), range.trim()));
        let (first, last) = (
            u32::from_str_radix(first, 16).unwrap(),
            u32::from_str_radix(last, 16).unwrap(),
        );
        if first > 0xFFFF {
            continue;
        }
        let class = class_named(name.trim());
        for cp in first..=last {
            unicode_class.insert(cp, class);
        }
    }
    let mut unicode_bracket: BTreeMap<u32, (u32, bool)> = BTreeMap::new();
    for line in brackets.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let fields: Vec<&str> = line.split(';').map(str::trim).collect();
        if fields.len() == 3 {
            let cp = u32::from_str_radix(fields[0], 16).unwrap();
            let pair = u32::from_str_radix(fields[1], 16).unwrap();
            unicode_bracket.insert(cp, (pair, fields[2] == "o"));
        }
    }
    let agrees = |cp: u32| {
        // Unlisted codepoints default to L, R, AL, ... by block; only listed ones are compared.
        let ours = class(cp);
        let theirs = unicode_class.get(&cp).copied().unwrap_or(Class::L);
        let ours_bracket = bracket(cp);
        let theirs_bracket = unicode_bracket
            .get(&cp)
            .map(|&(pair, open)| if open { cp } else { pair });
        ours == theirs && ours_bracket == theirs_bracket
    };

    let mut tally = Tally::new("BidiCharacterTest");
    let mut skipped = 0;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let fields: Vec<&str> = line.split(';').collect();
        if fields.len() != 5 {
            continue;
        }
        let cps: Vec<u32> = fields[0]
            .split_whitespace()
            .map(|h| u32::from_str_radix(h, 16).unwrap())
            .collect();
        if cps.len() > MAX_LINE || !cps.iter().all(|&cp| agrees(cp)) {
            skipped += 1;
            continue;
        }
        let (auto, level) = match fields[1] {
            "0" => (false, 0),
            "1" => (false, 1),
            _ => (true, 0),
        };
        let want_paragraph: u8 = fields[2].parse().unwrap();
        let want_levels: Vec<Option<u8>> = fields[3]
            .split_whitespace()
            .map(|t| t.parse().ok())
            .collect();
        let want_order: Vec<usize> = fields[4]
            .split_whitespace()
            .map(|t| t.parse().unwrap())
            .collect();
        let classes: Vec<(u32, Class)> = cps.iter().map(|&cp| (cp, class(cp))).collect();
        let line = chars(&classes);
        let (got_levels, got_paragraph, got_order) = run(&line, auto, level);
        // An untouched line reports no paragraph level: it is 0.
        let ok =
            got_levels == want_levels && got_order == want_order && got_paragraph == want_paragraph;
        tally.record(
            ok,
            || {
                std::format!(
                    "{:04X?} para {} want {} {:?} {:?} got {} {:?} {:?}",
                    cps,
                    fields[1],
                    want_paragraph,
                    want_levels,
                    want_order,
                    got_paragraph,
                    got_levels,
                    got_order
                )
            },
            key(classes.iter().map(|c| c.1)),
        );
    }
    tally.report();
    eprintln!(
        "BidiCharacterTest: {skipped} lines skipped (characters the tables treat differently)"
    );
}

/// The examples and the Unicode 8.0 cases from the top of `BidiCharacterTest.txt` (17.0.0): the
/// neutral and bracket examples of UAX #9, overrides on isolates and brackets, marks after
/// brackets (across ZWJ and ZWSP), and the bracket stack's limit. Not here: lines with U+20D6,
/// which the tables leave out, and lines longer than [`MAX_LINE`]. Runs without the downloaded
/// files.
#[test]
fn uax9_examples() {
    let mut failures = Vec::new();
    for line in EXAMPLES.lines() {
        let fields: Vec<&str> = line.split(';').collect();
        let cps: Vec<u32> = fields[0]
            .split_whitespace()
            .map(|h| u32::from_str_radix(h, 16).unwrap())
            .collect();
        let (auto, level) = match fields[1] {
            "0" => (false, 0),
            "1" => (false, 1),
            _ => (true, 0),
        };
        let want_levels: Vec<Option<u8>> = fields[3]
            .split_whitespace()
            .map(|t| t.parse().ok())
            .collect();
        let want_order: Vec<usize> = fields[4]
            .split_whitespace()
            .map(|t| t.parse().unwrap())
            .collect();
        let classes: Vec<(u32, Class)> = cps.iter().map(|&cp| (cp, class(cp))).collect();
        let (levels, paragraph, order) = run(&chars(&classes), auto, level);
        if levels != want_levels
            || order != want_order
            || paragraph != fields[2].parse::<u8>().unwrap()
        {
            failures.push(line);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

const EXAMPLES: &str = "\
05D0 05D1 0028 05D2 05D3 005B 0026 0065 0066 005D 002E 0029 0067 0068;0;0;1 1 0 1 1 0 0 0 0 0 0 0 0 0;1 0 2 4 3 5 6 7 8 9 10 11 12 13\n\
05D0 05D1 0028 05D2 05D3 005B 0026 0065 0066 005D 002E 0029 0067 0068;1;1;1 1 1 1 1 1 1 2 2 1 1 1 2 2;12 13 11 10 9 7 8 6 5 4 3 2 1 0\n\
0061 0062 0063 0020 0028 0064 0065 0066 0020 0627 0628 062C 0029 0020 05D0 05D1 05D2;0;0;0 0 0 0 0 0 0 0 0 1 1 1 0 0 1 1 1;0 1 2 3 4 5 6 7 8 11 10 9 12 13 16 15 14\n\
0061 0062 0063 0020 0028 0064 0065 0066 0020 0627 0628 062C 0029 0020 05D0 05D1 05D2;1;1;2 2 2 1 1 2 2 2 1 1 1 1 1 1 1 1 1;16 15 14 13 12 11 10 9 8 5 6 7 4 3 0 1 2\n\
05D0 05D1 05D2 0020 0028 0064 0065 0066 0020 0627 0628 062C 0029 0020 0061 0062 0063;0;0;1 1 1 0 0 0 0 0 0 1 1 1 0 0 0 0 0;2 1 0 3 4 5 6 7 8 11 10 9 12 13 14 15 16\n\
05D0 05D1 05D2 0020 0028 0064 0065 0066 0020 0627 0628 062C 0029 0020 0061 0062 0063;1;1;1 1 1 1 1 2 2 2 1 1 1 1 1 1 2 2 2;14 15 16 13 12 11 10 9 8 5 6 7 4 3 2 1 0\n\
0061 0062 0063 0020 0028 0627 0628 062C 0020 0064 0065 0066 0029 0020 05D0 05D1 05D2;0;0;0 0 0 0 0 1 1 1 0 0 0 0 0 0 1 1 1;0 1 2 3 4 7 6 5 8 9 10 11 12 13 16 15 14\n\
0061 0062 0063 0020 0028 0627 0628 062C 0020 0064 0065 0066 0029 0020 05D0 05D1 05D2;1;1;2 2 2 1 1 1 1 1 1 2 2 2 1 1 1 1 1;16 15 14 13 12 9 10 11 8 7 6 5 4 3 0 1 2\n\
05D0 05D1 05D2 0020 0028 0627 0628 062C 0020 0064 0065 0066 0029 0020 0061 0062 0063;0;0;1 1 1 0 0 1 1 1 0 0 0 0 0 0 0 0 0;2 1 0 3 4 7 6 5 8 9 10 11 12 13 14 15 16\n\
05D0 05D1 05D2 0020 0028 0627 0628 062C 0020 0064 0065 0066 0029 0020 0061 0062 0063;1;1;1 1 1 1 1 1 1 1 1 2 2 2 1 1 2 2 2;14 15 16 13 12 9 10 11 8 7 6 5 4 3 2 1 0\n\
0627 0628 062C 0020 0062 006F 006F 006B 0028 0073 0029;0;0;1 1 1 0 0 0 0 0 0 0 0;2 1 0 3 4 5 6 7 8 9 10\n\
0627 0628 062C 0020 0062 006F 006F 006B 0028 0073 0029;1;1;1 1 1 1 2 2 2 2 2 2 2;4 5 6 7 8 9 10 3 2 1 0\n\
202E 0061 202A 0062 202C 2066 0063 2069 202A 0064 202C 0065 202C;2;0;x 1 x 2 x 1 2 1 x 2 x 1 x;11 9 7 6 5 3 1\n\
202E 0061 202A 0062 202C 2066 0063 2069 202A 0064 202C 0065 202C;1;1;x 3 x 4 x 3 4 3 x 4 x 3 x;11 9 7 6 5 3 1\n\
202D 05D0 202B 05D1 202C 2068 05D2 2069 202B 05D3 202C 05D4 202C;2;1;x 2 x 3 x 2 3 2 x 3 x 2 x;1 3 5 6 7 9 11\n\
202D 0661 202B 0662 202C 2068 0663 2069 202B 0664 202C 0665 202C;0;0;x 2 x 4 x 2 6 2 x 4 x 2 x;1 3 5 6 7 9 11\n\
202A 05D0 0028 05D1 202C 202D 0029;2;1;x 3 3 3 x x 2;3 2 1 6\n\
202A 05D0 0028 05D1 202C 202D 0029 202C;2;1;x 3 3 3 x x 2 x;3 2 1 6\n\
202B 0061 0028 0062 202C 202E 0029;2;0;x 2 2 2 x x 1;6 1 2 3\n\
202B 0061 0028 0062 202C 202E 0029 202C;2;0;x 2 2 2 x x 1 x;6 1 2 3\n\
202A 202E 0061 202C 0028 05D0 202C 202D 0029 202C;2;0;x x 3 x 3 3 x x 2 x;5 4 2 8\n\
202B 202D 05D0 202C 0028 0061 202C 202E 0029 202C;2;1;x x 4 x 4 4 x x 3 x;8 2 4 5\n\
202A 202E 0061 202C 0028 005B 05D0 202C 202D 005D 0029 202C;2;0;x x 3 x 3 3 3 x x 2 2 x;6 5 4 2 9 10\n\
202B 202D 05D0 202C 0028 005B 0061 202C 202E 005D 0029 202C;2;1;x x 4 x 4 4 4 x x 3 3 x;10 9 2 4 5 6\n\
202D 0028 202C 202A 05D0 0029 05D1;2;1;x 2 x x 3 3 3;1 6 5 4\n\
202D 0028 202C 202A 05D0 0029 05D1 202C;2;1;x 2 x x 3 3 3 x;1 6 5 4\n\
202E 0028 202C 202B 0061 0029 0062;2;0;x 1 x x 2 2 2;4 5 6 1\n\
202E 0028 202C 202B 0061 0029 0062 202C;2;0;x 1 x x 2 2 2 x;4 5 6 1\n\
202D 202E 0061 202C 0028 202C 202A 05D0 0029 05D1;2;0;x x 3 x 2 x x 3 3 3;2 4 9 8 7\n\
202E 202D 05D0 202C 0028 202C 202B 0061 0029 0062;2;1;x x 4 x 3 x x 4 4 4;7 8 9 4 2\n\
202D 202E 0061 202C 0028 005B 202C 202A 05D0 005D 0029 05D1;2;0;x x 3 x 2 2 x x 3 3 3 3;2 4 5 11 10 9 8\n\
202E 202D 05D0 202C 0028 005B 202C 202B 0061 005D 0029 0062;2;1;x x 4 x 3 3 x x 4 4 4 4;8 9 10 11 5 4 2\n\
0061 0028 0062 0029 0331;1;1;2 2 2 2 2;0 1 2 3 4\n\
0061 0028 0332 0062 0029 0333;1;1;2 2 2 2 2 2;0 1 2 3 4 5\n\
05D0 0028 05D1 0029 0331;0;0;1 1 1 1 1;4 3 2 1 0\n\
05D0 0028 0332 05D1 0029 0333;0;0;1 1 1 1 1 1;5 4 3 2 1 0\n\
0661 0028 0662 0029 0331;0;0;2 1 2 1 1;4 3 2 1 0\n\
0661 0028 0332 0662 0029 0333;0;0;2 1 1 2 1 1;5 4 3 2 1 0\n\
0061 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0062 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029;1;1;2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2;0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95 96 97 98 99 100 101 102 103 104 105 106 107 108 109 110 111 112 113 114 115 116 117 118 119 120 121 122 123 124 125\n\
0061 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0028 0062 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029 0029;1;1;2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2;0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64 65 66 67 68 69 70 71 72 73 74 75 76 77 78 79 80 81 82 83 84 85 86 87 88 89 90 91 92 93 94 95 96 97 98 99 100 101 102 103 104 105 106 107 108 109 110 111 112 113 114 115 116 117 118 119 120 121 122 123 124 125 126 127
";
