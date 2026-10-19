//! The encodings expat knows without help (UTF-8, UTF-16 either way, ISO-8859-1, US-ASCII),
//! detected as expat detects them and turned into UTF-8 for the tokenizer.
//!
//! What expat rejects stays rejectable: a byte US-ASCII does not have and a UTF-16 low surrogate
//! on its own become 0xFF, which is never valid UTF-8. A high surrogate takes whatever unit
//! follows it, as in expat.

use alloc::vec::Vec;

/// A byte the tokenizer always rejects.
pub(crate) const REJECT: u8 = 0xFF;
/// Stands for a high surrogate whose next unit has not come: the start of a character that
/// the input ends inside, as expat sees it.
const CUT: u8 = 0xF0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Encoding {
    Utf8,
    Latin1,
    Ascii,
    Utf16Le,
    Utf16Be,
}

impl Encoding {
    pub(crate) fn is_utf16(self) -> bool {
        matches!(self, Encoding::Utf16Le | Encoding::Utf16Be)
    }

    /// Whether U+0080..U+00FF follow expat's Latin-1 name table (see `chars`).
    pub(crate) fn latin1_names(self) -> bool {
        !matches!(self, Encoding::Utf8)
    }

    /// The encoding an XML declaration names, as expat matches names: case-insensitively, and
    /// "UTF-16" as whichever byte order the document already has.
    pub(crate) fn named(name: &[u8], current: Encoding) -> Option<Encoding> {
        let is = |s: &str| name.eq_ignore_ascii_case(s.as_bytes());
        if is("UTF-16") && current.is_utf16() {
            Some(current)
        } else if is("ISO-8859-1") {
            Some(Encoding::Latin1)
        } else if is("US-ASCII") {
            Some(Encoding::Ascii)
        } else if is("UTF-8") {
            Some(Encoding::Utf8)
        } else if is("UTF-16") || is("UTF-16BE") {
            Some(Encoding::Utf16Be)
        } else if is("UTF-16LE") {
            Some(Encoding::Utf16Le)
        } else {
            None
        }
    }

    /// How many input bytes produced `decoded`.
    pub(crate) fn raw_len(self, decoded: &[u8]) -> u64 {
        let count = |f: fn(u8) -> u64| decoded.iter().map(|&b| f(b)).sum();
        match self {
            Encoding::Utf8 | Encoding::Ascii => decoded.len() as u64,
            Encoding::Latin1 => count(|b| u64::from(b & 0xC0 != 0x80)),
            Encoding::Utf16Le | Encoding::Utf16Be => count(|b| match b {
                0x80..=0xBF => 0,
                0xF0..=0xF4 => 4,
                _ => 2,
            }),
        }
    }
}

pub(crate) struct Decoder {
    /// None until enough of the document has come to tell.
    pub(crate) encoding: Option<Encoding>,
    /// Bytes not decoded yet: the start of the document before detection, else part of a
    /// UTF-16 unit or a high surrogate waiting for the next unit.
    held: [u8; 3],
    held_len: usize,
    /// Bytes the byte order mark took.
    pub(crate) bom_len: usize,
    /// At the end: a UTF-16 document had an odd byte left over.
    pub(crate) odd_byte: bool,
    /// The whole document is shorter than expat needs to tell its encoding (one byte, or the
    /// first two bytes of a UTF-8 byte order mark).
    pub(crate) too_short: bool,
}

impl Decoder {
    pub(crate) const fn new() -> Self {
        Decoder {
            encoding: None,
            held: [0; 3],
            held_len: 0,
            bom_len: 0,
            odd_byte: false,
            too_short: false,
        }
    }

    /// Decodes `raw` onto `out`; with `last` nothing is held back. False when `out` cannot grow.
    pub(crate) fn decode(&mut self, raw: &[u8], last: bool, out: &mut Vec<u8>) -> bool {
        let mut raw = raw;
        let mut start = [0; 3];
        let joined;
        if self.encoding.is_none() {
            // Detect from the first bytes, as expat's initScan does for a document.
            let have = self.held_len + raw.len();
            let mut first = [0u8; 3];
            for (i, b) in self
                .held
                .iter()
                .take(self.held_len)
                .chain(raw.iter())
                .take(3)
                .enumerate()
            {
                if let Some(slot) = first.get_mut(i) {
                    *slot = *b;
                }
            }
            let need_more = matches!((have, first), (0 | 1, _) | (2, [0xEF, 0xBB, _]));
            if need_more && !last {
                for &b in raw {
                    if let Some(slot) = self.held.get_mut(self.held_len) {
                        *slot = b;
                        self.held_len += 1;
                    }
                }
                return true;
            }
            self.too_short = need_more;
            let (encoding, bom) = match first {
                [0xFE, 0xFF, _] if have >= 2 => (Encoding::Utf16Be, 2),
                [0xFF, 0xFE, _] if have >= 2 => (Encoding::Utf16Le, 2),
                [0xEF, 0xBB, 0xBF] if have >= 3 => (Encoding::Utf8, 3),
                [0, _, _] if have >= 2 => (Encoding::Utf16Be, 0),
                [_, 0, _] if have >= 2 => (Encoding::Utf16Le, 0),
                _ => (Encoding::Utf8, 0),
            };
            self.encoding = Some(encoding);
            self.bom_len = bom;
            // Put the held bytes back in front of the new ones, past the mark.
            let held = self.held_len;
            self.held_len = 0;
            if held > 0 {
                let mut tmp = Vec::new();
                if tmp.try_reserve_exact(held + raw.len()).is_err() {
                    return false;
                }
                tmp.extend_from_slice(self.held.get(..held).unwrap_or_default());
                tmp.extend_from_slice(raw);
                joined = tmp;
                raw = joined.get(bom..).unwrap_or_default();
            } else {
                start.copy_from_slice(&first);
                raw = raw.get(bom..).unwrap_or_default();
            }
        }
        self.push(raw, last, out)
    }

