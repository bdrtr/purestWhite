//! A page's body in the section file (`test/page_codec` pins it). Packed, little-endian: an
//! element count (u16); each element's tag (u8) and fields; then the footnotes (a u16 count, then
//! a 32-byte number and a 256-byte href each) and the links (a u16 count, then a 256-byte href and
//! x, y, width and height as i16 each). Strings in fixed fields are NUL-terminated and zero-padded;
//! other strings are a u32 length, then the bytes.
//!
//! - A line ([`LINE`]): x, y (i16), then a text block: the word count (u16), whether it has focus
//!   splits (u8), its text bytes (u16) and the paragraph's first word (u16, `u16::MAX` for none);
//!   the arena (see [`arena_size`]); a ruby string per word (u32 length, bytes); the
//!   [`BlockStyle`]; and a word-font flag (u8) with, when set, three font ids (i32) and a slot per
//!   word.
//! - An image ([`IMAGE`]): x, y, its cache path and source href (strings), width and height (i16).
//! - A rule ([`RULE`]): x, y, width (u16) and thickness (u8).
//! - A drop cap ([`DROP_CAP`]): x, y, a font id (i32), a scale (u16, 1/256), a style (u8) and its
//!   text (a u8 length, bytes).
//! - A border box ([`BORDER_BOX`]): x, y, width and height (u16), four sides (width and style, u8
//!   each) and whether it is shaded (u8).
//!
//! A fixed field the file ends in reads as zero, and so does every field after it; a cut ruby or
//! image string reads as empty. A cut arena, word-font table, drop cap, footnote or link makes the
//! page unreadable.

use crate::{Sink, Source};
use bembeyaz_io::{Cursor, Out};

pub const LINE: u8 = 1;
pub const IMAGE: u8 = 2;
pub const RULE: u8 = 3;
pub const DROP_CAP: u8 = 4;
pub const BORDER_BOX: u8 = 5;

pub const MAX_FOOTNOTES: usize = 16;
pub const MAX_LINKS: usize = 32;
pub const NUMBER_LEN: usize = 32;
pub const HREF_LEN: usize = 256;
/// A footnote: its number and href, NUL-terminated. `bb_page_footnote` in C.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Footnote {
    pub number: [u8; NUMBER_LEN],
    pub href: [u8; HREF_LEN],
}

/// A link: its href, NUL-terminated, and its box. `bb_page_link` in C.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub href: [u8; HREF_LEN],
    pub x: i16,
    pub y: i16,
    pub width: i16,
    pub height: i16,
}
pub const MAX_WORDS: u16 = 10_000;
pub const MAX_DROP_CAP: usize = 12;
pub const WORD_FONTS: usize = 3;
const SMALL_CAPS_SLOT: u8 = 0x80;
// None, solid, double, dotted, dashed.
const BORDER_STYLES: u8 = 5;

/// A text block's counts. `bb_page_text` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct TextHeader {
    pub word_count: u16,
    pub focus: bool,
    pub text_bytes: u16,
    pub paragraph_start: u16,
}

/// The bytes of a text block's arena: one array per field, each `word_count` long, in order:
/// source ranges (two u32), text offsets (u16), x (i16), [focus suffix x (u16)], styles (u8),
/// [focus boundaries (u8)]; then the text, each word NUL-terminated.
#[must_use]
pub fn arena_size(h: &TextHeader) -> usize {
    let words = usize::from(h.word_count);
    let focus = if h.focus { words * 3 } else { 0 };
    words * (8 + 2 + 2 + 1) + focus + usize::from(h.text_bytes)
}

/// A line's layout style. `bb_page_block_style` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct BlockStyle {
    pub alignment: u8,
    pub text_align_defined: bool,
    pub margin_top: i16,
    pub margin_bottom: i16,
    pub margin_left: i16,
    pub margin_right: i16,
    pub padding_top: i16,
    pub padding_bottom: i16,
    pub padding_left: i16,
    pub padding_right: i16,
    pub text_indent: i16,
    pub text_indent_defined: bool,
    pub is_rtl: bool,
    pub direction_defined: bool,
    pub character_spacing: i8,
    pub font_id: i32,
}

/// A border box side. `bb_page_side` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Side {
    pub width: u8,
    pub style: u8,
}

