//! Paragraph layout as lib/Epub's `ParsedText` did it, which `test/parsed_text` pins: words split
//! into tokens (CJK characters, explicit hyphens, Focus Reading's bold prefixes), then lines
//! broken by minimum raggedness, or greedily with hyphenation, and laid out with justification,
//! indents, ruby overhangs and bidirectional reordering.
//!
//! Glyph metrics come from the caller ([`Metrics`]): a line comes out the same as before only if
//! they do. A paragraph keeps its text in 2 KB chunks of NUL-terminated words (`store.rs`), and
//! measures and hands out NUL-terminated text, because the renderer and `TextBlock` take C strings.
//! Nothing here panics, and every allocation is fallible: a token or line that cannot be stored is
//! dropped and [`Paragraph::dropped_words`] reports it, as the C++ did for its word arena.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;

mod layout;
mod offsets;
mod store;
mod text;
#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::vec::Vec;

use offsets::{Absorbed, Full, VisibleOffsets};
use store::{Word, WordStore};

pub use store::Reclaim;
use text::{
    cjk_character_break_offsets, contains_cjk_breakable, count_codepoints,
    ends_with_breakable_hyphen, explicit_hyphen_break_offsets, first_codepoint,
    has_cjk_break_opportunity_between, is_word_character, last_codepoint, may_contain_rtl_bytes,
};

pub use layout::{Line, LinkSpan, Params};

/// `EpdFontFamily::Style` bits.
pub mod style {
    pub const BOLD: u8 = 1;
    pub const UNDERLINE: u8 = 4;
    pub const SUP: u8 = 16;
    pub const RUBY_CONTINUE: u8 = 64;
    pub const SMALL_CAPS: u8 = 128;
    /// Stored styles reuse the small-caps bit for "a hyphen was inserted at this split".
    pub const DISCRETIONARY_HYPHEN: u8 = 0x80;
    /// Size slots carry small caps in their high bit instead.
    pub const SMALL_CAPS_SLOT: u8 = 0x80;
}

/// Inline sizes a paragraph can use besides its own: `TextBlock::MAX_WORD_FONTS`.
pub const MAX_WORD_FONTS: usize = 3;
/// Slot 0 is the paragraph's size.
pub const MAX_SIZE_SLOTS: usize = MAX_WORD_FONTS + 1;
/// Lines longer than this keep the block font for every word.
const MAX_FONT_SLOT_WORDS: usize = 128;

/// `CssTextAlign`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Justify,
    Left,
    Center,
    Right,
    None,
}

impl Align {
    #[must_use]
    pub fn from_index(index: u8) -> Self {
        match index {
            0 => Align::Justify,
            1 => Align::Left,
            2 => Align::Center,
            3 => Align::Right,
            _ => Align::None,
        }
    }
}

/// The parts of the paragraph's `BlockStyle` layout reads; `is_rtl` may be set by it.
#[derive(Clone, Copy)]
#[allow(clippy::struct_excessive_bools)]
pub struct BlockStyle {
    pub align: Align,
    pub text_align_defined: bool,
    pub direction_defined: bool,
    pub is_rtl: bool,
    pub text_indent_defined: bool,
    pub text_indent: i16,
}

/// Glyph metrics, as `GfxRenderer` gives them. `text` ends with a NUL byte.
pub trait Metrics {
    fn text_advance(&mut self, font: i32, text: &[u8], style: u8, spacing: i8) -> i32;
    fn kerning(&mut self, font: i32, left: u32, right: u32, style: u8, spacing: i8) -> i32;
    fn space_advance(&mut self, font: i32, left: u32, right: u32, style: u8) -> i32;
    fn space_width(&mut self, font: i32, style: u8) -> i32;
    fn ascender(&mut self, font: i32) -> i32;
    fn is_sd_font(&mut self, font: i32) -> bool;
    /// Loads the advances of every codepoint of `segments` (each NUL-separated words) for an
    /// SD-card font.
    fn prewarm(
        &mut self,
        font: i32,
        segments: &[&[u8]],
        include_space: bool,
        include_hyphen: bool,
        style_mask: u8,
        small_caps: bool,
    );
}

