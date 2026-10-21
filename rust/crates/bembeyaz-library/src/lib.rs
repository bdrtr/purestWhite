//! Text for the Library index, as lib/LibraryIndex's `LibraryText` describes it (and
//! `test/library_text` pins): the fold used for search and sort keys, the display name of a
//! person, the order-insensitive author key, query matching and the surname sort key.
//!
//! The fold decomposes rather than composes, so precomposed and decomposed text fold alike, and
//! maps the letters that have no decomposition (ø, æ, ß, ł, ...) explicitly. Every result is
//! appended to a caller's `Vec<u8>`, failing with [`OutOfMemory`] rather than aborting.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

use alloc::vec::Vec;
use bembeyaz_utf8 as utf8;

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;

/// Longest author key, in bytes.
pub const AUTHOR_KEY_MAX_BYTES: usize = 12;

/// A result could not be held.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct OutOfMemory;

fn push(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), OutOfMemory> {
    out.try_reserve(bytes.len()).map_err(|_| OutOfMemory)?;
    out.extend_from_slice(bytes);
    Ok(())
}

/// Letters with no canonical decomposition, and the apostrophes kept so "O'Brien" stays one word.
const EXPLICIT_MAP: [(u32, &[u8]); 19] = [
    (0x00D8, b"o"),
    (0x00F8, b"o"),
    (0x00C6, b"ae"),
    (0x00E6, b"ae"),
    (0x0152, b"oe"),
    (0x0153, b"oe"),
    (0x00DF, b"ss"),
    (0x0141, b"l"),
    (0x0142, b"l"),
    (0x00D0, b"d"),
    (0x00F0, b"d"),
    (0x0110, b"d"),
    (0x0111, b"d"),
    (0x00DE, b"th"),
    (0x00FE, b"th"),
    (0x0131, b"i"),
    (0x0027, b"'"),
    (0x2019, b"'"),
    (0x2018, b"'"),
];

fn explicit_mapping(cp: u32) -> Option<&'static [u8]> {
    EXPLICIT_MAP.iter().find(|(c, _)| *c == cp).map(|(_, r)| *r)
}

/// Fully decomposed: é -> e, and the rare doubly accented forms (ế -> ê -> e).
fn strip_diacritics(mut cp: u32) -> u32 {
    for _ in 0..4 {
        match utf8::decomposed_base(cp) {
            Some(base) => cp = base,
            None => break,
        }
    }
    cp
}

fn is_ascii_alnum(cp: u32) -> bool {
    u8::try_from(cp).is_ok_and(|c| c.is_ascii_alphanumeric())
}

type Range = (u32, u32);

fn in_ranges(cp: u32, ranges: &[Range]) -> bool {
    ranges
        .iter()
        .any(|&(first, last)| (first..=last).contains(&cp))
}

/// Scripts the SD-font catalog covers.
const LETTER_RANGES: &[Range] = &[
    (0x0041, 0x005A),
    (0x0061, 0x007A),
    (0x00C0, 0x02AF),
    (0x0370, 0x03FF),
    (0x1F00, 0x1FFF),
    (0x0400, 0x0481),
    (0x048A, 0x052F),
    (0x0531, 0x0556),
    (0x0560, 0x0588),
    (0x05D0, 0x05EA),
    (0x05EF, 0x05F2),
    (0xFB1D, 0xFB4F),
    (0x0620, 0x064A),
    (0x066E, 0x066F),
    (0x0671, 0x06D3),
    (0x06D5, 0x06D5),
    (0x06E5, 0x06E6),
    (0x06EE, 0x06EF),
    (0x06FA, 0x06FC),
    (0x06FF, 0x06FF),
    (0x0750, 0x077F),
    (0x08A0, 0x08C9),
    (0xFB50, 0xFDF9),
    (0xFE70, 0xFEFC),
    (0x10A0, 0x10C5),
    (0x10D0, 0x10FF),
    (0x1C90, 0x1CBF),
    (0x2D00, 0x2D25),
    (0x1200, 0x135A),
    (0x1380, 0x138F),
    (0x2D80, 0x2DDE),
    (0x13A0, 0x13F5),
    (0x13F8, 0x13FD),
    (0xAB70, 0xABBF),
    (0x2D30, 0x2D67),
    (0x3041, 0x3096),
    (0x30A1, 0x30FA),
    (0x3105, 0x312F),
    (0x31A0, 0x31BF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xF900, 0xFAFF),
    (0x1100, 0x11FF),
    (0x3131, 0x318E),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7FF),
    (0x20000, 0x2EBEF),
    (0x2F800, 0x2FA1F),
    (0x30000, 0x323AF),
];

