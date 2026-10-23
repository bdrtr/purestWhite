//! A chapter's text cut into words, as lib/Epub's `ChapterHtmlSlimParser` feeds them to a
//! paragraph: bytes gather in the parser's word buffer until whitespace ends the word, a no-break
//! space (U+00A0, U+202F) becomes a space word attached to both neighbours, a BOM is dropped, a
//! word that fills the buffer is cut on a codepoint boundary and goes on attached, and preserved
//! whitespace (`white-space: pre`) keeps each space as a word, a tab as four, and a newline as a
//! line break. Each word starts at the visible offset of its first byte. An initial letter (drop
//! cap) takes its text first: a drop-cap span takes all of it (up to three codepoints in twelve
//! bytes), a `::first-letter` rule the block's leading punctuation and first letter.
//!
//! The buffer and what a word becomes are the parser's ([`Words`]); this module only decides where
//! words end.

use bembeyaz_utf8 as utf8;

/// Bytes the word buffer holds before a word is cut.
pub const MAX_WORD: usize = 200;
/// Bytes an initial letter holds.
pub const MAX_INITIAL_BYTES: usize = 12;
/// Codepoints an initial letter holds.
const MAX_INITIAL_CODEPOINTS: u8 = 3;

/// The initial letter being captured (`bb_html_initial_letter`). `lines` and `bold` are the
/// parser's, kept as they are until a capture is given up.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InitialLetter {
    /// The captured text, ending in a zero.
    pub text: [u8; MAX_INITIAL_BYTES + 1],
    pub len: u8,
    pub codepoints: u8,
    pub lines: u8,
    pub bold: bool,
    /// A `::first-letter` rule waits for the block's first letter.
    pub pending: bool,
    /// The depth of the drop-cap span whose text is taken, -1 for none.
    pub span_depth: i32,
}

impl InitialLetter {
    /// None being captured.
    pub const NONE: Self = Self {
        text: [0; MAX_INITIAL_BYTES + 1],
        len: 0,
        codepoints: 0,
        lines: 0,
        bold: false,
        pending: false,
        span_depth: -1,
    };
}

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
    /// The initial letter being captured.
    fn initial(&mut self) -> InitialLetter;
    fn set_initial(&mut self, letter: &InitialLetter);
    /// Whether the paragraph has no word yet.
    fn block_empty(&mut self) -> bool;
}

