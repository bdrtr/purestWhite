//! UTF-8 helpers for device text: lenient decoding, NFC composition for the
//! Latin/Vietnamese marks and Hangul jamo the fonts cannot position, and
//! codepoint-boundary truncation.
//!
//! Behaviour matches CrossPoint `lib/Utf8/Utf8.cpp` byte for byte, including
//! how malformed input decodes to U+FFFD and where a NUL byte ends the text.
#![no_std]
#![warn(clippy::pedantic)]

mod compose_table;

use compose_table::COMPOSE_TABLE;

pub const REPLACEMENT: u32 = 0xFFFD;

/// Byte length announced by a lead byte. Invalid leads count as 1.
#[must_use]
pub fn sequence_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        _ if lead >> 5 == 0x6 => 2,
        _ if lead >> 4 == 0xE => 3,
        _ if lead >> 3 == 0x1E => 4,
        _ => 1,
    }
}

/// Decodes the codepoint at the start of `bytes`, returning it with the number
/// of bytes consumed. An empty slice or a leading NUL yields `(0, 0)`.
///
/// Malformed input yields U+FFFD: a bad lead or an overlong, surrogate or
/// out-of-range value consumes one byte; a missing continuation byte consumes
/// the bytes read before it.
#[must_use]
pub fn decode(bytes: &[u8]) -> (u32, usize) {
    let Some(&lead) = bytes.first() else {
        return (0, 0);
    };
    if lead == 0 {
        return (0, 0);
    }
    let len = sequence_len(lead);
    if len == 1 {
        return if lead >= 0x80 {
            (REPLACEMENT, 1)
        } else {
            (u32::from(lead), 1)
        };
    }
    for i in 1..len {
        match bytes.get(i) {
            Some(&b) if b & 0xC0 == 0x80 => {}
            _ => return (REPLACEMENT, i),
        }
    }
    let mut cp = u32::from(lead) & ((1 << (7 - len)) - 1);
    for &b in &bytes[1..len] {
        cp = (cp << 6) | (u32::from(b) & 0x3F);
    }
    let overlong = match len {
        2 => cp < 0x80,
        3 => cp < 0x800,
        _ => cp < 0x10000,
    };
    if overlong || (0xD800..=0xDFFF).contains(&cp) || cp > 0x10_FFFF {
        return (REPLACEMENT, 1);
    }
    (cp, len)
}

/// Iterates the codepoints of `bytes` with [`decode`]'s lenient rules,
/// stopping at the end or at a NUL byte.
#[must_use]
pub fn codepoints(bytes: &[u8]) -> Codepoints<'_> {
    Codepoints { rest: bytes }
}

#[derive(Clone, Debug)]
pub struct Codepoints<'a> {
    rest: &'a [u8],
}

impl<'a> Codepoints<'a> {
    /// The bytes not yet decoded.
    #[must_use]
    pub fn as_bytes(&self) -> &'a [u8] {
        self.rest
    }
}

impl Iterator for Codepoints<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        let (cp, used) = decode(self.rest);
        if used == 0 {
            return None;
        }
        self.rest = &self.rest[used..];
        Some(cp)
    }
}

impl core::iter::FusedIterator for Codepoints<'_> {}

/// Encodes `cp` into `buf` and returns the used prefix (1-4 bytes).
/// Values are not validated, matching the C++ encoder.
#[allow(
    clippy::cast_possible_truncation,
    reason = "each byte is masked to fit"
)]
pub fn encode(cp: u32, buf: &mut [u8; 4]) -> &[u8] {
    let len = if cp < 0x80 {
        buf[0] = cp as u8;
        1
    } else if cp < 0x800 {
        buf[0] = (0xC0 | (cp >> 6)) as u8;
        buf[1] = (0x80 | (cp & 0x3F)) as u8;
        2
    } else if cp < 0x10000 {
        buf[0] = (0xE0 | (cp >> 12)) as u8;
        buf[1] = (0x80 | ((cp >> 6) & 0x3F)) as u8;
        buf[2] = (0x80 | (cp & 0x3F)) as u8;
        3
    } else {
        buf[0] = (0xF0 | (cp >> 18)) as u8;
        buf[1] = (0x80 | ((cp >> 12) & 0x3F)) as u8;
        buf[2] = (0x80 | ((cp >> 6) & 0x3F)) as u8;
        buf[3] = (0x80 | (cp & 0x3F)) as u8;
        4
    };
    &buf[..len]
}

/// Characters that break without a hyphen, as `utf8IsCjkBreakable` in lib/Utf8/Utf8.h: CJK
/// ideographs, kana, Hangul, CJK punctuation and fullwidth forms.
#[must_use]
pub fn is_cjk_breakable(cp: u32) -> bool {
    matches!(
        cp,
        0x1100..=0x11FF
            | 0x3000..=0x309F
            | 0x30A0..=0x30FF
            | 0x3130..=0x318F
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7FF
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF01..=0xFF60
            | 0xFF65..=0xFFEF
            | 0x2_0000..=0x2_A6DF
            | 0x2_A700..=0x2_B73F
    )
}

