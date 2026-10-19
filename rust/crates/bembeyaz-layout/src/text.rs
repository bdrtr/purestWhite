//! Codepoint classes and the places a word is split into tokens.

use alloc::vec::Vec;

use bembeyaz_hyphenation::{is_explicit_hyphen, is_soft_hyphen};
use bembeyaz_utf8::{decode, is_cjk_breakable, is_combining_mark, is_hangul};

/// U+00AD in UTF-8.
pub(crate) const SOFT_HYPHEN: &[u8] = b"\xC2\xAD";

/// The codepoints of `bytes` with where each ends, up to the end or a NUL.
pub(crate) fn codepoint_ends(bytes: &[u8]) -> impl Iterator<Item = (u32, usize)> + '_ {
    let mut at = 0;
    core::iter::from_fn(move || {
        let (cp, len) = decode(bytes.get(at..)?);
        if len == 0 {
            return None;
        }
        at += len;
        Some((cp, at))
    })
}

/// The first codepoint drawn: soft hyphens skipped, 0 for none.
pub(crate) fn first_codepoint(word: &[u8]) -> u32 {
    codepoint_ends(word)
        .map(|(cp, _)| cp)
        .find(|&cp| cp != 0xAD)
        .unwrap_or(0)
}

/// The last codepoint, read from the start of the last UTF-8 sequence.
pub(crate) fn last_codepoint(word: &[u8]) -> u32 {
    let mut i = word.len().saturating_sub(1);
    while i > 0 && word.get(i).is_some_and(|&b| b & 0xC0 == 0x80) {
        i -= 1;
    }
    decode(word.get(i..).unwrap_or_default()).0
}

pub(crate) fn count_codepoints(text: &[u8]) -> u32 {
    let mut count = 0u32;
    let mut at = 0;
    while at < text.len() {
        let (_, len) = decode(text.get(at..).unwrap_or_default());
        at += len.max(1);
        count = count.wrapping_add(1);
    }
    count
}

pub(crate) fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// `word` without its soft hyphens, appended to `out`.
pub(crate) fn push_without_soft_hyphens(out: &mut Vec<u8>, word: &[u8]) {
    let mut rest = word;
    while let Some(at) = rest.windows(2).position(|w| w == SOFT_HYPHEN) {
        out.extend_from_slice(rest.get(..at).unwrap_or_default());
        rest = rest.get(at + 2..).unwrap_or_default();
    }
    out.extend_from_slice(rest);
}

/// Hebrew, Arabic or Syriac lead bytes: a word that may start right to left.
pub(crate) fn may_contain_rtl_bytes(word: &[u8]) -> bool {
    word.iter().any(|&b| (0xD6..=0xDB).contains(&b))
}

/// A visible hyphen or dash after which a line may break: not U+00AD (drawn only when its break
/// is taken) and not U+2011 (whose purpose is to forbid one).
pub(crate) fn allows_break_after_explicit_hyphen(cp: u32) -> bool {
    is_explicit_hyphen(cp) && !is_soft_hyphen(cp) && cp != 0x2011
}

/// A continuation forbids a break, unless it also has no space before it.
pub(crate) fn allows_break(continues: bool, no_space_before: bool) -> bool {
    !continues || no_space_before
}

/// Gaps that justification stretches: word gaps and CJK gaps, and a literal space token.
pub(crate) fn is_justifiable_gap(continues: bool, no_space_before: bool, is_space: bool) -> bool {
    !continues || (is_space && !no_space_before)
}

pub(crate) fn ends_with_breakable_hyphen(token: &[u8]) -> bool {
    !token.is_empty() && allows_break_after_explicit_hyphen(last_codepoint(token))
}

fn is_no_break_before_cjk_punctuation(cp: u32) -> bool {
    allows_break_after_explicit_hyphen(cp)
        || matches!(
            cp,
            0x2E | 0x2C
                | 0x3A
                | 0x3B
                | 0x21
                | 0x3F
                | 0x29
                | 0x5D
                | 0x7D
                | 0x00BB
                | 0x2019
                | 0x201D
                | 0x3001
                | 0x3002
                | 0x3009
                | 0x300B
                | 0x300D
                | 0x300F
                | 0x3011
                | 0x3015
                | 0x3017
                | 0x3019
                | 0x301B
                | 0xFF01
                | 0xFF09
                | 0xFF0C
                | 0xFF0E
                | 0xFF1A
                | 0xFF1B
                | 0xFF1F
                | 0xFF3D
                | 0xFF5D
        )
}

fn is_no_break_after_cjk_punctuation(cp: u32) -> bool {
    matches!(
        cp,
        0x28 | 0x5B
            | 0x7B
            | 0x00AB
            | 0x2018
            | 0x201C
            | 0x3008
            | 0x300A
            | 0x300C
            | 0x300E
            | 0x3010
            | 0x3014
            | 0x3016
            | 0x3018
            | 0x301A
            | 0xFF08
            | 0xFF3B
            | 0xFF5B
    )
}