/// Makes what [`read`] finds. Each method that gives room for bytes gives exactly as many, or
/// None when it cannot; each that takes an element says whether it could keep it.
pub trait Builder {
    /// The page has `count` elements.
    fn begin(&mut self, count: u16);
    /// A line at (x, y): room for its arena of `arena_size` bytes (empty without words).
    fn line(&mut self, x: i16, y: i16, text: &TextHeader, arena_size: usize) -> Option<&mut [u8]>;
    /// Room for the current line's ruby on `word`: only for a ruby that is not empty, at most once
    /// a word, in word order.
    fn ruby(&mut self, word: u16, len: usize) -> Option<&mut [u8]>;
    /// Room for the current line's font ids and word slots.
    fn word_fonts(&mut self, size: usize) -> Option<&mut [u8]>;
    /// The current line's style; the line is complete.
    fn line_end(&mut self, style: &BlockStyle) -> bool;
    /// Room for the next image's cache path (`source` false) or source href, when not empty.
    fn image_text(&mut self, source: bool, len: usize) -> Option<&mut [u8]>;
    fn image(&mut self, x: i16, y: i16, width: i16, height: i16) -> bool;
    fn rule(&mut self, x: i16, y: i16, width: u16, thickness: u8) -> bool;
    #[allow(clippy::too_many_arguments)]
    fn drop_cap(
        &mut self,
        x: i16,
        y: i16,
        font_id: i32,
        scale: u16,
        style: u8,
        text: &[u8],
    ) -> bool;
    #[allow(clippy::too_many_arguments)]
    fn border_box(
        &mut self,
        x: i16,
        y: i16,
        width: u16,
        height: u16,
        sides: &[Side; 4],
        shaded: bool,
    ) -> bool;
    /// Room for the page's `count` footnotes (only when there are some).
    fn footnotes(&mut self, count: usize) -> Option<&mut [Footnote]>;
    /// Room for the page's `count` links (only when there are some).
    fn links(&mut self, count: usize) -> Option<&mut [Link]>;
}

/// Why a page could not be read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    Tag(u8),
    /// Too many words, or a paragraph start past them.
    Words,
    /// Text bytes that cannot hold the words.
    TextSize,
    /// The arena was cut short.
    Arena,
    /// Word offsets out of order, out of the text or not after a NUL.
    Layout,
    WordFontFlag(u8),
    /// The word fonts were cut short.
    WordFonts,
    WordFontSlot(u8),
    Rule,
    DropCap,
    BorderStyle(u8),
    BorderBox,
    FootnoteCount(u16),
    Footnote,
    LinkCount(u16),
    Link,
    /// The builder could not make or keep something.
    Memory,
}

const MESSAGES: [&core::ffi::CStr; 18] = [
    c"read",
    c"unknown element tag",
    c"word count or paragraph start out of range",
    c"text size does not fit the words",
    c"text block cut short",
    c"corrupt word offsets",
    c"bad word font flag",
    c"word fonts cut short",
    c"bad word font slot",
    c"invalid horizontal rule",
    c"invalid drop cap",
    c"invalid border style",
    c"empty border box",
    c"too many footnotes",
    c"footnote cut short",
    c"too many links",
    c"invalid link",
    c"out of memory",
];

impl Error {
    /// A number for the C side, from 1; 0 is success.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Error::Tag(_) => 1,
            Error::Words => 2,
            Error::TextSize => 3,
            Error::Arena => 4,
            Error::Layout => 5,
            Error::WordFontFlag(_) => 6,
            Error::WordFonts => 7,
            Error::WordFontSlot(_) => 8,
            Error::Rule => 9,
            Error::DropCap => 10,
            Error::BorderStyle(_) => 11,
            Error::BorderBox => 12,
            Error::FootnoteCount(_) => 13,
            Error::Footnote => 14,
            Error::LinkCount(_) => 15,
            Error::Link => 16,
            Error::Memory => 17,
        }
    }

    /// What a [`code`](Self::code) (or 0, success) means.
    #[must_use]
    pub fn message(code: i32) -> &'static core::ffi::CStr {
        usize::try_from(code)
            .ok()
            .and_then(|i| MESSAGES.get(i))
            .copied()
            .unwrap_or(c"unknown")
    }
}