/// Hangul letters, as `utf8IsHangul` in lib/Utf8/Utf8.h.
#[must_use]
pub fn is_hangul(cp: u32) -> bool {
    matches!(cp, 0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xFFA0..=0xFFDC)
}

/// Combining diacritical marks that must not advance the pen.
#[must_use]
pub fn is_combining_mark(cp: u32) -> bool {
    matches!(cp, 0x0300..=0x036F | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F)
}

/// Canonical composition of `base` + `mark`, including algorithmic Hangul
/// LV and LVT pairs.
#[must_use]
pub fn compose_pair(base: u32, mark: u32) -> Option<u32> {
    if (0x1100..=0x1112).contains(&base) && (0x1161..=0x1175).contains(&mark) {
        return Some(0xAC00 + (base - 0x1100) * 588 + (mark - 0x1161) * 28);
    }
    if (0xAC00..=0xD7A3).contains(&base)
        && (base - 0xAC00).is_multiple_of(28)
        && (0x11A8..=0x11C2).contains(&mark)
    {
        return Some(base + mark - 0x11A7);
    }
    if !is_combining_mark(mark) {
        return None;
    }
    let key = (u16::try_from(base).ok()?, u16::try_from(mark).ok()?);
    COMPOSE_TABLE
        .binary_search_by_key(&key, |e| (e.base, e.mark))
        .ok()
        .map(|i| u32::from(COMPOSE_TABLE[i].composed))
}

