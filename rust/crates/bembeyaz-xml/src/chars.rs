//! Characters as the tokenizer sees them: what may appear in a document, in a name, and where a
//! character ends. The input is UTF-8 by now (see `decode`), checked here as expat checks it.

use crate::names::{NAME_MORE, NAME_START};

/// The character at the start of some bytes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Char {
    /// An ASCII character, whatever it is.
    Ascii(u8),
    /// A valid multi-byte character.
    Wide { cp: u32, len: usize },
    /// The bytes end inside the character.
    Partial,
    /// Not a character XML allows: a stray or malformed byte, an overlong form, a surrogate,
    /// U+FFFE or U+FFFF, or a value past U+10FFFF.
    Invalid,
}

/// Reads the character at `at`; `Partial` also when `at` is the end.
pub(crate) fn char_at(bytes: &[u8], at: usize) -> Char {
    let Some(&lead) = bytes.get(at) else {
        return Char::Partial;
    };
    let len = match lead {
        0x00..=0x7F => return Char::Ascii(lead),
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return Char::Invalid,
    };
    // A short sequence is partial before it is invalid, as in expat.
    let Some(seq) = bytes.get(at..at + len) else {
        return Char::Partial;
    };
    let trail = |i: usize| seq.get(i).copied().unwrap_or(0);
    let cont = |b: u8| b & 0xC0 == 0x80;
    let (b1, b2, b3) = (trail(1), trail(2), trail(3));
    let valid = match len {
        2 => lead >= 0xC2 && cont(b1),
        3 => {
            cont(b2)
                && match lead {
                    0xE0 => (0xA0..=0xBF).contains(&b1),
                    0xED => (0x80..=0x9F).contains(&b1),
                    // U+FFFE and U+FFFF
                    0xEF if b1 == 0xBF => b2 <= 0xBD,
                    _ => cont(b1),
                }
        }
        _ => {
            cont(b2)
                && cont(b3)
                && match lead {
                    0xF0 => (0x90..=0xBF).contains(&b1),
                    0xF4 => (0x80..=0x8F).contains(&b1),
                    _ => cont(b1),
                }
        }
    };
    if !valid {
        return Char::Invalid;
    }
    let mut cp = u32::from(lead) & (0x7F >> len);
    for &b in seq.iter().skip(1) {
        cp = (cp << 6) | u32::from(b & 0x3F);
    }
    Char::Wide { cp, len }
}

pub(crate) fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

/// ASCII characters a document may contain.
pub(crate) fn is_xml_ascii(b: u8) -> bool {
    b >= 0x20 || matches!(b, b'\t' | b'\n' | b'\r')
}

pub(crate) fn is_ascii_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

pub(crate) fn is_ascii_name_char(b: u8) -> bool {
    is_ascii_name_start(b) || b.is_ascii_digit() || b == b'.' || b == b'-'
}

/// Name characters from U+0080: `start` for the first one. With `latin1` (an ISO-8859-1 or
/// UTF-16 document) U+0080..U+00FF follow expat's Latin-1 table, which also lets U+00AA, U+00B5
/// and U+00BA start names. Characters past U+FFFF are never part of a name.
pub(crate) fn is_wide_name_char(cp: u32, start: bool, latin1: bool) -> bool {
    if latin1 && cp < 0x100 {
        return match cp {
            0xAA | 0xB5 | 0xBA | 0xC0..=0xD6 | 0xD8..=0xF6 | 0xF8..=0xFF => true,
            0xB7 => !start,
            _ => false,
        };
    }
    let Ok(cp) = u16::try_from(cp) else {
        return false;
    };
    let within = |table: &[(u16, u16)]| {
        table
            .binary_search_by(|&(lo, hi)| {
                if hi < cp {
                    core::cmp::Ordering::Less
                } else if lo > cp {
                    core::cmp::Ordering::Greater
                } else {
                    core::cmp::Ordering::Equal
                }
            })
            .is_ok()
    };
    within(&NAME_START) || (!start && within(&NAME_MORE))
}

/// Appends `cp` as UTF-8.
pub(crate) fn push_utf8(out: &mut alloc::vec::Vec<u8>, cp: u32) {
    let mut buf = [0; 4];
    out.extend_from_slice(encode_utf8(cp, &mut buf));
}

pub(crate) fn encode_utf8(cp: u32, buf: &mut [u8; 4]) -> &[u8] {
    // The casts keep the low bits on purpose.
    #[allow(clippy::cast_possible_truncation)]
    let len = match cp {
        0..=0x7F => {
            buf[0] = cp as u8;
            1
        }
        0x80..=0x7FF => {
            buf[0] = 0xC0 | (cp >> 6) as u8;
            buf[1] = 0x80 | (cp & 0x3F) as u8;
            2
        }
        0x800..=0xFFFF => {
            buf[0] = 0xE0 | (cp >> 12) as u8;
            buf[1] = 0x80 | ((cp >> 6) & 0x3F) as u8;
            buf[2] = 0x80 | (cp & 0x3F) as u8;
            3
        }
        _ => {
            buf[0] = 0xF0 | (cp >> 18) as u8;
            buf[1] = 0x80 | ((cp >> 12) & 0x3F) as u8;
            buf[2] = 0x80 | ((cp >> 6) & 0x3F) as u8;
            buf[3] = 0x80 | (cp & 0x3F) as u8;
            4
        }
    };
    buf.get(..len).unwrap_or_default()
}
