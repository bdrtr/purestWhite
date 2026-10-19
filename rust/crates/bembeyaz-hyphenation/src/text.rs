//! Character classes, case folding and the word's codepoints.

use alloc::vec::Vec;

/// Latin letters: ASCII, Latin-1 and Latin Extended-A, and capital sharp s.
pub fn is_latin_letter(cp: u32) -> bool {
    matches!(cp, 0x41..=0x5A | 0x61..=0x7A | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0x17F | 0x1E9E)
}

pub fn is_cyrillic_letter(cp: u32) -> bool {
    matches!(cp, 0x400..=0x52F)
}

pub fn is_alphabetic(cp: u32) -> bool {
    is_latin_letter(cp) || is_cyrillic_letter(cp)
}

/// Lowercase for the Latin patterns: ASCII, Latin-1, Latin Extended-A, Ÿ and ẞ.
pub fn to_lower_latin(cp: u32) -> u32 {
    match cp {
        0x41..=0x5A | 0xC0..=0xD6 | 0xD8..=0xDE => cp + 0x20,
        // Latin Extended-A pairs each capital with the next codepoint; the capitals sit on even
        // codepoints, then on odd ones from U+0139, even from U+014A and odd from U+0179.
        0x100..=0x137 | 0x14A..=0x177 if cp.is_multiple_of(2) => cp + 1,
        0x139..=0x148 | 0x179..=0x17E if !cp.is_multiple_of(2) => cp + 1,
        0x178 => 0xFF,
        0x1E9E => 0xDF,
        _ => cp,
    }
}

/// Lowercase for the Cyrillic patterns: the basic capitals and Ё only.
pub fn to_lower_cyrillic(cp: u32) -> u32 {
    match cp {
        0x410..=0x42F => cp + 0x20,
        0x401 => 0x451,
        _ => cp,
    }
}

/// Trimmed from both ends of a word.
pub fn is_punctuation(cp: u32) -> bool {
    matches!(
        cp,
        0x21 | 0x22
            | 0x27..=0x29
            | 0x2C..=0x2F
            | 0x3A
            | 0x3B
            | 0x3F
            | 0x5B
            | 0x5D
            | 0x7B
            | 0x7D
            | 0xA0
            | 0xAB
            | 0xBB
            | 0x2018..=0x201A
            | 0x201C..=0x201E
            | 0x2026
            | 0x2039
            | 0x203A
    )
}

pub fn is_apostrophe(cp: u32) -> bool {
    matches!(cp, 0x27 | 0x2018 | 0x2019)
}

/// Hyphens and dashes a word may already break at (and the underscore and ellipsis).
#[must_use]
pub fn is_explicit_hyphen(cp: u32) -> bool {
    matches!(
        cp,
        0x2D | 0x5F | 0xAD | 0x58A | 0x2010
            ..=0x2015
                | 0x2026
                | 0x2043
                | 0x207B
                | 0x208B
                | 0x2212
                | 0x2E17
                | 0x2E3A
                | 0x2E3B
                | 0xFE58
                | 0xFE63
                | 0xFF0D
    )
}

#[must_use]
pub fn is_soft_hyphen(cp: u32) -> bool {
    cp == 0xAD
}

pub fn is_separator(cp: u32) -> bool {
    is_explicit_hyphen(cp) || is_apostrophe(cp)
}

/// Characters that break without a hyphen: as `utf8IsCjkBreakable` in lib/Utf8/Utf8.h.
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

/// The precomposed letter for `base` followed by the combining `mark`, for the Western
/// European and Polish accents only (a lightweight NFC, not Unicode's).
fn compose(base: u32, mark: u32) -> Option<u32> {
    // Each mark: the bases it takes, then the letters they make, in the same order.
    const TABLE: &[(u32, &[u8], &[u32])] = &[
        (
            0x300,
            b"AaEeIiOoUu",
            &[0xC0, 0xE0, 0xC8, 0xE8, 0xCC, 0xEC, 0xD2, 0xF2, 0xD9, 0xF9],
        ),
        (
            0x301,
            b"AaEeIiOoUuYyCcNnSsZz",
            &[
                0xC1, 0xE1, 0xC9, 0xE9, 0xCD, 0xED, 0xD3, 0xF3, 0xDA, 0xFA, 0xDD, 0xFD, 0x106,
                0x107, 0x143, 0x144, 0x15A, 0x15B, 0x179, 0x17A,
            ],
        ),
        (
            0x302,
            b"AaEeIiOoUu",
            &[0xC2, 0xE2, 0xCA, 0xEA, 0xCE, 0xEE, 0xD4, 0xF4, 0xDB, 0xFB],
        ),
        (0x303, b"AaNn", &[0xC3, 0xE3, 0xD1, 0xF1]),
        (
            0x308,
            b"AaEeIiOoUuYy",
            &[
                0xC4, 0xE4, 0xCB, 0xEB, 0xCF, 0xEF, 0xD6, 0xF6, 0xDC, 0xFC, 0x178, 0xFF,
            ],
        ),
        (0x307, b"Zz", &[0x17B, 0x17C]),
        (0x327, b"Cc", &[0xC7, 0xE7]),
        (0x328, b"AaEe", &[0x104, 0x105, 0x118, 0x119]),
    ];
    let &(_, bases, letters) = TABLE.iter().find(|&&(m, _, _)| m == mark)?;
    let at = bases.iter().position(|&b| u32::from(b) == base)?;
    letters.get(at).copied()
}