fn check_layout(arena: &[u8], h: TextHeader) -> Result<(), Error> {
    let words = usize::from(h.word_count);
    let text_bytes = usize::from(h.text_bytes);
    let offsets = arena.get(words * 8..words * 10).ok_or(Error::Layout)?;
    let text = arena
        .get(arena.len().saturating_sub(text_bytes)..)
        .filter(|t| t.len() == text_bytes)
        .ok_or(Error::Layout)?;
    let offsets = offsets.as_chunks::<2>().0;
    let first = offsets.first().map(|o| u16::from_le_bytes(*o));
    if first != Some(0) || text.last() != Some(&0) {
        return Err(Error::Layout);
    }
    let mut previous = 0usize;
    for o in offsets.iter().skip(1) {
        let at = usize::from(u16::from_le_bytes(*o));
        if at <= previous || at >= text_bytes || text.get(at - 1) != Some(&0) {
            return Err(Error::Layout);
        }
        previous = at;
    }
    Ok(())
}

fn check_word_fonts(fonts: &[u8]) -> Result<(), Error> {
    let (ids, slots) = fonts
        .split_at_checked(4 * WORD_FONTS)
        .ok_or(Error::WordFonts)?;
    let ids = ids.as_chunks::<4>().0;
    for &slot in slots {
        let index = slot & !SMALL_CAPS_SLOT;
        let font = match index {
            0 => 1,
            _ => ids
                .get(usize::from(index) - 1)
                .map_or(0, |b| i32::from_le_bytes(*b)),
        };
        if usize::from(index) > WORD_FONTS || font == 0 {
            return Err(Error::WordFontSlot(slot));
        }
    }
    Ok(())
}

fn read_text<S: Source>(
    cur: &mut Cursor<S>,
    builder: &mut impl Builder,
    x: i16,
    y: i16,
) -> Result<(), Error> {
    let h = TextHeader {
        word_count: cur.u16(),
        focus: cur.u8() != 0,
        text_bytes: cur.u16(),
        paragraph_start: cur.u16(),
    };
    let words = h.word_count;
    if words > MAX_WORDS || (h.paragraph_start != u16::MAX && h.paragraph_start >= words) {
        return Err(Error::Words);
    }
    if (words == 0 && h.text_bytes != 0) || (words > 0 && h.text_bytes < words) {
        return Err(Error::TextSize);
    }
    let arena = builder
        .line(x, y, &h, arena_size(&h))
        .ok_or(Error::Memory)?;
    if !arena.is_empty() {
        if !cur.read(arena) {
            return Err(Error::Arena);
        }
        check_layout(arena, h)?;
    }
    for word in 0..words {
        // One the file cannot hold reads as no ruby, and so does every read after it.
        let Some(len) = cur.string_len().filter(|&len| len > 0) else {
            continue;
        };
        let room = builder.ruby(word, len).ok_or(Error::Memory)?;
        let _ = cur.read(room);
    }
    let style = BlockStyle {
        alignment: cur.u8(),
        text_align_defined: cur.u8() != 0,
        margin_top: cur.i16(),
        margin_bottom: cur.i16(),
        margin_left: cur.i16(),
        margin_right: cur.i16(),
        padding_top: cur.i16(),
        padding_bottom: cur.i16(),
        padding_left: cur.i16(),
        padding_right: cur.i16(),
        text_indent: cur.i16(),
        text_indent_defined: cur.u8() != 0,
        is_rtl: cur.u8() != 0,
        direction_defined: cur.u8() != 0,
        character_spacing: cur.i8(),
        font_id: cur.i32(),
    };
    match cur.u8() {
        0 => {}
        1 => {
            let room = builder
                .word_fonts(4 * WORD_FONTS + usize::from(words))
                .ok_or(Error::Memory)?;
            if !cur.read(room) {
                return Err(Error::WordFonts);
            }
            check_word_fonts(room)?;
        }
        flag => return Err(Error::WordFontFlag(flag)),
    }
    if builder.line_end(&style) {
        Ok(())
    } else {
        Err(Error::Memory)
    }
}

fn keep(kept: bool) -> Result<(), Error> {
    if kept { Ok(()) } else { Err(Error::Memory) }
}

