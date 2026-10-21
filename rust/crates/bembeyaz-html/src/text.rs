//! Plain text (a `.txt` or `.md` book) as the XHTML body lib/Txt's `TxtToHtml` writes, pinned by
//! `test/txt_to_html`: line ends become `<br />` (CRs dropped), spaces at the start of a line become
//! `&#160;`, a run of spaces inside a line becomes `&#160;` except its last space, which stays, a
//! run at the end of a line or of the text is dropped, `&`, `<` and `>` are escaped, other bytes
//! below 0x20 except tab become spaces, and a UTF-8 byte order mark at the start of the first piece
//! is dropped.

const BOM: &[u8] = b"\xEF\xBB\xBF";
const NBSP: &[u8] = b"&#160;";

/// A text being converted, fed in the pieces it is read in. `bb_text_html` in C.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct TextToHtml {
    started: bool,
    at_line_start: bool,
    pending_spaces: usize,
}

impl Default for TextToHtml {
    fn default() -> Self {
        Self::new()
    }
}

impl TextToHtml {
    #[must_use]
    pub const fn new() -> Self {
        TextToHtml {
            started: false,
            at_line_start: true,
            pending_spaces: 0,
        }
    }

    /// Converts the next piece, handing the XHTML to `out` in pieces.
    pub fn feed(&mut self, piece: &[u8], out: &mut impl FnMut(&[u8])) {
        // An empty piece is no first piece.
        if piece.is_empty() {
            return;
        }
        let mut text = piece;
        if !self.started {
            self.started = true;
            if text.starts_with(BOM) {
                text = text.get(BOM.len()..).unwrap_or_default();
            }
        }
        // Bytes copied as they are, handed over in one go.
        let mut run_start = 0;
        let mut i = 0;
        let flush = |out: &mut dyn FnMut(&[u8]), from: usize, to: usize| {
            if to > from {
                out(text.get(from..to).unwrap_or_default());
            }
        };
        while let Some(&b) = text.get(i) {
            let plain = !matches!(b, b'\r' | b'\n' | b' ' | b'&' | b'<' | b'>')
                && (b >= 0x20 || b == b'\t');
            if plain && self.pending_spaces == 0 {
                self.at_line_start = false;
                i += 1;
                continue;
            }
            flush(out, run_start, i);
            run_start = i + 1;
            i += 1;
            match b {
                b'\r' => {}
                b'\n' => {
                    self.pending_spaces = 0;
                    out(b"<br />");
                    self.at_line_start = true;
                }
                b' ' if self.at_line_start => out(NBSP),
                b' ' => self.pending_spaces += 1,
                _ => {
                    for _ in 1..self.pending_spaces {
                        out(NBSP);
                    }
                    if self.pending_spaces > 0 {
                        out(b" ");
                    }
                    self.pending_spaces = 0;
                    self.at_line_start = false;
                    match b {
                        b'&' => out(b"&amp;"),
                        b'<' => out(b"&lt;"),
                        b'>' => out(b"&gt;"),
                        _ if b < 0x20 && b != b'\t' => out(b" "),
                        _ => out(core::slice::from_ref(&b)),
                    }
                }
            }
        }
        flush(out, run_start, i);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]

    extern crate std;
    use super::TextToHtml;
    use std::vec::Vec;

    fn convert(pieces: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut t = TextToHtml::new();
        for piece in pieces {
            t.feed(piece, &mut |b| out.extend_from_slice(b));
        }
        out
    }

    #[test]
    fn converts_lines_spaces_and_escapes() {
        assert_eq!(
            convert(&[b"  a  b \r\nc<d>&e\x01f\tg  "]),
            b"&#160;&#160;a&#160; b<br />c&lt;d&gt;&amp;e f\tg"
        );
        assert_eq!(convert(&[b"\xEF\xBB\xBFx"]), b"x");
        assert_eq!(convert(&[b"x", b"\xEF\xBB\xBFy"]), b"x\xEF\xBB\xBFy");
        assert_eq!(convert(&[b"\xEF\xBB", b"\xBFy"]), b"\xEF\xBB\xBFy");
        assert_eq!(convert(&[b"a ", b" b"]), b"a&#160; b");
    }

    #[test]
    fn piece_boundaries_change_nothing_after_the_first() {
        let text = b"  Lorem ipsum\r\n   dolor <sit> & amet\n\n    end   ";
        let whole = convert(&[text]);
        for cut in 1..text.len() {
            assert_eq!(convert(&[&text[..cut], &text[cut..]]), whole, "{cut}");
        }
    }
}
