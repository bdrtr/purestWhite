//! Where a word may break at a line end: at hyphens and apostrophes it already has, at the
//! breaks of Liang's patterns for the book's language, and, as a last resort, anywhere.
//!
//! Behaviour matches the C++ hyphenator this replaces (`lib/Epub/Epub/hyphenation` up to
//! 46ebf6e), which `test/hyphenation_eval` pins. The patterns are typst/hypher's tries.
//!
//! The words come from books: nothing here panics. A word's codepoints stay on the stack up to
//! 70 (the patterns look at 68 letters at most); a longer word takes two fallible heap blocks of
//! 4 bytes per codepoint. The caller bounds the word (`WordStore` keeps at most 0xFFFE bytes).
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

mod languages;
mod liang;
mod text;

pub use languages::{Language, language};
pub use text::is_explicit_hyphen;

use text::{Word, is_alphabetic, is_apostrophe, is_cjk_breakable, is_separator, trim};

/// A place a word may break.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Break {
    /// Where the text after the break starts, in bytes into the word.
    pub offset: usize,
    /// A hyphen must be drawn at the line end: false where the word already shows one (or an
    /// apostrophe), and between CJK characters.
    pub inserted_hyphen: bool,
}

/// Minimum letters before and after a break when no language is set.
const DEFAULT_MIN: usize = 2;

/// Calls `emit` for each place `word` may break, in order.
///
/// Explicit hyphens between letters break, and then the parts between hyphens and apostrophes
/// break by the patterns. Apostrophes between letters break when three letters or more stand
/// on each side. A word with neither breaks by the patterns.
///
/// With `fallback`, where the patterns find nothing, the word breaks before every character
/// that leaves the minimum on each side: the whole word, or each part of a word with
/// apostrophes (only with a language). Words with explicit hyphens get no fallback.
pub fn breaks(
    word: &[u8],
    language: Option<Language>,
    fallback: bool,
    mut emit: impl FnMut(Break),
) {
    if word.is_empty() {
        return;
    }
    let bytes = word;
    let mut word = Word::new();
    if !word.read(bytes) {
        return;
    }
    let (lo, hi) = trim(word.values());
    let cps = word.values().get(lo..hi).unwrap_or_default();
    let offsets = word.offsets().get(lo..hi).unwrap_or_default();
    let offset = |i: usize| offsets.get(i).map_or(0, |&o| o as usize);

    let explicit = |i: usize| -> Option<bool> {
        // A hyphen at i - 1 between two letters.
        let hyphen = *cps.get(i.checked_sub(1)?)?;
        let letters = is_alphabetic(*cps.get(i.checked_sub(2)?)?) && is_alphabetic(*cps.get(i)?);
        (text::is_explicit_hyphen(hyphen) && letters).then_some(text::is_soft_hyphen(hyphen))
    };
    let has_explicit = (2..cps.len()).any(|i| explicit(i).is_some());
    let has_apostrophe = cps.iter().any(|&cp| is_apostrophe(cp));

    if has_explicit || has_apostrophe {
        // Patterns per part; fallback only without explicit hyphens.
        let fallback = fallback && !has_explicit;
        let mut start = 0;
        for end in (0..=cps.len()).filter(|&i| cps.get(i).is_none_or(|&cp| is_separator(cp))) {
            if start > 0 {
                let from_hyphen = if has_explicit { explicit(start) } else { None };
                let from_apostrophe = (has_apostrophe && contraction(cps, start)).then_some(false);
                if let Some(inserted_hyphen) = from_hyphen.or(from_apostrophe) {
                    emit(Break {
                        offset: offset(start),
                        inserted_hyphen,
                    });
                }
            }
            if let (Some(language), Some(part)) = (language, cps.get(start..end))
                && !part.is_empty()
            {
                let mut found = language.pattern_breaks(part);
                if fallback && matches!(found, Breaks::None) {
                    found = every(language.minima());
                }
                for_each_break(found, part.len(), |i| {
                    emit(Break {
                        offset: offset(start + i),
                        inserted_hyphen: true,
                    });
                });
            }
            start = end + 1;
        }
        return;
    }

    let mut found = language.map_or(Breaks::None, |language| language.pattern_breaks(cps));
    if fallback && matches!(found, Breaks::None) {
        found = every(language.map_or((DEFAULT_MIN, DEFAULT_MIN), Language::minima));
    }
    for_each_break(found, cps.len(), |i| {
        let cjk = cps.get(i).is_some_and(|&cp| is_cjk_breakable(cp))
            || i.checked_sub(1)
                .and_then(|p| cps.get(p))
                .is_some_and(|&cp| is_cjk_breakable(cp));
        emit(Break {
            offset: offset(i),
            inserted_hyphen: !cjk,
        });
    });
}

/// The breaks inside one run of characters.
#[derive(Clone, Copy)]
enum Breaks {
    None,
    /// Bit `i`: before character `i`.
    Patterns(u128),
    /// Before every character with these many before and after it.
    Every {
        min_prefix: usize,
        min_suffix: usize,
    },
}

/// Every break that leaves `(min_prefix, min_suffix)` characters on each side.
fn every((min_prefix, min_suffix): (usize, usize)) -> Breaks {
    Breaks::Every {
        min_prefix,
        min_suffix,
    }
}

fn for_each_break(breaks: Breaks, len: usize, mut f: impl FnMut(usize)) {
    match breaks {
        Breaks::None => {}
        Breaks::Patterns(mut bits) => {
            while bits != 0 {
                f(bits.trailing_zeros() as usize);
                bits &= bits - 1;
            }
        }
        Breaks::Every {
            min_prefix,
            min_suffix,
        } => {
            for i in min_prefix..=len.saturating_sub(min_suffix) {
                if i + min_suffix <= len {
                    f(i);
                }
            }
        }
    }
}

/// An apostrophe at `start - 1` between letters, with three letters or more in the parts on
/// each side, so that `l'`, `d'`, `'ve` and `'ll` stay on their line.
fn contraction(cps: &[u32], start: usize) -> bool {
    const MIN_SIDE: usize = 3;
    let at = |i: Option<usize>| i.and_then(|i| cps.get(i)).copied();
    let Some(apostrophe) = start.checked_sub(1) else {
        return false;
    };
    if !at(Some(apostrophe)).is_some_and(is_apostrophe)
        || !at(apostrophe.checked_sub(1)).is_some_and(is_alphabetic)
        || !at(Some(start)).is_some_and(is_alphabetic)
    {
        return false;
    }
    let before = cps.get(..apostrophe).unwrap_or_default();
    let after = cps.get(start..).unwrap_or_default();
    letters(before.iter().rev()) >= MIN_SIDE && letters(after.iter()) >= MIN_SIDE
}

/// Letters up to the next hyphen or apostrophe.
fn letters<'a>(cps: impl Iterator<Item = &'a u32>) -> usize {
    cps.take_while(|&&cp| !is_separator(cp))
        .filter(|&&cp| is_alphabetic(cp))
        .count()
}

#[cfg(test)]
extern crate std;
#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    reason = "tests"
)]
mod tests;