fn read_element<S: Source>(cur: &mut Cursor<S>, builder: &mut impl Builder) -> Result<(), Error> {
    let tag = cur.u8();
    if !(LINE..=BORDER_BOX).contains(&tag) {
        return Err(Error::Tag(tag));
    }
    let (x, y) = (cur.i16(), cur.i16());
    match tag {
        LINE => read_text(cur, builder, x, y),
        IMAGE => {
            for source in [false, true] {
                if let Some(len) = cur.string_len().filter(|&len| len > 0) {
                    let room = builder.image_text(source, len).ok_or(Error::Memory)?;
                    let _ = cur.read(room);
                }
            }
            let (width, height) = (cur.i16(), cur.i16());
            keep(builder.image(x, y, width, height))
        }
        RULE => {
            let (width, thickness) = (cur.u16(), cur.u8());
            if width == 0 || thickness == 0 {
                return Err(Error::Rule);
            }
            keep(builder.rule(x, y, width, thickness))
        }
        DROP_CAP => {
            let (font_id, scale, style, len) =
                (cur.i32(), cur.u16(), cur.u8(), usize::from(cur.u8()));
            let mut text = [0u8; MAX_DROP_CAP];
            let text = text
                .get_mut(..len)
                .filter(|t| !t.is_empty() && scale != 0)
                .ok_or(Error::DropCap)?;
            if !cur.read(text) {
                return Err(Error::DropCap);
            }
            keep(builder.drop_cap(x, y, font_id, scale, style, text))
        }
        _ => {
            let (width, height) = (cur.u16(), cur.u16());
            let mut sides = [Side::default(); 4];
            for side in &mut sides {
                *side = Side {
                    width: cur.u8(),
                    style: cur.u8(),
                };
                if side.style >= BORDER_STYLES {
                    return Err(Error::BorderStyle(side.style));
                }
            }
            let shaded = cur.u8() != 0;
            if width == 0 || height == 0 {
                return Err(Error::BorderBox);
            }
            keep(builder.border_box(x, y, width, height, &sides, shaded))
        }
    }
}

/// Reads the page at `at`, handing what it holds to `builder`. Returns where it ends.
///
/// # Errors
///
/// What makes the page unreadable; `builder` may have been given part of it.
pub fn read(file: &mut impl Source, at: u32, builder: &mut impl Builder) -> Result<u32, Error> {
    let mut buf = [0u8; 256];
    let mut cur = Cursor::new(file, at, &mut buf);
    let count = cur.u16();
    builder.begin(count);
    for _ in 0..count {
        read_element(&mut cur, builder)?;
    }
    let count = usize::from(cur.u16());
    if count > MAX_FOOTNOTES {
        return Err(Error::FootnoteCount(
            u16::try_from(count).unwrap_or(u16::MAX),
        ));
    }
    if count > 0 {
        for f in builder.footnotes(count).ok_or(Error::Memory)? {
            if !cur.read(&mut f.number) || !cur.read(&mut f.href) {
                return Err(Error::Footnote);
            }
            f.number[NUMBER_LEN - 1] = 0;
            f.href[HREF_LEN - 1] = 0;
        }
    }
    let count = usize::from(cur.u16());
    if count > MAX_LINKS {
        return Err(Error::LinkCount(u16::try_from(count).unwrap_or(u16::MAX)));
    }
    if count > 0 {
        for l in builder.links(count).ok_or(Error::Memory)? {
            if !cur.read(&mut l.href) {
                return Err(Error::Link);
            }
            l.href[HREF_LEN - 1] = 0;
            (l.x, l.y, l.width, l.height) = (cur.i16(), cur.i16(), cur.i16(), cur.i16());
            if l.href[0] == 0 || l.width <= 0 || l.height <= 0 {
                return Err(Error::Link);
            }
        }
    }
    Ok(cur.position())
}

/// Reads a text block alone at `at` (the builder's line is at 0, 0). Returns where it ends.
///
/// # Errors
///
/// As [`read`].
pub fn read_text_block(
    file: &mut impl Source,
    at: u32,
    builder: &mut impl Builder,
) -> Result<u32, Error> {
    let mut buf = [0u8; 256];
    let mut cur = Cursor::new(file, at, &mut buf);
    read_text(&mut cur, builder, 0, 0)?;
    Ok(cur.position())
}