/// How [`scan`] reads a piece of text.
#[allow(clippy::struct_excessive_bools, reason = "the C ABI's flags")]
#[derive(Clone, Copy, Default)]
pub struct Piece {
    /// The text is visible: its codepoints count as offsets from `start`.
    pub counted: bool,
    pub start: u32,
    /// `white-space: pre`.
    pub preserve: bool,
    /// The text is inside a footnote link, which never starts an initial letter.
    pub footnote_link: bool,
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

/// Trims `text` of ASCII whitespace and collapses each run inside it to one space, in place (the
/// chapter parser's ruby text); returns the new length.
pub fn collapse_space(text: &mut [u8]) -> usize {
    let mut len = 0;
    let mut pending = false;
    for i in 0..text.len() {
        let Some(&byte) = text.get(i) else { break };
        if is_space(byte) {
            pending = len > 0;
            continue;
        }
        if pending {
            if let Some(slot) = text.get_mut(len) {
                *slot = b' ';
            }
            len += 1;
            pending = false;
        }
        if let Some(slot) = text.get_mut(len) {
            *slot = byte;
        }
        len += 1;
    }
    len
}

// Opening quotes and other punctuation that lead into a drop cap letter.
fn is_initial_punctuation(cp: u32) -> bool {
    match u8::try_from(cp) {
        Ok(ascii) if ascii < 0x80 => ascii.is_ascii_punctuation(),
        _ => {
            matches!(cp, 0xA1 | 0xAB | 0xBB | 0xBF)
                || (0x2010..=0x205E).contains(&cp)
                || (0x3008..=0x3011).contains(&cp)
        }
    }
}

// Not an initial letter after all: its text goes back into the word.
#[cold]
fn give_up_initial(words: &mut impl Words, letter: &InitialLetter) {
    for &b in letter.text.get(..usize::from(letter.len)).unwrap_or(&[]) {
        words.push(b);
    }
    words.set_initial(&InitialLetter::NONE);
}

// The initial letter's capture at the byte `rest` starts with: the bytes it took, 0 for none,
// None when no initial letter is pending any more. Apart: it runs only at a paragraph's start.
#[inline(never)]
fn take_initial(words: &mut impl Words, rest: &[u8], footnote_link: bool) -> Option<usize> {
    let mut letter = words.initial();
    let capturing_span = letter.span_depth >= 0;
    if !capturing_span && !letter.pending {
        return None;
    }
    let lead = rest.first().copied().unwrap_or(0);
    let space = is_space(lead);
    if !capturing_span && (space || words.buffered() != 0 || footnote_link || !words.block_empty())
    {
        return Some(0);
    }
    if space {
        // Several words: not an initial letter after all.
        if letter.len > 0 {
            give_up_initial(words, &letter);
        }
        return Some(0);
    }
    let len = rest.len().min(match lead {
        0..0x80 => 1,
        0x80..0xE0 => 2,
        0xE0..0xF0 => 3,
        _ => 4,
    });
    let at = usize::from(letter.len);
    if at + len > MAX_INITIAL_BYTES || letter.codepoints >= MAX_INITIAL_CODEPOINTS {
        give_up_initial(words, &letter);
        return Some(0);
    }
    if let (Some(dst), Some(src)) = (letter.text.get_mut(at..at + len), rest.get(..len)) {
        dst.copy_from_slice(src);
    }
    // at + len is at most MAX_INITIAL_BYTES.
    letter.len = u8::try_from(at + len).unwrap_or(0);
    if let Some(end) = letter.text.get_mut(at + len) {
        *end = 0;
    }
    letter.codepoints += 1;
    if !capturing_span {
        // ::first-letter takes leading punctuation plus the first letter.
        let head = rest.get(..rest.len().min(4)).unwrap_or(&[]);
        if !is_initial_punctuation(utf8::decode(head).0) {
            letter.pending = false;
        }
    }
    words.set_initial(&letter);
    Some(len)
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
    // No pending letter spares reading it for every byte; none can start during the scan.
    let letter = words.initial();
    let mut initial = letter.span_depth >= 0 || letter.pending;
    let mut i = 0;
    while let Some(&b) = text.get(i) {
        let offset = next_offset;
        if piece.counted && b & 0xC0 != 0x80 {
            next_offset = next_offset.wrapping_add(1);
        }
        let rest = text.get(i..).unwrap_or(&[]);

        if initial {
            match take_initial(words, rest, piece.footnote_link) {
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

    use super::{InitialLetter, MAX_WORD, Piece, Scan, Words, scan};
    use std::string::String;
    use std::vec::Vec;

    #[derive(Default)]
    struct Paragraph {
        buf: Vec<u8>,
        offset: u32,
        continues: bool,
        out: Vec<(String, u32, bool)>,
        breaks: usize,
        letter: Option<InitialLetter>,
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
        fn initial(&mut self) -> InitialLetter {
            self.letter.unwrap_or(InitialLetter::NONE)
        }
        fn set_initial(&mut self, letter: &InitialLetter) {
            self.letter = Some(*letter);
        }
        fn block_empty(&mut self) -> bool {
            self.out.is_empty()
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
        footnote_link: false,
    };

    fn first_letter() -> InitialLetter {
        InitialLetter {
            pending: true,
            lines: 3,
            ..InitialLetter::NONE
        }
    }

    fn span() -> InitialLetter {
        InitialLetter {
            span_depth: 4,
            ..InitialLetter::NONE
        }
    }

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

    // The captured text and the words, after scanning `text` with `letter` armed.
    fn initial(text: &str, letter: InitialLetter) -> (String, Vec<String>) {
        let mut p = Paragraph {
            letter: Some(letter),
            ..Paragraph::default()
        };
        let piece = COUNTED;
        assert_eq!(scan(text.as_bytes(), piece, &mut p), Scan::Done);
        p.flush();
        let l = p.letter.unwrap();
        (
            String::from_utf8_lossy(&l.text[..usize::from(l.len)]).into_owned(),
            p.out.into_iter().map(|w| w.0).collect(),
        )
    }

    #[test]
    fn a_first_letter_takes_leading_punctuation_and_one_letter() {
        let (cap, words) = initial(" \u{a1}Hola mundo", first_letter());
        assert_eq!(cap, "\u{a1}H");
        assert_eq!(words, ["ola", "mundo"]);
        // Three dots fill it: the letter gives up and its text goes back into the word.
        let (cap, words) = initial("...Hola", first_letter());
        assert_eq!((cap.as_str(), words[0].as_str()), ("", "...Hola"));
    }

    #[test]
    fn a_span_takes_up_to_three_codepoints_in_twelve_bytes() {
        assert_eq!(initial("ABC", span()).0, "ABC");
        let (cap, words) = initial("ABCD", span());
        assert_eq!((cap.as_str(), words[0].as_str()), ("", "ABCD"));
        assert_eq!(initial("\u{1f600}\u{1f601}\u{1f602}", span()).0.len(), 12);
        let (cap, words) = initial("A B", span());
        assert_eq!((cap.as_str(), words.join("|").as_str()), ("", "A|B"));
        // A footnote link never starts a ::first-letter letter.
        let mut p = Paragraph {
            letter: Some(first_letter()),
            ..Paragraph::default()
        };
        let piece = Piece {
            footnote_link: true,
            ..COUNTED
        };
        assert_eq!(scan(b"12", piece, &mut p), Scan::Done);
        assert_eq!(p.letter.unwrap().len, 0);
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
                letter: [None, Some(first_letter()), Some(span())][(next() % 3) as usize],
                stop_at_break: next() % 2 == 0,
                ..Paragraph::default()
            };
            let piece = Piece {
                counted: next() % 2 == 0,
                start: next(),
                preserve: next() % 2 == 0,
                footnote_link: next() % 4 == 0,
            };
            let _ = scan(&text, piece, &mut p);
            assert!(p.buf.len() <= MAX_WORD + 1);
        }
    }

    fn collapsed(text: &[u8]) -> std::vec::Vec<u8> {
        let mut buf = text.to_vec();
        let n = super::collapse_space(&mut buf);
        buf.truncate(n);
        buf
    }

    #[test]
    fn ruby_text_is_trimmed_and_collapsed() {
        assert_eq!(collapsed(b""), b"");
        assert_eq!(collapsed(b" \t\r\n "), b"");
        assert_eq!(collapsed(b"  a \t\n b\r\nc  "), b"a b c");
        assert_eq!(collapsed(b"a\xC2\xA0 b"), b"a\xC2\xA0 b");
        assert_eq!(collapsed(b"\x0ba\x0c"), b"\x0ba\x0c");
        assert_eq!(collapsed(b" x"), b"x");
    }
}