/// Codepoints kept on the stack, punctuation included; a longer word goes to the heap.
const INLINE: usize = 70;

/// A word's codepoints, accents composed, with where each starts in the word.
pub struct Word {
    inline_values: [u32; INLINE],
    inline_offsets: [u32; INLINE],
    heap: Option<(Vec<u32>, Vec<u32>)>,
    len: usize,
}

impl Word {
    /// An empty word, to [`Word::read`] into where it lies (it is too big to move around).
    pub const fn new() -> Self {
        Word {
            inline_values: [0; INLINE],
            inline_offsets: [0; INLINE],
            heap: None,
            len: 0,
        }
    }

    /// Reads `bytes` up to its end or a NUL; false when a long word does not fit in memory.
    pub fn read(&mut self, bytes: &[u8]) -> bool {
        self.len = self.fill(bytes);
        if self.len <= INLINE {
            return true;
        }
        // Read again into a block of the size the first pass counted.
        let mut values = Vec::new();
        let mut offsets = Vec::new();
        if values.try_reserve_exact(self.len).is_err()
            || offsets.try_reserve_exact(self.len).is_err()
        {
            return false;
        }
        values.resize(self.len, 0);
        offsets.resize(self.len, 0);
        self.heap = Some((values, offsets));
        self.fill(bytes);
        true
    }

    /// Stores what fits of `bytes`' codepoints; returns how many there are.
    fn fill(&mut self, bytes: &[u8]) -> usize {
        let (values, offsets) = self.slots_mut();
        scan(bytes, |i, cp, offset| {
            if let (Some(v), Some(o)) = (values.get_mut(i), offsets.get_mut(i)) {
                *v = cp;
                *o = offset;
            }
        })
    }

    fn slots_mut(&mut self) -> (&mut [u32], &mut [u32]) {
        match &mut self.heap {
            Some((values, offsets)) => (values, offsets),
            None => (&mut self.inline_values, &mut self.inline_offsets),
        }
    }

    pub fn values(&self) -> &[u32] {
        let values: &[u32] = match &self.heap {
            Some((values, _)) => values,
            None => &self.inline_values,
        };
        values.get(..self.len).unwrap_or_default()
    }

    pub fn offsets(&self) -> &[u32] {
        let offsets: &[u32] = match &self.heap {
            Some((_, offsets)) => offsets,
            None => &self.inline_offsets,
        };
        offsets.get(..self.len).unwrap_or_default()
    }
}

/// Calls `put(index, codepoint, offset)` for each codepoint of `bytes` (to its end or a NUL),
/// a combining mark folded into the letter before it when they compose; returns the count.
fn scan(bytes: &[u8], mut put: impl FnMut(usize, u32, u32)) -> usize {
    let mut count = 0;
    let mut last = None;
    let mut at = 0;
    while let Some(rest) = bytes.get(at..) {
        let (cp, used) = bembeyaz_utf8::decode(rest);
        if used == 0 {
            break;
        }
        let offset = u32::try_from(at).unwrap_or(u32::MAX);
        at += used;
        if let Some((start, prev)) = last
            && let Some(composed) = compose(prev, cp)
        {
            last = Some((start, composed));
            put(count - 1, composed, start);
            continue;
        }
        put(count, cp, offset);
        last = Some((offset, cp));
        count += 1;
    }
    count
}

/// The part of `cps` hyphenation looks at: without a trailing footnote mark like `[12]` and
/// without punctuation at either end.
pub fn trim(cps: &[u32]) -> (usize, usize) {
    let mut hi = cps.len();
    if cps.len() >= 3 {
        let at = |i: usize| cps.get(i).copied().unwrap_or(0);
        // Past any punctuation after it, a bracket and two digits or more.
        let mut end = cps.len();
        while end > 0 && is_punctuation(at(end - 1)) {
            end -= 1;
        }
        if end > 0 && at(end - 1).wrapping_sub(u32::from(b'0')) < 10 {
            let mut pos = end - 1;
            while pos > 0 && at(pos - 1).wrapping_sub(u32::from(b'0')) < 10 {
                pos -= 1;
            }
            if pos > 0 && at(pos - 1) == u32::from(b'[') && end - pos > 1 {
                hi = pos - 1;
            }
        }
    }
    let mut lo = 0;
    while lo < hi && cps.get(lo).is_some_and(|&cp| is_punctuation(cp)) {
        lo += 1;
    }
    while hi > lo && cps.get(hi - 1).is_some_and(|&cp| is_punctuation(cp)) {
        hi -= 1;
    }
    (lo, hi)
}