/// The base letter a precomposed codepoint decomposes to ("é" -> "e").
/// Letters in their own right ("ø") have none.
#[must_use]
pub fn decomposed_base(cp: u32) -> Option<u32> {
    if cp < 0x00C0 {
        return None;
    }
    // Linear: the table is sorted by (base, mark), not by composed value.
    COMPOSE_TABLE
        .iter()
        .find(|e| u32::from(e.composed) == cp)
        .map(|e| u32::from(e.base))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compose {
    /// No composable sequence is possible; the input is already the result.
    Unchanged,
    /// The result was written through `emit`.
    Composed,
    /// `on_absorbed` returned false; discard what was emitted.
    Aborted,
}

/// Composes NFD sequences into precomposed codepoints, writing the result in
/// pieces through `emit`. Text stops at the first NUL byte.
///
/// `on_absorbed` receives the codepoint index of every input codepoint merged
/// into its predecessor.
pub fn compose_nfc(
    input: &[u8],
    mut on_absorbed: impl FnMut(u32) -> bool,
    mut emit: impl FnMut(&[u8]),
) -> Compose {
    // Composition needs a combining mark U+0300-036F (lead 0xCC/0xCD) or a
    // conjoining jamo (lead 0xE1). Plain ASCII and NFC text have neither.
    if !input.iter().any(|&b| matches!(b, 0xCC | 0xCD | 0xE1)) {
        return Compose::Unchanged;
    }

    let mut buf = [0u8; 4];
    let mut emit_cp = |cp: u32| emit(encode(cp, &mut buf));
    let mut base: Option<u32> = None;
    for (current, cp) in (0u32..).zip(codepoints(input)) {
        if let Some(composed) = base.and_then(|b| compose_pair(b, cp)) {
            if !on_absorbed(current) {
                return Compose::Aborted;
            }
            base = Some(composed);
        } else if is_combining_mark(cp) {
            if let Some(b) = base.take() {
                emit_cp(b);
            }
            emit_cp(cp);
        } else {
            if let Some(b) = base {
                emit_cp(b);
            }
            base = Some(cp);
        }
    }
    if let Some(b) = base {
        emit_cp(b);
    }
    Compose::Composed
}

/// Composes `buf` in place and returns the new length. Bytes that do not
/// compose, including malformed UTF-8, are kept unchanged so the text never
/// grows. Text stops at the first NUL byte.
pub fn compose_nfc_in_place(buf: &mut [u8]) -> usize {
    let mut read = 0;
    let mut write = 0;
    let mut base_start = 0;
    let mut base = 0u32;
    loop {
        let (cp, used) = decode(&buf[read..]);
        if used == 0 {
            break;
        }
        let start = read;
        read += used;
        if let Some(composed) = compose_pair(base, cp) {
            // Every supported composition is a BMP codepoint no longer than
            // the pair it replaces, so rewriting the base never passes `read`.
            let mut tmp = [0u8; 4];
            let bytes = encode(composed, &mut tmp);
            write = base_start + bytes.len();
            buf[base_start..write].copy_from_slice(bytes);
            base = composed;
        } else {
            base_start = write;
            buf.copy_within(start..read, write);
            write += used;
            base = if is_combining_mark(cp) { 0 } else { cp };
        }
    }
    write
}

/// Length of `buf` without a trailing incomplete sequence. A lone incomplete
/// sequence at the very start is kept.
#[must_use]
pub fn safe_truncate_len(buf: &[u8]) -> usize {
    let Some(mut lead) = buf.len().checked_sub(1) else {
        return 0;
    };
    while lead > 0 && buf[lead] & 0xC0 == 0x80 {
        lead -= 1;
    }
    if buf.len() - lead < sequence_len(buf[lead]) && lead > 0 {
        lead
    } else {
        buf.len()
    }
}

/// Length of `s` after removing its last codepoint.
#[must_use]
pub fn remove_last_char_len(s: &[u8]) -> usize {
    let Some(mut pos) = s.len().checked_sub(1) else {
        return 0;
    };
    while pos > 0 && s[pos] & 0xC0 == 0x80 {
        pos -= 1;
    }
    pos
}

/// Length of `s` after removing its last `count` codepoints.
#[must_use]
pub fn truncate_chars_len(s: &[u8], count: usize) -> usize {
    let mut len = s.len();
    for _ in 0..count {
        if len == 0 {
            break;
        }
        len = remove_last_char_len(&s[..len]);
    }
    len
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::vec::Vec;

    fn compose(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        match compose_nfc(input, |_| true, |b| out.extend_from_slice(b)) {
            Compose::Unchanged => input.to_vec(),
            Compose::Composed => out,
            Compose::Aborted => unreachable!(),
        }
    }

    fn compose_in_place(input: &[u8]) -> Vec<u8> {
        let mut buf = input.to_vec();
        let len = compose_nfc_in_place(&mut buf);
        buf.truncate(len);
        buf
    }

    #[test]
    fn decode_rejects_malformed_input() {
        assert_eq!(decode(b""), (0, 0));
        assert_eq!(decode(b"\0a"), (0, 0));
        assert_eq!(decode(b"a"), ('a' as u32, 1));
        assert_eq!(decode("ğ".as_bytes()), (0x11F, 2));
        assert_eq!(decode("€".as_bytes()), (0x20AC, 3));
        assert_eq!(decode("𝄞".as_bytes()), (0x1D11E, 4));
        assert_eq!(decode(b"\x80"), (REPLACEMENT, 1)); // stray continuation
        assert_eq!(decode(b"\xFF"), (REPLACEMENT, 1));
        assert_eq!(decode(b"\xE2\x82"), (REPLACEMENT, 2)); // truncated
        assert_eq!(decode(b"\xE2a"), (REPLACEMENT, 1));
        assert_eq!(decode(b"\xC0\x80"), (REPLACEMENT, 1)); // overlong NUL
        assert_eq!(decode(b"\xED\xA0\x80"), (REPLACEMENT, 1)); // surrogate
        assert_eq!(decode(b"\xF4\x90\x80\x80"), (REPLACEMENT, 1)); // > U+10FFFF
    }

    #[test]
    fn composes_latin_vietnamese_and_hangul() {
        for f in [compose, compose_in_place] {
            assert_eq!(f(b"hello"), b"hello");
            assert_eq!(f(b"e\xCC\x81"), "é".as_bytes());
            assert_eq!(f(b"a\xCC\x82\xCC\x81"), "ấ".as_bytes());
            assert_eq!(f(b"q\xCC\x81"), b"q\xCC\x81");
            assert_eq!(f(b"\xCC\x81"), b"\xCC\x81");
            assert_eq!(f("\u{1112}\u{1161}\u{11AB}".as_bytes()), "한".as_bytes());
        }
    }

    #[test]
    fn compose_reports_absorbed_indices_and_aborts() {
        let mut seen = Vec::new();
        let result = compose_nfc(
            b"xe\xCC\x81",
            |i| {
                seen.push(i);
                true
            },
            |_| {},
        );
        assert_eq!(result, Compose::Composed);
        assert_eq!(seen, [2]);
        assert_eq!(
            compose_nfc(b"e\xCC\x81", |_| false, |_| {}),
            Compose::Aborted
        );
    }

    #[test]
    fn compose_table_is_sorted_for_binary_search() {
        assert!(COMPOSE_TABLE.is_sorted_by_key(|e| (e.base, e.mark)));
    }

    #[test]
    fn codepoints_stop_at_nul() {
        let mut it = codepoints(b"a\xC3\xA9\x80\0z");
        assert!(it.by_ref().eq([0x61, 0xE9, REPLACEMENT]));
        assert_eq!(it.as_bytes(), b"\0z");
    }

    #[test]
    fn decomposed_base_lookup() {
        assert_eq!(decomposed_base(0xE9), Some('e' as u32));
        assert_eq!(decomposed_base(0xF8), None);
        assert_eq!(decomposed_base('a' as u32), None);
    }

    #[test]
    fn truncation_respects_codepoint_boundaries() {
        assert_eq!(safe_truncate_len(b""), 0);
        assert_eq!(safe_truncate_len(b"ab\xE2\x82"), 2);
        assert_eq!(safe_truncate_len(b"ab\xE2\x82\xAC"), 5);
        assert_eq!(safe_truncate_len(b"\xE2\x82"), 2);
        assert_eq!(remove_last_char_len("aé".as_bytes()), 1);
        assert_eq!(remove_last_char_len(b""), 0);
        assert_eq!(truncate_chars_len("aéğ".as_bytes(), 2), 1);
        assert_eq!(truncate_chars_len(b"ab", 5), 0);
    }
}