fn cjk_boundary_allows_break(left: u32, right: u32) -> bool {
    if allows_break_after_explicit_hyphen(left) {
        return !is_no_break_before_cjk_punctuation(right);
    }
    if !is_cjk_breakable(left) && !is_cjk_breakable(right) {
        return false;
    }
    if is_no_break_after_cjk_punctuation(left) || is_no_break_before_cjk_punctuation(right) {
        return false;
    }
    !is_combining_mark(right)
}

/// Korean separates words with spaces, so a boundary touching Hangul is not a gap-less break
/// inside a line; `hangul_line_end_breaks` still splits a Hangul word at a line end.
pub(crate) fn has_cjk_break_opportunity_between(left: u32, right: u32) -> bool {
    if allows_break_after_explicit_hyphen(left) {
        return cjk_boundary_allows_break(left, right);
    }
    if is_hangul(left) || is_hangul(right) {
        return false;
    }
    cjk_boundary_allows_break(left, right)
}

/// Where a Hangul word may split at a line end (no hyphen is drawn); false when out of memory.
pub(crate) fn hangul_line_end_breaks(word: &[u8], out: &mut Vec<(usize, bool)>) -> bool {
    let mut cps = codepoint_ends(word);
    let Some((mut prev, mut offset)) = cps.next() else {
        return true;
    };
    for (cur, end) in cps {
        if (is_hangul(prev) || is_hangul(cur)) && cjk_boundary_allows_break(prev, cur) {
            if out.try_reserve(1).is_err() {
                return false;
            }
            out.push((offset, false));
        }
        prev = cur;
        offset = end;
    }
    true
}

pub(crate) fn contains_cjk_breakable(text: &[u8]) -> bool {
    codepoint_ends(text).any(|(cp, _)| is_cjk_breakable(cp))
}

/// Byte offsets inside `text` where its CJK characters may break, or none when it has none;
/// false when out of memory.
pub(crate) fn cjk_character_break_offsets(text: &[u8], out: &mut Vec<usize>) -> bool {
    out.clear();
    if !contains_cjk_breakable(text) {
        return true;
    }
    let mut cps = codepoint_ends(text);
    let Some((mut prev, mut end)) = cps.next() else {
        return true;
    };
    for (cur, next_end) in cps {
        if has_cjk_break_opportunity_between(prev, cur) {
            if out.try_reserve(1).is_err() {
                return false;
            }
            out.push(end);
        }
        prev = cur;
        end = next_end;
    }
    true
}

/// Byte offsets right after each explicit hyphen or dash run that has text before (or, with
/// `attached`, a word before) and text after it; false when out of memory.
pub(crate) fn explicit_hyphen_break_offsets(
    text: &[u8],
    attached: bool,
    out: &mut Vec<usize>,
) -> bool {
    out.clear();
    let mut leading = attached;
    let mut at = 0;
    while at < text.len() {
        let (cp, len) = decode(text.get(at..).unwrap_or_default());
        if len == 0 {
            break;
        }
        at += len;
        if allows_break_after_explicit_hyphen(cp) {
            while at < text.len() {
                let (next, next_len) = decode(text.get(at..).unwrap_or_default());
                if next_len == 0 || !allows_break_after_explicit_hyphen(next) {
                    break;
                }
                at += next_len;
            }
            if leading && at < text.len() {
                if out.try_reserve(1).is_err() {
                    return false;
                }
                out.push(at);
            }
        }
        leading = true;
    }
    true
}

/// A letter for Focus Reading: Latin letters and apostrophes, most non-ASCII letters, not
/// punctuation, dashes or symbols.
pub(crate) fn is_word_character(cp: u32) -> bool {
    if cp < 128 {
        let lower = cp | 0x20;
        return (u32::from(b'a')..=u32::from(b'z')).contains(&lower) || cp == u32::from(b'\'');
    }
    if (0x2000..=0x2BFF).contains(&cp) {
        return cp == 0x2018 || cp == 0x2019;
    }
    if (0x00A1..=0x00BF).contains(&cp) {
        return cp == 0x00AA || cp == 0x00B5 || cp == 0x00BA;
    }
    if (0x2E00..=0x2E7F).contains(&cp) {
        return false;
    }
    !matches!(cp, 0x02D7 | 0xFE63 | 0xFF0D)
}

/// The rubyless-ideograph test ruby widths use.
pub(crate) fn is_cjk_ideograph(cp: u32) -> bool {
    matches!(cp, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0xF900..=0xFAFF | 0x2_0000..=0x3_FFFF)
}
