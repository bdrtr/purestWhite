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