const NUMBER_RANGES: &[Range] = &[
    (0x0030, 0x0039),
    (0x0660, 0x0669),
    (0x06F0, 0x06F9),
    (0x0966, 0x096F),
    (0xFF10, 0xFF19),
];

const MARK_RANGES: &[Range] = &[
    (0x0483, 0x0489),
    (0x0591, 0x05BD),
    (0x05BF, 0x05BF),
    (0x05C1, 0x05C2),
    (0x05C4, 0x05C5),
    (0x05C7, 0x05C7),
    (0x0610, 0x061A),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x06DF, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x08D3, 0x08FF),
    (0xFB1E, 0xFB1E),
];

fn is_number(cp: u32) -> bool {
    in_ranges(cp, NUMBER_RANGES)
}

fn is_mark(cp: u32) -> bool {
    utf8::is_combining_mark(cp) || in_ranges(cp, MARK_RANGES)
}

fn is_letter(cp: u32) -> bool {
    if is_mark(cp)
        || is_number(cp)
        || matches!(
            cp,
            0x00D7 | 0x00F7 | 0x0374 | 0x0375 | 0x037E | 0x0387 | 0x03F6
        )
    {
        return false;
    }
    in_ranges(cp, LETTER_RANGES)
}

/// Casefolds and strips diacritics for matching and sorting, appending to `out`: Latin accents
/// decomposed, the [`EXPLICIT_MAP`] letters mapped, ASCII lowercased, other letters and numbers
/// kept, marks dropped, and every other run (punctuation, symbols, unmapped scripts) one space,
/// none at the ends. Stops at a NUL or at a sequence the text cuts short.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn fold(text: &[u8], out: &mut Vec<u8>) -> Result<(), OutOfMemory> {
    let start = out.len();
    // The fold never outgrows its input.
    out.try_reserve(text.len()).map_err(|_| OutOfMemory)?;
    let mut rest = text;
    let mut pending_space = false;
    let space = |out: &mut Vec<u8>, pending: &mut bool| {
        let result = if *pending && out.len() > start {
            push(out, b" ")
        } else {
            Ok(())
        };
        *pending = false;
        result
    };
    while let Some(&lead) = rest.first() {
        if utf8::sequence_len(lead) > rest.len() {
            break;
        }
        let (cp, used) = utf8::decode(rest);
        if used == 0 {
            break;
        }
        rest = rest.get(used..).unwrap_or_default();
        if is_mark(cp) {
            continue;
        }
        if let Some(mapped) = explicit_mapping(cp) {
            space(out, &mut pending_space)?;
            push(out, mapped)?;
            continue;
        }
        let base = strip_diacritics(cp);
        if is_ascii_alnum(base) {
            space(out, &mut pending_space)?;
            #[allow(clippy::cast_possible_truncation)]
            push(out, &[(base as u8).to_ascii_lowercase()])?;
            continue;
        }
        if base >= 0x80 && (is_letter(base) || is_number(base)) {
            space(out, &mut pending_space)?;
            let mut buf = [0; 4];
            push(out, utf8::encode(base, &mut buf))?;
            continue;
        }
        if out.len() > start {
            pending_space = true;
        }
    }
    Ok(())
}

/// The first letter of a folded key, or 0 when it starts with anything else (the shared `#`
/// group).
#[must_use]
pub fn folded_group_initial(folded: &[u8]) -> u32 {
    let (cp, _) = utf8::decode(folded);
    if is_letter(cp) { cp } else { 0 }
}

/// A person's name tidied for display (see `cleanPersonName` in LibraryText.h), appended to
/// `out`.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn clean_person_name(author: &[u8], out: &mut Vec<u8>) -> Result<(), OutOfMemory> {
    let mut kept: Vec<u8> = Vec::new();
    kept.try_reserve(author.len()).map_err(|_| OutOfMemory)?;
    let mut depth = 0u32;
    for (i, &c) in author.iter().enumerate() {
        match c {
            b'[' | b'(' => depth += 1,
            b']' | b')' => depth = depth.saturating_sub(1),
            // Secondary authors.
            b';' => break,
            _ if depth > 0 => {}
            b'_' => {
                // "Herbert G_ Wells": between letters it stood for a full stop.
                let between = i > 0
                    && author.get(i - 1).is_some_and(u8::is_ascii_alphabetic)
                    && author.get(i + 1).is_some_and(u8::is_ascii_alphabetic);
                kept.push(if between { b'.' } else { b' ' });
            }
            _ => kept.push(c),
        }
    }
    while kept
        .last()
        .is_some_and(|c| matches!(c, b' ' | b',' | b'-' | b'.' | b'_'))
    {
        kept.pop();
    }
    let lead = kept.iter().take_while(|&&c| c == b' ').count();

    // Collapse the space runs the removals left.
    let mut collapsed: Vec<u8> = Vec::new();
    collapsed.try_reserve(kept.len()).map_err(|_| OutOfMemory)?;
    let mut space = false;
    for &c in kept.get(lead..).unwrap_or_default() {
        if c == b' ' {
            space = true;
            continue;
        }
        if space && !collapsed.is_empty() {
            collapsed.push(b' ');
        }
        space = false;
        collapsed.push(c);
    }

    // A single comma: "Austen, Jane" turned round. Several are left as written.
    let mut commas = collapsed.iter().enumerate().filter(|&(_, &c)| c == b',');
    if let (Some((comma, _)), None) = (commas.next(), commas.next()) {
        let last = collapsed.get(..comma).unwrap_or_default();
        let first = collapsed.get(comma + 1..).unwrap_or_default();
        let first = first
            .get(first.iter().take_while(|&&c| c == b' ').count()..)
            .unwrap_or_default();
        let last = last
            .get(..last.len() - last.iter().rev().take_while(|&&c| c == b' ').count())
            .unwrap_or_default();
        if !first.is_empty() && !last.is_empty() {
            push(out, first)?;
            push(out, b" ")?;
            return push(out, last);
        }
    }
    push(out, &collapsed)
}

