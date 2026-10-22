//! A chapter's text cut into words, as lib/Epub's `ChapterHtmlSlimParser` feeds them to a
//! paragraph: bytes gather in the parser's word buffer until whitespace ends the word, a no-break
//! space (U+00A0, U+202F) becomes a space word attached to both neighbours, a BOM is dropped, a
//! word that fills the buffer is cut on a codepoint boundary and goes on attached, and preserved
//! whitespace (`white-space: pre`) keeps each space as a word, a tab as four, and a newline as a
//! line break. Each word starts at the visible offset of its first byte.
//!
//! The buffer and what a word becomes are the parser's ([`Words`]); this module only decides where
//! words end.

use bembeyaz_utf8 as utf8;

/// Bytes the word buffer holds before a word is cut.
pub const MAX_WORD: usize = 200;

/// The parser's word buffer and what it does with words. A callback may change the buffer, so
/// every read after one sees its effect.
pub trait Words {
    /// How many bytes are buffered.
    fn buffered(&mut self) -> usize;
    /// Copies the buffered bytes into `out` and returns how many.
    fn read(&mut self, out: &mut [u8; MAX_WORD]) -> usize;
    /// Replaces the buffered bytes.
    fn set(&mut self, bytes: &[u8]);
    fn push(&mut self, byte: u8);
    /// The visible offset of the buffered word's first byte.
    fn offset(&mut self) -> u32;
    fn set_offset(&mut self, offset: u32);
    /// Whether the next word attaches to the one before, with no break between.
    fn set_continues(&mut self, continues: bool);
    /// Hands the buffered word to the paragraph and empties the buffer.
    fn flush(&mut self);
    /// Preserved whitespace: the paragraph's first line has no indent.
    fn suppress_indent(&mut self);
    /// A preserved newline; false when there is no paragraph left to go on with.
    fn line_break(&mut self) -> bool;
    /// An initial letter's capture at `rest` (the text from here on): the bytes it took, 0 for
    /// none; `None` when no initial letter is pending any more.
    fn initial(&mut self, rest: &[u8]) -> Option<usize>;
}

/// How [`scan`] reads a piece of text.
#[derive(Clone, Copy, Default)]
pub struct Piece {
    /// The text is visible: its codepoints count as offsets from `start`.
    pub counted: bool,
    pub start: u32,
    /// `white-space: pre`.
    pub preserve: bool,
    /// An initial letter may be pending (false spares a [`Words::initial`] call per byte).
    pub initial: bool,
}

/// Whether [`scan`] read all of the text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scan {
    Done,
    /// A line break left no paragraph.
    Stopped,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\r' | b'\n' | b'\t')
}

// One attached space word for a no-break space at `offset`.
fn no_break_space(words: &mut impl Words, offset: u32) {
    if words.buffered() > 0 {
        words.flush();
    }
    words.set(b" ");
    words.set_offset(offset);
    words.set_continues(true);
    words.flush();
    words.set_continues(true);
}

// The buffer is full: hands over the complete codepoints, keeping a cut one for the next word.
// Apart, so its copy of the buffer is not on the stack during every other callback.
#[inline(never)]
#[cold]
fn cut_full_word(words: &mut impl Words) {
    let mut buf = [0u8; MAX_WORD];
    let len = words.read(&mut buf);
    let bytes = buf.get(..len).unwrap_or(&[]);
    let safe = utf8::safe_truncate_len(bytes);
    if safe < len && safe > 0 {
        let complete = utf8::codepoints(bytes.get(..safe).unwrap_or(&[])).count();
        let offset = words
            .offset()
            .wrapping_add(u32::try_from(complete).unwrap_or(u32::MAX));
        let cut = bytes.get(safe..).unwrap_or(&[]);
        let mut kept = [0u8; 4];
        let kept_len = cut.len().min(kept.len());
        if let (Some(dst), Some(src)) = (kept.get_mut(..kept_len), cut.get(..kept_len)) {
            dst.copy_from_slice(src);
        }
        words.set(bytes.get(..safe).unwrap_or(&[]));
        words.flush();
        words.set_continues(true);
        words.set(kept.get(..kept_len).unwrap_or(&[]));
        words.set_offset(offset);
    } else {
        words.flush();
        words.set_continues(true);
    }
}

