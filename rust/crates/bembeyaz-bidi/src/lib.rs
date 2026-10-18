//! Bidirectional text for the reader: the Unicode Bidirectional Algorithm (UAX #9) as CrossPoint
//! applies it to one line, Arabic contextual shaping, and the placement of Hebrew and Arabic marks.
//!
//! Behaviour matches the C minibidi this replaces (`lib/MiniBidi/minibidi.c` up to fdd0780),
//! including where that differs from UAX #9: `test/bidi_utils` pins it, and each fix is its own
//! change. One such departure kept here: text ends at its first malformed UTF-8 sequence. A line
//! holds at most [`MAX_LINE`] characters. The text comes from books: nothing here allocates or
//! panics.
//!
//! The algorithm and tables come from minibidi by Ahmad Khalifa (MIT licence), as changed in
//! mintty by Thomas Wolff.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

mod reorder;
mod shape;
mod tables;

use bembeyaz_utf8::{REPLACEMENT, codepoints};
use core::cmp::Ordering;

/// Characters in one line; longer text is left as it is.
pub const MAX_LINE: usize = 128;

/// Bidi classes of UAX #9, table 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[allow(clippy::upper_case_acronyms, reason = "the names UAX #9 uses")]
pub enum Class {
    L,
    LRE,
    LRO,
    R,
    AL,
    RLE,
    RLO,
    PDF,
    EN,
    ES,
    ET,
    AN,
    CS,
    NSM,
    BN,
    B,
    S,
    WS,
    ON,
    LRI,
    RLI,
    FSI,
    PDI,
}

/// Arabic joining types (ArabicShaping.txt): which sides a letter connects on. No letter the
/// tables cover joins on the left only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Join {
    Right,
    Dual,
    None,
    Causing,
}

/// How a [`tables::PAIRS`] entry takes part in reordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pair {
    /// Mirrors only.
    Mirror,
    /// Opens a bracket pair (rule N0) and mirrors.
    Open,
    /// Closes a bracket pair and mirrors.
    Close,
}

/// The joiners around a character, in [`Char::joiners`].
pub(crate) const ZWNJ: u8 = 0x01;
pub(crate) const ZWJ: u8 = 0x02;
/// Written over the Alef a Lam-Alef ligature absorbs; never output.
pub(crate) const LIGATURE_PLACEHOLDER: u32 = 0xFFFF;

/// One character of a line being reordered.
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub(crate) struct Char {
    pub(crate) cp: u32,
    /// Logical position, or the word it belongs to in [`word_order`].
    pub(crate) index: u16,
    /// ZWNJ/ZWJ next to it: low nibble those after it, high nibble those before.
    pub(crate) joiners: u8,
    /// The class of `cp` as read. Mirroring and shaping keep it (brackets and quotes are ON
    /// either way, presentation forms AL like their letters, marks are left alone; a test checks
    /// the tables), so it is looked up once.
    pub(crate) class: Class,
}

/// Storage for one line, owned by the caller so it need not live on the stack. All-zero bytes
/// are a valid value (`Class::L` is 0).
#[repr(C)]
pub struct Workspace {
    line: [Char; MAX_LINE],
    shaped: [u32; MAX_LINE],
}

impl Workspace {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            line: [Char {
                cp: 0,
                index: 0,
                joiners: 0,
                class: Class::ON,
            }; MAX_LINE],
            shaped: [0; MAX_LINE],
        }
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

/// The bidi class of `cp`; `ON` for codepoints the tables leave out.
#[must_use]
pub fn class(cp: u32) -> Class {
    if let Some(&class) = usize::try_from(cp).ok().and_then(|i| tables::ASCII.get(i)) {
        return class;
    }
    let Ok(cp) = u16::try_from(cp) else {
        return Class::ON;
    };
    tables::RANGES
        .binary_search_by(|&(first, last, _)| {
            if last < cp {
                Ordering::Less
            } else if first > cp {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .ok()
        .and_then(|i| tables::RANGES.get(i))
        .map_or(Class::ON, |&(_, _, class)| class)
}

fn pair(cp: u32) -> Option<(u32, Pair)> {
    let cp = u16::try_from(cp).ok()?;
    let i = tables::PAIRS
        .binary_search_by_key(&cp, |&(from, _, _)| from)
        .ok()?;
    tables::PAIRS
        .get(i)
        .map(|&(_, to, kind)| (u32::from(to), kind))
}

/// The glyph that mirrors `cp` in a right-to-left run (rule L4), or `cp`.
pub(crate) fn mirror(cp: u32) -> u32 {
    pair(cp).map_or(cp, |(to, _)| to)
}

/// For a bracket, the opening bracket of its pair (itself when it opens).
pub(crate) fn bracket(cp: u32) -> Option<u32> {
    match pair(cp)? {
        (_, Pair::Open) => Some(cp),
        (opener, Pair::Close) => Some(opener),
        (_, Pair::Mirror) => None,
    }
}

/// A Hebrew or Arabic combining mark: no advance of its own, drawn over the base before it, and
/// transparent to Arabic joining. Latin marks (U+0300..U+036F) are not: they have their own path.
#[must_use]
pub fn is_transparent_mark(cp: u32) -> bool {
    cp >= 0x0591 && class(cp) == Class::NSM
}

/// The codepoints of `text` up to its end or the first malformed sequence.
fn text_codepoints(text: &[u8]) -> impl Iterator<Item = u32> + '_ {
    codepoints(text).take_while(|&cp| cp != 0 && cp != REPLACEMENT)
}

/// The direction of the first strong character among the first `max_chars` characters of
/// `text`: `Some(true)` right-to-left, `Some(false)` left-to-right, `None` when there is none.
#[must_use]
pub fn first_strong_is_rtl(text: &[u8], max_chars: usize) -> Option<bool> {
    text_codepoints(text)
        .take(max_chars)
        .find_map(|cp| match class(cp) {
            Class::R | Class::AL => Some(true),
            Class::L => Some(false),
            _ => None,
        })
}

/// The paragraph direction [`visual`] lays a line out in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// From the line's first strong character, else left-to-right (rules P2, P3).
    Auto,
    Ltr,
    Rtl,
}

/// Whether [`visual`] took the text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visual {
    Done,
    Empty,
    TooLong,
}