/// A text block to write.
pub struct Line<'a> {
    pub text: TextHeader,
    /// [`arena_size`] bytes.
    pub arena: &'a [u8],
    /// Each word's ruby, empty for none.
    pub ruby: &'a dyn Fn(u16) -> &'a [u8],
    pub style: BlockStyle,
    /// The font ids and a slot per word, when the words use other fonts.
    pub word_fonts: Option<&'a [u8]>,
}

fn put_text<S: Sink>(out: &mut Out<S>, line: &Line) {
    let h = &line.text;
    out.push(&h.word_count.to_le_bytes());
    out.push(&[u8::from(h.focus)]);
    out.push(&h.text_bytes.to_le_bytes());
    out.push(&h.paragraph_start.to_le_bytes());
    if h.word_count > 0 {
        out.push(line.arena);
    }
    for word in 0..h.word_count {
        let ruby = (line.ruby)(word);
        out.push(&u32::try_from(ruby.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.push(ruby);
    }
    let s = &line.style;
    out.push(&[s.alignment, u8::from(s.text_align_defined)]);
    for v in [
        s.margin_top,
        s.margin_bottom,
        s.margin_left,
        s.margin_right,
        s.padding_top,
        s.padding_bottom,
        s.padding_left,
        s.padding_right,
        s.text_indent,
    ] {
        out.push(&v.to_le_bytes());
    }
    out.push(&[
        u8::from(s.text_indent_defined),
        u8::from(s.is_rtl),
        u8::from(s.direction_defined),
        s.character_spacing.to_le_bytes()[0],
    ]);
    out.push(&s.font_id.to_le_bytes());
    match line.word_fonts {
        Some(fonts) => {
            out.push(&[1]);
            out.push(fonts);
        }
        None => out.push(&[0]),
    }
}

// Writes from `at` with `put`; where the writing ended, unless a write failed.
fn write<S: Sink>(sink: &mut S, at: u32, put: impl FnOnce(&mut Out<S>)) -> Option<u32> {
    let mut out = Out::new(sink, at);
    put(&mut out);
    out.finish()
}

/// Writes a page's element count at `at`. Returns where the writing ended, unless it failed.
pub fn write_count(sink: &mut impl Sink, at: u32, count: u16) -> Option<u32> {
    write(sink, at, |out| out.push(&count.to_le_bytes()))
}

fn put_xy<S: Sink>(out: &mut Out<S>, tag: u8, x: i16, y: i16) {
    out.push(&[tag]);
    out.push(&x.to_le_bytes());
    out.push(&y.to_le_bytes());
}

/// Writes a line at (x, y). As [`write_count`].
pub fn write_line(sink: &mut impl Sink, at: u32, x: i16, y: i16, line: &Line) -> Option<u32> {
    write(sink, at, |out| {
        put_xy(out, LINE, x, y);
        put_text(out, line);
    })
}

/// Writes a text block alone. As [`write_count`].
pub fn write_text_block(sink: &mut impl Sink, at: u32, line: &Line) -> Option<u32> {
    write(sink, at, |out| put_text(out, line))
}

/// Writes an image. As [`write_count`].
#[allow(clippy::too_many_arguments)]
pub fn write_image(
    sink: &mut impl Sink,
    at: u32,
    x: i16,
    y: i16,
    path: &[u8],
    source: &[u8],
    width: i16,
    height: i16,
) -> Option<u32> {
    write(sink, at, |out| {
        put_xy(out, IMAGE, x, y);
        out.push_string(path);
        out.push_string(source);
        out.push(&width.to_le_bytes());
        out.push(&height.to_le_bytes());
    })
}

/// Writes a rule. As [`write_count`].
pub fn write_rule(
    sink: &mut impl Sink,
    at: u32,
    x: i16,
    y: i16,
    width: u16,
    thickness: u8,
) -> Option<u32> {
    write(sink, at, |out| {
        put_xy(out, RULE, x, y);
        out.push(&width.to_le_bytes());
        out.push(&[thickness]);
    })
}

/// Writes a drop cap with at most [`MAX_DROP_CAP`] bytes of `text`. As [`write_count`].
#[allow(clippy::too_many_arguments)]
pub fn write_drop_cap(
    sink: &mut impl Sink,
    at: u32,
    x: i16,
    y: i16,
    font_id: i32,
    scale: u16,
    style: u8,
    text: &[u8],
) -> Option<u32> {
    let text = text.get(..MAX_DROP_CAP).unwrap_or(text);
    write(sink, at, |out| {
        put_xy(out, DROP_CAP, x, y);
        out.push(&font_id.to_le_bytes());
        out.push(&scale.to_le_bytes());
        out.push(&[style, u8::try_from(text.len()).unwrap_or(0)]);
        out.push(text);
    })
}

/// Writes a border box. As [`write_count`].
#[allow(clippy::too_many_arguments)]
pub fn write_border_box(
    sink: &mut impl Sink,
    at: u32,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
    sides: &[Side; 4],
    shaded: bool,
) -> Option<u32> {
    write(sink, at, |out| {
        put_xy(out, BORDER_BOX, x, y);
        out.push(&width.to_le_bytes());
        out.push(&height.to_le_bytes());
        for side in sides {
            out.push(&[side.width, side.style]);
        }
        out.push(&[u8::from(shaded)]);
    })
}

/// Writes the first [`MAX_FOOTNOTES`] footnotes and [`MAX_LINKS`] links. As [`write_count`].
pub fn write_notes(
    sink: &mut impl Sink,
    at: u32,
    footnotes: &[Footnote],
    links: &[Link],
) -> Option<u32> {
    let footnotes = footnotes.get(..MAX_FOOTNOTES).unwrap_or(footnotes);
    let links = links.get(..MAX_LINKS).unwrap_or(links);
    write(sink, at, |out| {
        out.push(&u16::try_from(footnotes.len()).unwrap_or(0).to_le_bytes());
        for f in footnotes {
            out.push(&f.number);
            out.push(&f.href);
        }
        out.push(&u16::try_from(links.len()).unwrap_or(0).to_le_bytes());
        for l in links {
            out.push(&l.href);
            for v in [l.x, l.y, l.width, l.height] {
                out.push(&v.to_le_bytes());
            }
        }
    })
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{
        BlockStyle, Builder, Error, Footnote, HREF_LEN, Line, Link, NUMBER_LEN, Side, TextHeader,
        arena_size, read, read_text_block, write_border_box, write_count, write_drop_cap,
        write_image, write_line, write_notes, write_rule, write_text_block,
    };
    use std::format;
    use std::string::String;
    use std::vec;
    use std::vec::Vec;

    use bembeyaz_io::testing::Bytes;

    // Records what it is given, as text, and keeps the bytes it gave room for.
    #[derive(Default)]
    struct Log {
        events: Vec<String>,
        arena: Vec<u8>,
        ruby: Vec<u8>,
        fonts: Vec<u8>,
        image: [Vec<u8>; 2],
        notes: Vec<Footnote>,
        targets: Vec<Link>,
        refuse_line: bool,
    }

    impl Builder for Log {
        fn begin(&mut self, count: u16) {
            self.events.push(format!("begin {count}"));
        }
        fn line(&mut self, x: i16, y: i16, text: &TextHeader, size: usize) -> Option<&mut [u8]> {
            if self.refuse_line {
                return None;
            }
            self.events.push(format!("line {x} {y} {text:?}"));
            self.arena = vec![0; size];
            Some(&mut self.arena)
        }
        fn ruby(&mut self, word: u16, len: usize) -> Option<&mut [u8]> {
            self.events.push(format!("ruby {word}"));
            self.ruby = vec![0; len];
            Some(&mut self.ruby)
        }
        fn word_fonts(&mut self, size: usize) -> Option<&mut [u8]> {
            self.fonts = vec![0; size];
            Some(&mut self.fonts)
        }
        fn line_end(&mut self, style: &BlockStyle) -> bool {
            self.events.push(format!(
                "end {:?} arena {:?} ruby {:?} fonts {:?}",
                style, self.arena, self.ruby, self.fonts
            ));
            true
        }
        fn image_text(&mut self, source: bool, len: usize) -> Option<&mut [u8]> {
            let slot = &mut self.image[usize::from(source)];
            *slot = vec![0; len];
            Some(slot)
        }
        fn image(&mut self, x: i16, y: i16, width: i16, height: i16) -> bool {
            self.events
                .push(format!("image {x} {y} {width} {height} {:?}", self.image));
            self.image = [Vec::new(), Vec::new()];
            true
        }
        fn rule(&mut self, x: i16, y: i16, width: u16, thickness: u8) -> bool {
            self.events
                .push(format!("rule {x} {y} {width} {thickness}"));
            true
        }
        fn drop_cap(&mut self, x: i16, y: i16, font: i32, scale: u16, style: u8, t: &[u8]) -> bool {
            self.events
                .push(format!("cap {x} {y} {font} {scale} {style} {t:?}"));
            true
        }
        fn border_box(
            &mut self,
            x: i16,
            y: i16,
            width: u16,
            height: u16,
            sides: &[Side; 4],
            shaded: bool,
        ) -> bool {
            self.events
                .push(format!("box {x} {y} {width} {height} {sides:?} {shaded}"));
            true
        }
        fn footnotes(&mut self, count: usize) -> Option<&mut [Footnote]> {
            self.events.push(format!("notes {count}"));
            self.notes = vec![
                Footnote {
                    number: [9; NUMBER_LEN],
                    href: [9; HREF_LEN]
                };
                count
            ];
            Some(&mut self.notes)
        }
        fn links(&mut self, count: usize) -> Option<&mut [Link]> {
            self.events.push(format!("links {count}"));
            self.targets = vec![
                Link {
                    href: [0; HREF_LEN],
                    x: 0,
                    y: 0,
                    width: 0,
                    height: 0
                };
                count
            ];
            Some(&mut self.targets)
        }
    }

    // Two words, "ab" and "c", with focus splits, a ruby on the second and word fonts.
    fn line_parts() -> (TextHeader, Vec<u8>, Vec<u8>) {
        let text = TextHeader {
            word_count: 2,
            focus: true,
            text_bytes: 5,
            paragraph_start: 0,
        };
        let mut arena = vec![0u8; 16]; // source ranges
        arena.extend([0, 0, 3, 0]); // offsets
        arena.extend([1, 0, 9, 0]); // x
        arena.extend([2, 0, 0, 0]); // suffix x
        arena.extend([1, 0]); // styles
        arena.extend([1, 0]); // boundaries
        arena.extend(b"ab\0c\0");
        assert_eq!(arena.len(), arena_size(&text));
        let mut fonts = Vec::from(7i32.to_le_bytes());
        fonts.extend([0; 8]);
        fonts.extend([0, 1]);
        (text, arena, fonts)
    }

    fn sample() -> Vec<u8> {
        let (text, arena, fonts) = line_parts();
        let ruby = |w: u16| -> &[u8] { if w == 1 { b"rt" } else { b"" } };
        let line = Line {
            text,
            arena: &arena,
            ruby: &ruby,
            style: BlockStyle {
                alignment: 2,
                margin_top: -1,
                is_rtl: true,
                font_id: 5,
                ..BlockStyle::default()
            },
            word_fonts: Some(&fonts),
        };
        let mut file = Bytes::new(Vec::from(*b"xx"));
        let mut at = write_count(&mut file, 2, 5).unwrap();
        at = write_line(&mut file, at, 3, 4, &line).unwrap();
        at = write_image(&mut file, at, 1, 2, b"img.pxc", b"", 30, 20).unwrap();
        at = write_rule(&mut file, at, 0, 9, 100, 2).unwrap();
        at = write_drop_cap(&mut file, at, 0, 8, 6, 512, 1, b"A").unwrap();
        let sides = [Side { width: 1, style: 1 }; 4];
        at = write_border_box(&mut file, at, 0, 0, 10, 10, &sides, true).unwrap();
        let mut note = Footnote {
            number: [0; NUMBER_LEN],
            href: [0; HREF_LEN],
        };
        note.number[0] = b'1';
        let mut target = Link {
            href: [0; HREF_LEN],
            x: 0,
            y: 0,
            width: 5,
            height: 5,
        };
        target.href[0] = b'#';
        at = write_notes(&mut file, at, &[note], &[target]).unwrap();
        assert_eq!(at as usize, file.data.len());
        file.data
    }

    #[test]
    fn a_written_page_reads_back() {
        let mut file = Bytes::new(sample());
        let mut log = Log::default();
        let end = read(&mut file, 2, &mut log).unwrap();
        assert_eq!(end as usize, file.data.len());
        assert_eq!(log.events.len(), 10);
        assert_eq!(log.events[0], "begin 5");
        assert!(log.events[1].starts_with("line 3 4"));
        assert_eq!(log.events[2], "ruby 1");
        assert!(
            log.events[3].contains("is_rtl: true") && log.events[3].contains("ruby [114, 116]")
        );
        assert_eq!(
            log.events[4],
            "image 1 2 30 20 [[105, 109, 103, 46, 112, 120, 99], []]"
        );
        assert_eq!(log.events[5], "rule 0 9 100 2");
        assert_eq!(log.events[6], "cap 0 8 6 512 1 [65]");
        assert!(log.events[7].starts_with("box 0 0 10 10"));
        assert_eq!(log.events[8], "notes 1");
        assert_eq!(log.events[9], "links 1");
        assert_eq!(
            (&log.notes[0].number[..2], log.notes[0].href[HREF_LEN - 1]),
            (&[b'1', 0][..], 0)
        );
        let l = &log.targets[0];
        assert_eq!((l.href[0], l.x, l.y, l.width, l.height), (b'#', 0, 0, 5, 5));
    }

    #[test]
    fn a_text_block_reads_back_alone() {
        let (text, arena, _) = line_parts();
        let ruby = |_: u16| -> &[u8] { b"" };
        let line = Line {
            text,
            arena: &arena,
            ruby: &ruby,
            style: BlockStyle::default(),
            word_fonts: None,
        };
        let mut file = Bytes::new(Vec::new());
        let end = write_text_block(&mut file, 0, &line).unwrap();
        let mut log = Log::default();
        assert_eq!(read_text_block(&mut file, 0, &mut log), Ok(end));
        assert!(log.events[0].starts_with("line 0 0"));
    }

    #[test]
    fn damage_is_refused() {
        let good = sample();
        let line = 2 + 2 + 5; // after the count, the tag and x, y
        let offsets = line + 7 + 16;
        let text = offsets + 4 + 4 + 4 + 2 + 2;
        let damaged = |at: usize, value: u8| {
            let mut bytes = good.clone();
            bytes[at] = value;
            let mut log = Log::default();
            read(&mut Bytes::new(bytes), 2, &mut log)
        };
        assert_eq!(damaged(line + 1, 0x30).err(), Some(Error::Words)); // 12290 words
        assert_eq!(damaged(line + 5, 2).err(), Some(Error::Words)); // paragraph start
        assert_eq!(damaged(line + 3, 1).err(), Some(Error::TextSize));
        assert_eq!(damaged(offsets, 1).err(), Some(Error::Layout));
        assert_eq!(damaged(offsets + 2, 5).err(), Some(Error::Layout));
        assert_eq!(damaged(text + 2, b'x').err(), Some(Error::Layout));
        assert_eq!(damaged(text + 4, b'x').err(), Some(Error::Layout));
        assert_eq!(damaged(4, 9).err(), Some(Error::Tag(9)));
        let mut log = Log {
            refuse_line: true,
            ..Log::default()
        };
        assert_eq!(
            read(&mut Bytes::new(good.clone()), 2, &mut log),
            Err(Error::Memory)
        );
        // Cut inside the arena.
        assert_eq!(
            read(
                &mut Bytes::new(good[..text].to_vec()),
                2,
                &mut Log::default()
            ),
            Err(Error::Arena)
        );
    }

    #[test]
    fn any_page_never_panics() {
        let good = sample();
        let mut seed = 0x9a6e_c0de_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..3000 {
            let mut bytes = good.clone();
            for _ in 0..=next() % 6 {
                let at = next() as usize % bytes.len();
                bytes[at] = next() as u8;
            }
            if next() % 3 == 0 {
                bytes.truncate(next() as usize % bytes.len());
            }
            let mut log = Log::default();
            let _ = read(&mut Bytes::new(bytes.clone()), 2, &mut log);
            let _ = read_text_block(&mut Bytes::new(bytes), next() % 64, &mut Log::default());
        }
    }
}