/// One paragraph's tokens and what layout needs to know about them.
#[allow(clippy::struct_excessive_bools)]
pub struct Paragraph {
    hyphenation: bool,
    focus_reading: bool,
    indent_spaces: u8,
    store: WordStore,
    words: Vec<Word>,
    styles: Vec<u8>,
    continues: Vec<bool>,
    no_space_before: Vec<bool>,
    /// Focus Reading: bytes `[0, n)` of the token render bold; 0 for none.
    focus: Vec<u8>,
    links: Vec<u8>,
    /// Size slot, with `style::SMALL_CAPS_SLOT`.
    slots: Vec<u8>,
    offsets: VisibleOffsets,
    absorbed: Absorbed,
    /// Per token while any token has ruby; empty otherwise.
    ruby: Vec<Vec<u8>>,
    word_spacing: u8,
    character_spacing: i8,
    natural_align: bool,
    has_rtl_word: bool,
    dropped: bool,
    first_line_consumed: bool,
    start_emitted: bool,
    bidi: Option<Box<bembeyaz_bidi::Workspace>>,
    scratch: layout::Scratch,
}

/// How a token joins the one before it.
#[derive(Clone, Copy)]
struct Join {
    continues: bool,
    no_space_before: bool,
}

impl Paragraph {
    /// `reclaim` evicts caches when a chunk of word text cannot be allocated.
    #[must_use]
    pub fn new(
        hyphenation: bool,
        focus_reading: bool,
        indent_spaces: u8,
        reclaim: Option<Reclaim>,
    ) -> Self {
        Paragraph {
            hyphenation,
            focus_reading,
            indent_spaces,
            store: WordStore::new(reclaim),
            words: Vec::new(),
            styles: Vec::new(),
            continues: Vec::new(),
            no_space_before: Vec::new(),
            focus: Vec::new(),
            links: Vec::new(),
            slots: Vec::new(),
            offsets: VisibleOffsets::default(),
            absorbed: Absorbed::default(),
            ruby: Vec::new(),
            word_spacing: 100,
            character_spacing: 0,
            natural_align: false,
            has_rtl_word: false,
            dropped: false,
            first_line_consumed: false,
            start_emitted: false,
            bidi: None,
            scratch: layout::Scratch::default(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// A token or a line was dropped for lack of memory: the paragraph is incomplete.
    #[must_use]
    pub fn dropped_words(&self) -> bool {
        self.dropped
    }

    pub fn suppress_first_line_indent(&mut self) {
        self.first_line_consumed = true;
    }

    pub fn reset_first_line_indent(&mut self) {
        self.first_line_consumed = false;
        self.start_emitted = false;
    }

    fn word(&self, index: usize) -> &[u8] {
        self.words.get(index).map_or(&[], |&w| self.store.view(w))
    }

    /// The token's renderer style: the stored flag removed, small caps restored from its slot.
    #[must_use]
    pub fn word_style(&self, index: usize) -> u8 {
        let Some(&stored) = self.styles.get(index) else {
            return 0;
        };
        let small_caps = self
            .slots
            .get(index)
            .is_some_and(|&s| s & style::SMALL_CAPS_SLOT != 0);
        (stored & !style::DISCRETIONARY_HYPHEN) | if small_caps { style::SMALL_CAPS } else { 0 }
    }

    #[must_use]
    pub fn ruby_at(&self, index: usize) -> &[u8] {
        self.ruby.get(index).map_or(&[], Vec::as_slice)
    }

    /// Stores `token` with its NUL; `None` (and the paragraph marked incomplete) when out of
    /// memory.
    fn store(&mut self, token: &[u8]) -> Option<Word> {
        let word = self.store.append(token);
        self.dropped |= word.is_none();
        word
    }

    fn reserve_tokens(&mut self, additional: usize) -> Result<(), Full> {
        let fail = |_| Full;
        self.words.try_reserve(additional).map_err(fail)?;
        self.styles.try_reserve(additional).map_err(fail)?;
        self.continues.try_reserve(additional).map_err(fail)?;
        self.no_space_before.try_reserve(additional).map_err(fail)?;
        self.focus.try_reserve(additional).map_err(fail)?;
        self.links.try_reserve(additional).map_err(fail)?;
        self.slots.try_reserve(additional).map_err(fail)?;
        self.offsets.reserve(additional)
    }

    /// Appends one token; a failed allocation drops it and leaves the arrays in step.
    #[allow(clippy::too_many_arguments)]
    fn push_token(
        &mut self,
        token: &[u8],
        token_style: u8,
        join: Join,
        focus: u8,
        offset: u32,
        link: u8,
        slot: u8,
        pad_ruby: bool,
    ) {
        let pad = pad_ruby && !self.ruby.is_empty();
        if self.reserve_tokens(1).is_err()
            || (pad && self.ruby.try_reserve(1).is_err())
            || self.offsets.push(offset).is_err()
        {
            self.dropped = true;
            return;
        }
        // The offset goes first: a word stored and then not kept would pin its chunk.
        let Some(word) = self.store(token) else {
            self.offsets.pop();
            return;
        };
        self.words.push(word);
        self.styles.push(token_style & !style::SMALL_CAPS);
        self.continues.push(join.continues);
        self.no_space_before.push(join.no_space_before);
        self.focus.push(focus);
        self.links.push(link);
        self.slots.push(
            slot | if token_style & style::SMALL_CAPS != 0 {
                style::SMALL_CAPS_SLOT
            } else {
                0
            },
        );
        if pad_ruby && !self.ruby.is_empty() {
            self.ruby.push(Vec::new());
        }
    }

    fn last_ends_with_breakable_hyphen(&self) -> bool {
        self.words
            .len()
            .checked_sub(1)
            .is_some_and(|last| ends_with_breakable_hyphen(self.word(last)))
    }

    /// Adds a word as `ParsedText::addWord` did: NFC-composed, then split into tokens.
    #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
    pub fn add_word(
        &mut self,
        word: &[u8],
        font_style: u8,
        underline: bool,
        attach_to_previous: bool,
        offset: u32,
        link: u8,
        slot: u8,
        no_space_before: bool,
    ) {
        if word.is_empty() {
            return;
        }
        // NFC never lengthens text.
        let mut composed = Vec::new();
        let mut failed = composed.try_reserve(word.len()).is_err();
        let result = bembeyaz_utf8::compose_nfc(
            word,
            |index| self.absorbed.record(offset.wrapping_add(index)).is_ok(),
            |piece| {
                failed |= composed.try_reserve(piece.len()).is_err();
                if !failed {
                    composed.extend_from_slice(piece);
                }
            },
        );
        let word = match result {
            bembeyaz_utf8::Compose::Unchanged => word,
            bembeyaz_utf8::Compose::Composed if !failed => composed.as_slice(),
            _ => {
                self.dropped = true;
                return;
            }
        };
        if word.is_empty() {
            return;
        }
        self.add_composed(
            word,
            font_style,
            underline,
            attach_to_previous,
            offset,
            link,
            slot,
            no_space_before,
        );
    }

    #[allow(
        clippy::too_many_arguments,
        clippy::fn_params_excessive_bools,
        clippy::too_many_lines
    )]
    fn add_composed(
        &mut self,
        word: &[u8],
        font_style: u8,
        underline: bool,
        attach_to_previous: bool,
        offset: u32,
        link: u8,
        slot: u8,
        no_space_before: bool,
    ) {
        let base_style = font_style | if underline { style::UNDERLINE } else { 0 };
        let starts_rtl = !self.has_rtl_word
            && may_contain_rtl_bytes(word)
            && bembeyaz_bidi::first_strong_is_rtl(word, 64) == Some(true);

        // Only a glued token may become a gap-less break opportunity: whitespace between two
        // words is content (Korean is space-delimited and Hangul is "CJK breakable").
        let mut join = Join {
            continues: attach_to_previous,
            no_space_before,
        };
        if attach_to_previous && self.last_ends_with_breakable_hyphen() {
            join = Join {
                continues: true,
                no_space_before: true,
            };
        } else if attach_to_previous
            && self.words.len().checked_sub(1).is_some_and(|last| {
                has_cjk_break_opportunity_between(
                    last_codepoint(self.word(last)),
                    first_codepoint(word),
                )
            })
        {
            join = Join {
                continues: false,
                no_space_before: true,
            };
        }

        let mut breaks = Vec::new();
        if !cjk_character_break_offsets(word, &mut breaks) {
            self.dropped = true;
            return;
        }
        if !breaks.is_empty() {
            self.ensure_token_capacity(breaks.len() + 1);
            let mut first = true;
            let mut start = 0;
            let mut token_offset = offset;
            for &at in &breaks {
                if at <= start || at > word.len() {
                    continue;
                }
                let token = word.get(start..at).unwrap_or_default();
                let token_join = if first {
                    join
                } else {
                    Join {
                        continues: self.last_ends_with_breakable_hyphen(),
                        no_space_before: true,
                    }
                };
                self.push_token(
                    token,
                    base_style,
                    token_join,
                    0,
                    token_offset,
                    link,
                    slot,
                    true,
                );
                token_offset = self
                    .absorbed
                    .offset_after(token_offset, count_codepoints(token));
                first = false;
                start = at;
            }
            if start < word.len() {
                let token_join = if first {
                    join
                } else {
                    Join {
                        continues: self.last_ends_with_breakable_hyphen(),
                        no_space_before: true,
                    }
                };
                let token = word.get(start..).unwrap_or_default();
                self.push_token(
                    token,
                    base_style,
                    token_join,
                    0,
                    token_offset,
                    link,
                    slot,
                    true,
                );
            }
            self.has_rtl_word |= starts_rtl;
            return;
        }

        if contains_cjk_breakable(word) {
            self.push_token(word, base_style, join, 0, offset, link, slot, true);
            self.has_rtl_word |= starts_rtl;
            return;
        }

        // Already-bold text stays bold: a focus split would make its suffix regular.
        if !self.focus_reading || base_style & style::BOLD != 0 {
            if !explicit_hyphen_break_offsets(word, join.continues, &mut breaks) {
                self.dropped = true;
                return;
            }
            if breaks.is_empty() {
                self.push_token(word, base_style, join, 0, offset, link, slot, true);
            } else {
                self.ensure_token_capacity(breaks.len() + 1);
                let mut first = true;
                let mut start = 0;
                let mut token_offset = offset;
                let glued = Join {
                    continues: true,
                    no_space_before: true,
                };
                for &at in &breaks {
                    if at <= start || at > word.len() {
                        continue;
                    }
                    let token = word.get(start..at).unwrap_or_default();
                    let token_join = if first { join } else { glued };
                    self.push_token(
                        token,
                        base_style,
                        token_join,
                        0,
                        token_offset,
                        link,
                        slot,
                        true,
                    );
                    token_offset = self
                        .absorbed
                        .offset_after(token_offset, count_codepoints(token));
                    first = false;
                    start = at;
                }
                if start < word.len() {
                    let token_join = if first { join } else { glued };
                    let token = word.get(start..).unwrap_or_default();
                    self.push_token(
                        token,
                        base_style,
                        token_join,
                        0,
                        token_offset,
                        link,
                        slot,
                        true,
                    );
                }
            }
            self.has_rtl_word |= starts_rtl;
            return;
        }

        // Focus Reading: alternating word and non-word segments.
        self.ensure_token_capacity(word.len());
        let mut cps = text::codepoint_ends(word);
        let first = cps.next();
        let mut segment_start = 0;
        let mut in_word = first.is_some_and(|(cp, _)| is_word_character(cp));
        let mut first_segment = true;
        let mut prev_end = first.map_or(0, |(_, end)| end);
        for (cp, end) in cps {
            let letter = is_word_character(cp);
            if letter != in_word {
                let segment = word.get(segment_start..prev_end).unwrap_or_default();
                let segment_join = self.segment_join(first_segment, join);
                self.push_focus_segment(
                    word,
                    segment_start,
                    segment,
                    in_word,
                    segment_join,
                    base_style,
                    offset,
                    link,
                    slot,
                );
                segment_start = prev_end;
                in_word = letter;
                first_segment = false;
            }
            prev_end = end;
        }
        let segment = word.get(segment_start..).unwrap_or_default();
        let segment_join = self.segment_join(first_segment, join);
        self.push_focus_segment(
            word,
            segment_start,
            segment,
            in_word,
            segment_join,
            base_style,
            offset,
            link,
            slot,
        );
        self.has_rtl_word |= starts_rtl;
    }

    /// Only the first segment takes the word's own join; later ones glue on, breakable after a
    /// visible hyphen.
    fn segment_join(&self, first: bool, join: Join) -> Join {
        if first {
            join
        } else {
            Join {
                continues: true,
                no_space_before: self.last_ends_with_breakable_hyphen(),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_focus_segment(
        &mut self,
        word: &[u8],
        segment_start: usize,
        segment: &[u8],
        is_word: bool,
        join: Join,
        base_style: u8,
        offset: u32,
        link: u8,
        slot: u8,
    ) {
        let before = count_codepoints(word.get(..segment_start).unwrap_or_default());
        let segment_offset = self.absorbed.offset_after(offset, before);
        if !is_word {
            self.push_token(
                segment,
                base_style,
                join,
                0,
                segment_offset,
                link,
                slot,
                false,
            );
            return;
        }
        // 45% of the letters, 1 to 9: one bold at 4 letters, three at 7.
        let chars = count_codepoints(segment) as usize;
        let bold = (chars * 45 / 100).clamp(1, 9);
        if bold >= chars {
            self.push_token(
                segment,
                base_style | style::BOLD,
                join,
                0,
                segment_offset,
                link,
                slot,
                false,
            );
        } else {
            let split = text::codepoint_ends(segment)
                .nth(bold - 1)
                .map_or(0, |(_, end)| end);
            let boundary = u8::try_from(split).unwrap_or(u8::MAX);
            self.push_token(
                segment,
                base_style,
                join,
                boundary,
                segment_offset,
                link,
                slot,
                false,
            );
        }
    }

    /// Reserves the token arrays for a burst, doubling from 16 as `ParsedText` did.
    fn ensure_token_capacity(&mut self, additional: usize) {
        if additional == 0 {
            return;
        }
        let required = self.words.len() + additional;
        let capacity = self.styles.capacity();
        if capacity >= required {
            return;
        }
        let mut new_capacity = capacity.max(16);
        while new_capacity < required {
            new_capacity *= 2;
        }
        // Best effort: push_token reserves for itself.
        let _ = self.reserve_tokens(new_capacity - self.words.len());
    }

    /// Sets the ruby of one token.
    pub fn set_ruby(&mut self, index: usize, ruby: &[u8]) {
        if index >= self.words.len() || self.cover_ruby(index).is_err() {
            return;
        }
        if let Some(slot) = self.ruby.get_mut(index)
            && slot.try_reserve(ruby.len()).is_ok()
        {
            slot.clear();
            slot.extend_from_slice(ruby);
        }
    }

    /// Sets the ruby of a group of tokens on its first; the rest are bound to it.
    pub fn set_ruby_group(&mut self, start: usize, count: usize, ruby: &[u8]) {
        if start >= self.words.len() {
            return;
        }
        self.set_ruby(start, ruby);
        for index in start + 1..start + count {
            if index >= self.words.len() {
                break;
            }
            if self.cover_ruby(index).is_ok()
                && let Some(slot) = self.ruby.get_mut(index)
            {
                slot.clear();
            }
            if let Some(s) = self.styles.get_mut(index) {
                *s |= style::RUBY_CONTINUE;
            }
            if let Some(c) = self.continues.get_mut(index) {
                *c = true;
            }
            if let Some(n) = self.no_space_before.get_mut(index) {
                *n = false;
            }
        }
    }

    /// Gives every token a ruby entry once `index` has none.
    fn cover_ruby(&mut self, index: usize) -> Result<(), Full> {
        if self.ruby.len() <= index {
            self.ruby
                .try_reserve(self.words.len().saturating_sub(self.ruby.len()))
                .map_err(|_| Full)?;
            self.ruby.resize_with(self.words.len(), Vec::new);
        }
        Ok(())
    }
}