/// Writes `text` in visual order through `emit`, one codepoint at a time and at most one per
/// character read: reordered for a paragraph of `level`, Arabic letters in their contextual
/// forms, marks after their base, and the Tetragrammaton as `'ה`. `emit` is not called unless
/// the result is [`Visual::Done`].
pub fn visual(
    text: &[u8],
    level: Level,
    workspace: &mut Workspace,
    mut emit: impl FnMut(u32),
) -> Visual {
    let Workspace { line, shaped } = workspace;
    let mut count = 0;
    let mut last_base: Option<usize> = None;
    let mut pending = 0u8;
    let mut chars = codepoints(text);
    while !chars.as_bytes().is_empty() {
        let Some(slot) = line.get_mut(count) else {
            return Visual::TooLong;
        };
        let Some(cp) = chars.next().filter(|&cp| cp != 0 && cp != REPLACEMENT) else {
            break;
        };
        *slot = Char {
            cp,
            index: u16::try_from(count).unwrap_or(u16::MAX),
            joiners: 0,
            class: class(cp),
        };
        // A joiner marks the letters on both sides, as mintty does: the one before it in its low
        // nibble, the one after it in its high nibble. Shaping reads the marks after reordering.
        if cp == 0x200C || cp == 0x200D {
            let joiner = if cp == 0x200D { ZWJ } else { ZWNJ };
            if let Some(base) = last_base.and_then(|i| line.get_mut(i)) {
                base.joiners |= joiner;
            }
            pending |= joiner;
        } else {
            slot.joiners = pending << 4;
            pending = 0;
            last_base = Some(count);
        }
        count += 1;
    }
    let (Some(line), Some(shaped)) = (line.get_mut(..count), shaped.get_mut(..count)) else {
        return Visual::TooLong;
    };
    if line.is_empty() {
        return Visual::Empty;
    }

    // Reorder first: Arabic forms depend on the visual neighbours.
    let (auto, start) = match level {
        Level::Auto => (true, 0),
        Level::Ltr => (false, 0),
        Level::Rtl => (false, 1),
    };
    reorder::reorder(line, auto, start);
    shape::shape(line, shaped);
    emit_visual(line, shaped, &mut emit);
    Visual::Done
}

/// The output stage of [`visual`].
fn emit_visual(line: &[Char], shaped: &[u32], emit: &mut impl FnMut(u32)) {
    // The absorbed Alef and the joiners did their work in shaping; they have no glyphs.
    let hidden = |cp: u32| cp == LIGATURE_PLACEHOLDER || cp == 0x200C || cp == 0x200D;
    let at = |i: usize| shaped.get(i).copied().unwrap_or(0);
    // is_transparent_mark() of the shaped character, from the class read with it.
    let mark = |i: usize| at(i) >= 0x0591 && line.get(i).is_some_and(|c| c.class == Class::NSM);
    let mut i = 0;
    while i < shaped.len() {
        let cp = at(i);
        i += 1;
        if hidden(cp) {
            continue;
        }
        // The Tetragrammaton, in visual order and skipping its points, is written `'ה`.
        if cp == 0x05D4 || mark(i - 1) {
            const NAME: [u32; 4] = [0x05D4, 0x05D5, 0x05D4, 0x05D9];
            let mut matched = 0;
            let mut end = i - 1;
            while end < shaped.len() && matched < NAME.len() {
                let (c, is_mark) = (at(end), mark(end));
                end += 1;
                if !is_mark {
                    if NAME.get(matched) == Some(&c) {
                        matched += 1;
                    } else {
                        break;
                    }
                }
            }
            if matched == NAME.len() {
                emit(u32::from('\''));
                emit(0x05D4);
                i = end;
                continue;
            }
        }
        if !mark(i - 1) {
            emit(cp);
            continue;
        }
        // Rule L3: a reversed run puts marks before their base. They are drawn over the glyph
        // before them, so emit the base first, then its marks in logical order. A base after the
        // marks with a lower logical index means the run was reversed.
        let start = i - 1;
        let mut j = start;
        while j < shaped.len() && (hidden(at(j)) || mark(j)) {
            j += 1;
        }
        let index = |k: usize| line.get(k).map(|c| c.index);
        if j < shaped.len() && index(j) < index(start) {
            emit(at(j));
            (start..j)
                .rev()
                .filter(|&k| mark(k))
                .for_each(|k| emit(at(k)));
            i = j + 1;
        } else {
            // Not reversed (or no base after them): the marks already follow their base.
            (start..j).filter(|&k| mark(k)).for_each(|k| emit(at(k)));
            i = j;
        }
    }
}