fn is_single_codepoint(token: &[u8]) -> bool {
    let (_, used) = utf8::decode(token);
    !token.is_empty() && used == token.len()
}

/// An order-insensitive key for one person, at most [`AUTHOR_KEY_MAX_BYTES`], appended to `out`:
/// the cleaned, folded name's words of more than one codepoint (at most the first 12 words),
/// sorted and joined, cut at a UTF-8 boundary.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn author_key(author: &[u8], out: &mut Vec<u8>) -> Result<(), OutOfMemory> {
    const MAX_TOKENS: usize = 12;
    let mut cleaned = Vec::new();
    clean_person_name(author, &mut cleaned)?;
    let mut folded = Vec::new();
    fold(&cleaned, &mut folded)?;

    let mut tokens: [&[u8]; MAX_TOKENS] = [&[]; MAX_TOKENS];
    let mut count = 0;
    for token in folded.split(|&c| c == b' ').filter(|t| !t.is_empty()) {
        let Some(slot) = tokens.get_mut(count) else {
            break;
        };
        *slot = token;
        count += 1;
    }
    let mut kept = 0;
    for i in 0..count {
        let token = tokens.get(i).copied().unwrap_or_default();
        if !is_single_codepoint(token)
            && let Some(slot) = tokens.get_mut(kept)
        {
            *slot = token;
            kept += 1;
        }
    }
    let kept = tokens.get_mut(..kept).unwrap_or_default();
    kept.sort_unstable();

    let mut key: Vec<u8> = Vec::new();
    for token in kept.iter() {
        if !key.is_empty() {
            push(&mut key, b" ")?;
        }
        push(&mut key, token)?;
    }
    // A byte cut, not a word cut: the sort puts a short forename first, so a word cut would
    // reduce "Wollstonecraft, Mary" to "mary" and merge every Mary.
    if key.len() > AUTHOR_KEY_MAX_BYTES {
        let len = utf8::safe_truncate_len(key.get(..AUTHOR_KEY_MAX_BYTES).unwrap_or_default());
        key.truncate(len);
    }
    while key.last() == Some(&b' ') {
        key.pop();
    }
    push(out, &key)
}

/// fold keeps apostrophes for sorting; for searching the word after one ("eneide" in
/// "l'eneide") is worth typing, so it breaks words here.
fn is_word_break(c: u8) -> bool {
    c == b' ' || c == b'\''
}

/// Whether every word of the folded `needle` prefixes some word of the folded `haystack`. An
/// empty needle matches.
#[must_use]
pub fn matches_query(haystack: &[u8], needle: &[u8]) -> bool {
    needle
        .split(|&c| is_word_break(c))
        .filter(|w| !w.is_empty())
        .all(|word| {
            haystack
                .split(|&c| is_word_break(c))
                .filter(|w| !w.is_empty())
                .any(|h| h.starts_with(word))
        })
}

/// A shelf order key for a display name, appended to `out`: the folded last word, then the rest.
///
/// # Errors
///
/// When `out` cannot grow.
pub fn surname_key(display_author: &[u8], out: &mut Vec<u8>) -> Result<(), OutOfMemory> {
    let mut folded = Vec::new();
    fold(display_author, &mut folded)?;
    match folded.iter().rposition(|&c| c == b' ') {
        None => push(out, &folded),
        Some(sep) => {
            push(out, folded.get(sep + 1..).unwrap_or_default())?;
            push(out, b" ")?;
            push(out, folded.get(..sep).unwrap_or_default())
        }
    }
}