/// Cuts `text` into words through `words`, from where its buffer left off.
pub fn scan(text: &[u8], piece: Piece, words: &mut impl Words) -> Scan {
    let mut next_offset = piece.start;
    let mut initial = piece.initial;
    let mut i = 0;
    while let Some(&b) = text.get(i) {
        let offset = next_offset;
        if piece.counted && b & 0xC0 != 0x80 {
            next_offset = next_offset.wrapping_add(1);
        }
        let rest = text.get(i..).unwrap_or(&[]);

        if initial {
            match words.initial(rest) {
                Some(0) => {}
                Some(taken) => {
                    i += taken;
                    continue;
                }
                None => initial = false,
            }
        }

        if piece.preserve && is_space(b) {
            words.flush();
            words.suppress_indent();
            if b == b'\n' {
                if !words.line_break() {
                    return Scan::Stopped;
                }
            } else if b != b'\r' {
                // XML normalizes line endings; a tab is a bounded four spaces.
                for _ in 0..if b == b'\t' { 4 } else { 1 } {
                    words.set(b" ");
                    words.set_offset(offset);
                    words.flush();
                }
            }
            i += 1;
            continue;
        }

        if is_space(b) {
            if words.buffered() > 0 {
                words.flush();
            }
            // A real word boundary.
            words.set_continues(false);
            i += 1;
            continue;
        }

        // The byte in hand first: a slice comparison would be a call per byte.
        match b {
            0xC2 if rest.get(1) == Some(&0xA0) => {
                no_break_space(words, offset);
                i += 2;
                continue;
            }
            0xE2 if rest.get(1..3) == Some(&[0x80, 0xAF]) => {
                no_break_space(words, offset);
                i += 3;
                continue;
            }
            0xEF if rest.get(1..3) == Some(&[0xBB, 0xBF]) => {
                i += 3;
                continue;
            }
            _ => {}
        }

        // Out of room: cut the word (for CJK text, which has no spaces, this is what breaks it).
        if words.buffered() >= MAX_WORD {
            cut_full_word(words);
        }
        if words.buffered() == 0 {
            words.set_offset(offset);
        }
        words.push(b);
        i += 1;
    }
    Scan::Done
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        clippy::unwrap_used
    )]

    extern crate std;

    use super::{MAX_WORD, Piece, Scan, Words, scan};
    use std::string::String;
    use std::vec::Vec;

    #[derive(Default)]
    struct Paragraph {
        buf: Vec<u8>,
        offset: u32,
        continues: bool,
        out: Vec<(String, u32, bool)>,
        breaks: usize,
        /// Initial-letter bytes still to take (each call takes one codepoint).
        initial: Option<usize>,
        stop_at_break: bool,
    }

    impl Words for Paragraph {
        fn buffered(&mut self) -> usize {
            self.buf.len()
        }
        fn read(&mut self, out: &mut [u8; MAX_WORD]) -> usize {
            out[..self.buf.len()].copy_from_slice(&self.buf);
            self.buf.len()
        }
        fn set(&mut self, bytes: &[u8]) {
            self.buf = bytes.to_vec();
        }
        fn push(&mut self, byte: u8) {
            self.buf.push(byte);
        }
        fn offset(&mut self) -> u32 {
            self.offset
        }
        fn set_offset(&mut self, offset: u32) {
            self.offset = offset;
        }
        fn set_continues(&mut self, continues: bool) {
            self.continues = continues;
        }
        fn flush(&mut self) {
            if !self.buf.is_empty() {
                self.out.push((
                    String::from_utf8_lossy(&self.buf).into_owned(),
                    self.offset,
                    self.continues,
                ));
            }
            self.buf.clear();
            self.continues = false;
        }
        fn suppress_indent(&mut self) {}
        fn line_break(&mut self) -> bool {
            self.breaks += 1;
            !self.stop_at_break
        }
        fn initial(&mut self, rest: &[u8]) -> Option<usize> {
            match self.initial {
                Some(0) | None => None,
                Some(n) => {
                    self.initial = Some(n - 1);
                    Some(1.min(rest.len()))
                }
            }
        }
    }

    fn words(text: &[u8], piece: Piece) -> Vec<(String, u32, bool)> {
        let mut p = Paragraph::default();
        assert_eq!(scan(text, piece, &mut p), Scan::Done);
        p.flush();
        p.out
    }

    const COUNTED: Piece = Piece {
        counted: true,
        start: 0,
        preserve: false,
        initial: false,
    };

    #[test]
    fn spaces_end_words_and_no_break_spaces_attach() {
        let got = words("200\u{a0}km a\u{202f}b \u{feff}zz".as_bytes(), COUNTED);
        let want = [
            ("200", 0, false),
            (" ", 3, true),
            ("km", 4, true),
            ("a", 7, false),
            (" ", 8, true),
            ("b", 9, true),
            ("zz", 12, false),
        ];
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert_eq!((g.0.as_str(), g.1, g.2), w);
        }
    }

    #[test]
    fn a_full_buffer_cuts_on_a_codepoint_boundary() {
        let text = "\u{20ac}".repeat(70);
        let got = words(text.as_bytes(), COUNTED);
        assert_eq!(got.len(), 2);
        assert_eq!((got[0].0.len(), got[0].1), (198, 0));
        assert_eq!((got[1].0.len(), got[1].1, got[1].2), (12, 66, true));
        let got = words(&[b'a'; 250], COUNTED);
        assert_eq!((got[0].0.len(), got[1].0.len(), got[1].1), (200, 50, 200));
    }

    #[test]
    fn preserved_whitespace_keeps_spaces_tabs_and_breaks() {
        let piece = Piece {
            preserve: true,
            ..COUNTED
        };
        let mut p = Paragraph::default();
        assert_eq!(scan(b"a\tb\r\nc", piece, &mut p), Scan::Done);
        p.flush();
        let texts: Vec<&str> = p.out.iter().map(|w| w.0.as_str()).collect();
        assert_eq!(texts, ["a", " ", " ", " ", " ", "b", "c"]);
        assert_eq!(p.breaks, 1);
        p.stop_at_break = true;
        assert_eq!(scan(b"x\ny", piece, &mut p), Scan::Stopped);
    }

    #[test]
    fn an_initial_letter_takes_bytes_until_it_is_done() {
        let mut p = Paragraph {
            initial: Some(2),
            ..Paragraph::default()
        };
        assert_eq!(
            scan(
                b"Once",
                Piece {
                    initial: true,
                    ..COUNTED
                },
                &mut p
            ),
            Scan::Done
        );
        p.flush();
        assert_eq!(p.out[0].0, "ce");
        assert_eq!(p.out[0].1, 2);
    }

    #[test]
    fn any_text_never_panics() {
        let mut seed = 0x7e47_5ca1_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let pool: [&[u8]; 9] = [
            b" ",
            b"\t",
            b"\n",
            &[0xC2, 0xA0],
            &[0xE2, 0x80, 0xAF],
            &[0xEF, 0xBB, 0xBF],
            &[0xE2, 0x82],
            b"word",
            &[0x80, 0xFF],
        ];
        for _ in 0..3000 {
            let mut text = Vec::new();
            for _ in 0..next() % 40 {
                if next() % 20 == 0 {
                    text.extend(core::iter::repeat_n(b'x', (next() % 300) as usize));
                } else {
                    text.extend_from_slice(pool[(next() % 9) as usize]);
                }
            }
            let mut p = Paragraph {
                initial: Some((next() % 3) as usize),
                stop_at_break: next() % 2 == 0,
                ..Paragraph::default()
            };
            let piece = Piece {
                counted: next() % 2 == 0,
                start: next(),
                preserve: next() % 2 == 0,
                initial: next() % 2 == 0,
            };
            let _ = scan(&text, piece, &mut p);
            assert!(p.buf.len() <= MAX_WORD + 1);
        }
    }
}