/// The left-to-right order in which to draw the `count` words of a line, which `word` hands out
/// in logical order: `order[k]` is the word drawn k-th.
///
/// `Some(true)` when `order` is to be used: the words moved, or the line has no right-to-left
/// text but sits in a right-to-left paragraph (the order is then the logical one, and tells the
/// caller to place the words from the left). `Some(false)` when they keep their logical order in
/// a left-to-right paragraph. `None` for fewer than two words, or more than [`MAX_LINE`] words or
/// characters.
pub fn word_order<'a>(
    count: usize,
    mut word: impl FnMut(usize) -> &'a [u8],
    rtl: bool,
    workspace: &mut Workspace,
    order: &mut [u16],
) -> Option<bool> {
    if count <= 1 || count > MAX_LINE {
        return None;
    }
    let order = order.get_mut(..count)?;
    let space_index = u16::try_from(count).ok()?;
    let line = &mut workspace.line;
    let mut len = 0;
    for w in 0..count {
        let index = u16::try_from(w).unwrap_or(u16::MAX);
        let mut chars = codepoints(word(w));
        while !chars.as_bytes().is_empty() {
            let slot = line.get_mut(len)?;
            let Some(cp) = chars.next().filter(|&cp| cp != 0 && cp != REPLACEMENT) else {
                break;
            };
            *slot = Char {
                cp,
                index,
                joiners: 0,
                class: class(cp),
            };
            len += 1;
        }
        if w + 1 < count {
            // The space between words belongs to none of them.
            let slot = line.get_mut(len)?;
            *slot = Char {
                cp: u32::from(' '),
                index: space_index,
                joiners: 0,
                class: Class::WS,
            };
            len += 1;
        }
    }
    let line = line.get_mut(..len).filter(|line| !line.is_empty())?;

    // Lines of one direction need no algorithm.
    let has_rtl = line.iter().any(|c| matches!(c.class, Class::R | Class::AL));
    let has_ltr = line
        .iter()
        .any(|c| matches!(c.class, Class::L | Class::EN | Class::AN));
    if has_rtl && !has_ltr && rtl {
        for (slot, w) in order.iter_mut().zip((0..space_index).rev()) {
            *slot = w;
        }
        return Some(true);
    }
    if !has_rtl {
        // A left-to-right line in a right-to-left paragraph keeps its order, but is drawn from
        // the left: the caller needs an order to know.
        for (slot, w) in order.iter_mut().zip(0..space_index) {
            *slot = w;
        }
        return Some(rtl);
    }

    reorder::reorder(line, false, u8::from(rtl));
    place_words(line, count, order)
}

/// Writes the order of the `count` words of `line` (visual order) to `order`: a word sits where
/// its first character of a natural direction (else its first character) ended up. As
/// [`word_order`]; `None` when a word has no character (the C version gave up on it too).
fn place_words(line: &[Char], count: usize, order: &mut [u16]) -> Option<bool> {
    let natural = |class| {
        matches!(
            class,
            Class::L | Class::R | Class::AL | Class::EN | Class::AN
        )
    };
    let mut first_any = [u16::MAX; MAX_LINE];
    let mut first_natural = [u16::MAX; MAX_LINE];
    for (pos, c) in (0u16..).zip(line.iter()) {
        let w = usize::from(c.index);
        if w >= count {
            continue;
        }
        if let Some(first) = first_any.get_mut(w).filter(|f| **f == u16::MAX) {
            *first = pos;
        }
        if natural(c.class)
            && let Some(first) = first_natural.get_mut(w).filter(|f| **f == u16::MAX)
        {
            *first = pos;
        }
    }
    let mut placed = 0;
    for (pos, c) in (0u16..).zip(line.iter()) {
        let w = usize::from(c.index);
        if w >= count {
            continue;
        }
        let anchor = match first_natural.get(w) {
            Some(&n) if n != u16::MAX => n,
            _ => first_any.get(w).copied().unwrap_or(u16::MAX),
        };
        if anchor == u16::MAX {
            return None;
        }
        if anchor == pos {
            *order.get_mut(placed)? = c.index;
            placed += 1;
        }
    }
    (placed == count).then(|| order.iter().zip(0u16..).any(|(&w, k)| w != k))
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests"
)]
mod tests;