    fn push(&mut self, raw: &[u8], last: bool, out: &mut Vec<u8>) -> bool {
        let Some(encoding) = self.encoding else {
            return true;
        };
        let worst = match encoding {
            Encoding::Utf8 | Encoding::Ascii => raw.len(),
            Encoding::Latin1 => raw.len() * 2,
            Encoding::Utf16Le | Encoding::Utf16Be => raw.len() * 2 + 4,
        };
        if out.try_reserve(worst).is_err() {
            return false;
        }
        match encoding {
            Encoding::Utf8 => out.extend_from_slice(raw),
            Encoding::Ascii => out.extend(raw.iter().map(|&b| if b < 0x80 { b } else { REJECT })),
            Encoding::Latin1 => {
                for &b in raw {
                    if b < 0x80 {
                        out.push(b);
                    } else {
                        out.push(0xC0 | (b >> 6));
                        out.push(0x80 | (b & 0x3F));
                    }
                }
            }
            Encoding::Utf16Le | Encoding::Utf16Be => self.push_utf16(raw, encoding, last, out),
        }
        true
    }

    fn push_utf16(&mut self, raw: &[u8], encoding: Encoding, last: bool, out: &mut Vec<u8>) {
        let big = encoding == Encoding::Utf16Be;
        let unit = |a: u8, b: u8| {
            if big {
                u16::from_be_bytes([a, b])
            } else {
                u16::from_le_bytes([a, b])
            }
        };
        // `held` keeps an odd byte at [held_len - 1] and a waiting high surrogate at [0..2].
        let mut high: Option<u16> = None;
        let mut odd: Option<u8> = None;
        match self.held_len {
            1 => odd = self.held.first().copied(),
            2 | 3 => {
                high = Some(unit(self.held[0], self.held[1]));
                if self.held_len == 3 {
                    odd = self.held.get(2).copied();
                }
            }
            _ => {}
        }
        self.held_len = 0;
        if high.is_some() && out.last() == Some(&CUT) {
            out.pop();
        }
        let mut bytes = raw.iter().copied();
        loop {
            let a = match odd.take() {
                Some(a) => a,
                None => match bytes.next() {
                    Some(a) => a,
                    None => break,
                },
            };
            let Some(b) = bytes.next() else {
                odd = Some(a);
                break;
            };
            let u = unit(a, b);
            if let Some(h) = high.take() {
                push_pair(h, u, out);
            } else if (0xD800..0xDC00).contains(&u) {
                high = Some(u);
            } else if (0xDC00..0xE000).contains(&u) {
                out.push(REJECT);
            } else {
                let mut buf = [0; 4];
                out.extend_from_slice(crate::chars::encode_utf8(u32::from(u), &mut buf));
            }
        }
        if high.is_some() {
            out.push(CUT);
        }
        if last {
            self.odd_byte = odd.is_some();
            return;
        }
        if let Some(h) = high {
            let [x, y] = if big {
                h.to_be_bytes()
            } else {
                h.to_le_bytes()
            };
            self.held[0] = x;
            self.held[1] = y;
            self.held_len = 2;
        }
        if let Some((a, slot)) = odd.zip(self.held.get_mut(self.held_len)) {
            *slot = a;
            self.held_len += 1;
        }
    }
}

/// A high surrogate and the unit after it, as expat's UTF-16 to UTF-8 conversion writes them:
/// a valid supplementary character even when the second unit is no low surrogate.
fn push_pair(high: u16, next: u16, out: &mut Vec<u8>) {
    let [hh, hl] = high.to_be_bytes();
    let [nh, nl] = next.to_be_bytes();
    let plane = (((hh & 3) << 2) | ((hl >> 6) & 3)) + 1;
    out.push((plane >> 2) | 0xF0);
    out.push(((hl >> 2) & 0xF) | ((plane & 3) << 4) | 0x80);
    out.push(((hl & 3) << 4) | ((nh & 3) << 2) | (nl >> 6) | 0x80);
    out.push((nl & 0x3F) | 0x80);
}
