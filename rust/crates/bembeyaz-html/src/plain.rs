//! An HTML fragment as plain text (src/util/HtmlToPlainText, for dictionary definitions and Details
//! descriptions that cannot be laid out as styled pages): tags dropped, `p`, `h1` to `h6` and `hr` ending a paragraph (a
//! blank line) and `br`, `div`, `li` and `tr` a line, line ends in the text ending a line, tabs and
//! CRs as single spaces, named and numeric entities decoded, and spaces and line ends at the end
//! dropped. A break never stacks onto one already there, and drops the spaces before it.
//!
//! The text never grows: every entity is longer than its value, and every break replaces a tag or a
//! line end at least as long.

use super::entity;

/// Plain text being written into a buffer.
struct Out<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl Out<'_> {
    fn push(&mut self, b: u8) {
        if let Some(slot) = self.buf.get_mut(self.len) {
            *slot = b;
            self.len += 1;
        }
    }

    fn last(&self) -> Option<u8> {
        self.len
            .checked_sub(1)
            .and_then(|i| self.buf.get(i))
            .copied()
    }

    /// Drops trailing bytes that `drop` picks.
    fn trim(&mut self, drop: impl Fn(u8) -> bool) {
        while self.last().is_some_and(&drop) {
            self.len -= 1;
        }
    }

    /// Ends the text so far with `count` line ends, counting those already there; nothing at the
    /// start.
    fn line_ends(&mut self, count: usize) {
        self.trim(|b| b == b' ');
        if self.len == 0 {
            return;
        }
        let text = self.buf.get(..self.len).unwrap_or_default();
        let existing = text.iter().rev().take_while(|&&b| b == b'\n').count();
        for _ in existing..count {
            self.push(b'\n');
        }
    }

    /// Appends `codepoint` as UTF-8; nothing for 0, a surrogate or past U+10FFFF.
    fn codepoint(&mut self, codepoint: u32) {
        let mut utf8 = [0u8; 4];
        if codepoint != 0
            && let Some(c) = char::from_u32(codepoint)
        {
            for &b in c.encode_utf8(&mut utf8).as_bytes() {
                self.push(b);
            }
        }
    }
}

/// The break a tag (its text between `<` and `>`) makes.
fn tag_break(tag: &[u8]) -> usize {
    let tag = tag
        .iter()
        .position(|&b| b != b'/' && !matches!(b, b' ' | b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r'))
        .and_then(|start| tag.get(start..))
        .unwrap_or_default();
    let len = tag.iter().take_while(|b| b.is_ascii_alphanumeric()).count();
    let name = tag.get(..len).unwrap_or_default();
    let is = |names: &[&[u8]]| names.iter().any(|n| name.eq_ignore_ascii_case(n));
    if is(&[b"p", b"h1", b"h2", b"h3", b"h4", b"h5", b"h6", b"hr"]) {
        2
    } else {
        usize::from(is(&[b"br", b"div", b"li", b"tr"]))
    }
}

/// The codepoint of a numeric entity (`&#123;` or `&#x7B;`, from its `&` to its `;`), when it is one
/// up to U+10FFFF.
fn numeric(entity: &[u8]) -> Option<u32> {
    let digits = entity.strip_prefix(b"&#")?.strip_suffix(b";")?;
    let (digits, base) = match digits {
        [b'x' | b'X', rest @ ..] => (rest, 16),
        _ => (digits, 10),
    };
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |value, &c| {
        let digit = char::from(c).to_digit(base)?;
        (value <= (0x10_FFFF - digit) / base).then(|| value * base + digit)
    })
}

/// Writes `html` as plain text into `out`; how many bytes that took (as many as fit: `html`'s
/// length always does).
pub fn to_plain(html: &[u8], out: &mut [u8]) -> usize {
    let mut out = Out { buf: out, len: 0 };
    let mut i = 0;
    while let Some(&c) = html.get(i) {
        let rest = html.get(i..).unwrap_or_default();
        if let [b'<', next, ..] = rest
            && (matches!(next, b'/' | b'!' | b'?') || next.is_ascii_alphabetic())
        {
            let Some(close) = rest.iter().position(|&b| b == b'>') else {
                out.push(b'<');
                i += 1;
                continue;
            };
            let count = tag_break(rest.get(1..close).unwrap_or_default());
            if count > 0 {
                out.line_ends(count);
            }
            i += close + 1;
            continue;
        }
        if c == b'&'
            // Only a ';' within 16 bytes ends a reference.
            && let Some(semicolon) = rest.iter().take(17).skip(1).position(|&b| b == b';').map(|j| j + 1)
        {
            let reference = rest.get(..=semicolon).unwrap_or_default();
            if let Some(value) = entity(reference.get(1..semicolon).unwrap_or_default()) {
                for &b in value.text() {
                    out.push(b);
                }
                i += semicolon + 1;
                continue;
            }
            if let Some(codepoint) = numeric(reference) {
                out.codepoint(codepoint);
                i += semicolon + 1;
                continue;
            }
        }
        i += 1;
        match c {
            b'\r' | b'\t' => {
                if out.last().is_some_and(|b| b != b' ' && b != b'\n') {
                    out.push(b' ');
                }
            }
            b'\n' => out.line_ends(1),
            _ => out.push(c),
        }
    }
    out.trim(|b| b == b' ' || b == b'\n');
    out.len
}
