//! Measuring tokens, breaking lines and placing words, as `ParsedText::layoutAndExtractLines`.

use alloc::boxed::Box;
use alloc::vec::Vec;

use bembeyaz_hyphenation::Language;

use crate::text::{
    SOFT_HYPHEN, allows_break, contains, count_codepoints, first_codepoint, hangul_line_end_breaks,
    is_cjk_ideograph, is_justifiable_gap, last_codepoint, push_without_soft_hyphens,
};
use crate::{
    Align, BlockStyle, MAX_FONT_SLOT_WORDS, MAX_SIZE_SLOTS, MAX_WORD_FONTS, Metrics, Paragraph,
    Word, style,
};

/// The longest prefix (with its hyphen) a split may store, the size of the C++'s stack buffer.
const MAX_PREFIX: usize = 208;
/// Paragraph direction: the first tokens and characters looked at.
const RTL_PARAGRAPH_PROBE_WORDS: usize = 3;
const RTL_PARAGRAPH_PROBE_DEPTH: usize = 5;
/// Focus Reading's bold prefix is measured from at most this many bytes.
const FOCUS_PREFIX_MAX: usize = 39;

/// Buffers reused from line to line.
#[derive(Default)]
pub(crate) struct Scratch {
    measure: Vec<u8>,
    line: LineBuffers,
    bidi: BidiBuffers,
}

#[derive(Default)]
struct LineBuffers {
    text: Vec<u8>,
    word_starts: Vec<u32>,
    xpos: Vec<i16>,
    styles: Vec<u8>,
    focus: Vec<u8>,
    suffix_x: Vec<u16>,
    ruby_text: Vec<u8>,
    ruby_starts: Vec<u32>,
    links: Vec<LinkSpan>,
    ranges: Vec<[u32; 2]>,
    word_slots: Vec<u8>,
    /// What reordering writes into, swapped with the three above.
    spare_text: Vec<u8>,
    spare_starts: Vec<u32>,
    spare_styles: Vec<u8>,
}

/// Bidi reordering: the visual order, and the joins in it.
#[derive(Default)]
struct BidiBuffers {
    order: Vec<u16>,
    continues: Vec<bool>,
    no_space: Vec<bool>,
}

/// A link's box on a line: `link` is the paragraph's link id (1-based).
#[derive(Clone, Copy)]
#[repr(C)]
pub struct LinkSpan {
    pub link: u8,
    pub x: i16,
    pub width: i16,
    pub top_lift: i16,
}

/// A laid-out line, in visual order. Words and ruby are NUL-terminated in `text` and
/// `ruby_text`, starting at `word_starts` and `ruby_starts`; ruby is in logical order, one per
/// word. `focus` and `suffix_x` are empty when no word has a bold prefix, `word_slots` when every
/// word uses the block font.
pub struct Line<'a> {
    pub text: &'a [u8],
    pub word_starts: &'a [u32],
    pub xpos: &'a [i16],
    pub styles: &'a [u8],
    pub focus: &'a [u8],
    pub suffix_x: &'a [u16],
    pub ruby_text: &'a [u8],
    pub ruby_starts: &'a [u32],
    pub links: &'a [LinkSpan],
    pub ranges: &'a [[u32; 2]],
    pub paragraph_start: u16,
    pub word_slots: &'a [u8],
    pub word_fonts: [i32; MAX_WORD_FONTS],
    pub visible_offset: u32,
    /// Lines of the paragraph after this one, or `None` when the rest is not laid out yet.
    pub lines_after: Option<u32>,
    /// The paragraph's direction, as layout resolved it.
    pub is_rtl: bool,
}

/// What a layout pass is given besides the paragraph.
pub struct Params {
    pub font: i32,
    /// The font of each size slot (0: the paragraph's own), and how many are in use.
    pub slot_fonts: [i32; MAX_SIZE_SLOTS],
    pub slot_count: u8,
    pub width: u16,
    pub include_last_line: bool,
    pub character_spacing: i8,
    pub word_spacing: u8,
    pub max_lines: usize,
    pub language: Option<Language>,
    /// The paragraph's link targets: ids above this are dropped.
    pub link_count: usize,
}

fn scale_space(advance: i32, percent: u8) -> i32 {
    if percent == 100 {
        advance
    } else {
        (advance * i32::from(percent) + 50) / 100
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn to_u16(value: i32) -> u16 {
    value as u16
}

#[allow(clippy::cast_possible_truncation)]
fn to_i16(value: i32) -> i16 {
    value as i16
}

fn add_u16(width: &mut u16, value: i32) {
    *width = to_u16(i32::from(*width) + value);
}

fn justify_extra(spare: i32, gaps: usize) -> i32 {
    match i32::try_from(gaps) {
        Ok(gaps) if gaps >= 1 && spare > 0 => spare / gaps,
        _ => 0,
    }
}

/// A ruby group and its overhangs.
struct Group {
    start: usize,
    count: usize,
    left: i32,
    right: i32,
}

/// Measures through the renderer, which takes NUL-terminated text: `scratch` gets a copy.
struct Measure<'a> {
    metrics: &'a mut dyn Metrics,
    scratch: &'a mut Vec<u8>,
    spacing: i8,
    word_spacing: u8,
    /// A measurement had no memory for its copy (and read 0): the paragraph is incomplete.
    failed: bool,
}

impl Measure<'_> {
    /// The advance of `word` without its soft hyphens, with a hyphen appended if asked.
    fn word(&mut self, font: i32, word: &[u8], style: u8, append_hyphen: bool) -> u16 {
        if word == b" " && !append_hyphen {
            return to_u16(scale_space(
                self.metrics.space_width(font, style),
                self.word_spacing,
            ));
        }
        self.scratch.clear();
        if self.scratch.try_reserve(word.len() + 2).is_err() {
            self.failed = true;
            return 0;
        }
        push_without_soft_hyphens(self.scratch, word);
        if append_hyphen {
            self.scratch.push(b'-');
        }
        self.scratch.push(0);
        to_u16(
            self.metrics
                .text_advance(font, self.scratch, style, self.spacing),
        )
    }

    /// The bold prefix's advance plus the kerning into the regular suffix.
    fn focus_prefix(&mut self, font: i32, word: &[u8], style: u8, boundary: u8) -> u16 {
        let len = usize::from(boundary).min(FOCUS_PREFIX_MAX);
        self.scratch.clear();
        if self.scratch.try_reserve(len + 1).is_err() {
            self.failed = true;
            return 0;
        }
        self.scratch
            .extend_from_slice(word.get(..len).unwrap_or(word));
        self.scratch.push(0);
        let bold = style | style::BOLD;
        let prefix_last = last_codepoint(
            self.scratch
                .get(..self.scratch.len() - 1)
                .unwrap_or_default(),
        );
        let suffix_first =
            bembeyaz_utf8::decode(word.get(usize::from(boundary)..).unwrap_or_default()).0;
        let kerning = self
            .metrics
            .kerning(font, prefix_last, suffix_first, bold, self.spacing);
        let advance = self
            .metrics
            .text_advance(font, self.scratch, bold, self.spacing);
        to_u16(advance + kerning)
    }

    fn focus_word(
        &mut self,
        font: i32,
        word: &[u8],
        style: u8,
        boundary: u8,
        append_hyphen: bool,
    ) -> u16 {
        if boundary == 0 {
            return self.word(font, word, style, append_hyphen);
        }
        if usize::from(boundary) >= word.len() {
            return self.word(font, word, style | style::BOLD, append_hyphen);
        }
        let suffix = self.word(
            font,
            word.get(usize::from(boundary)..).unwrap_or_default(),
            style,
            append_hyphen,
        );
        to_u16(i32::from(self.focus_prefix(font, word, style, boundary)) + i32::from(suffix))
    }

    /// The gap before a token kept on the line after `left`: kerning when attached, letter
    /// spacing at a gap-less break, else a (scaled) space.
    fn gap(
        &mut self,
        font: i32,
        left: &[u8],
        right: &[u8],
        left_style: u8,
        continues: bool,
        no_space: bool,
    ) -> i32 {
        if continues {
            self.metrics.kerning(
                font,
                last_codepoint(left),
                first_codepoint(right),
                left_style,
                self.spacing,
            )
        } else if no_space {
            i32::from(self.spacing)
        } else {
            self.space(font, left, right, left_style)
        }
    }

    fn space(&mut self, font: i32, left: &[u8], right: &[u8], left_style: u8) -> i32 {
        scale_space(
            self.metrics.space_advance(
                font,
                last_codepoint(left),
                first_codepoint(right),
                left_style,
            ),
            self.word_spacing,
        )
    }

    fn kerning(&mut self, font: i32, left: &[u8], right: &[u8], left_style: u8) -> i32 {
        self.metrics.kerning(
            font,
            last_codepoint(left),
            first_codepoint(right),
            left_style,
            self.spacing,
        )
    }
}

/// Where `word` may split at a line end: Hangul breaks first, so they win a tie with a hyphenated
/// break of the same width. Its own frame, so the hyphenator's buffers are gone while the
/// candidates are measured. `None` when out of memory.
#[inline(never)]
fn break_candidates(
    word: &[u8],
    language: Option<Language>,
    fallback: bool,
) -> Option<Vec<(usize, bool)>> {
    let mut candidates = Vec::new();
    if !hangul_line_end_breaks(word, &mut candidates) {
        return None;
    }
    let mut full = false;
    bembeyaz_hyphenation::breaks(word, language, fallback, |b| {
        if candidates.try_reserve(1).is_ok() {
            candidates.push((b.offset, b.inserted_hyphen));
        } else {
            full = true;
        }
    });
    (!full).then_some(candidates)
}

/// A fresh bidi workspace, or `None` when out of memory. It is zeroed in place: all-zero bytes are
/// a valid workspace, and building one on the stack would take 2.6 KB.
fn new_workspace() -> Option<Box<bembeyaz_bidi::Workspace>> {
    let layout = core::alloc::Layout::new::<bembeyaz_bidi::Workspace>();
    // SAFETY: the workspace is not zero-sized, and zeroed bytes are a valid one.
    unsafe {
        #[allow(clippy::cast_ptr_alignment)] // the layout is the workspace's own
        let block = alloc::alloc::alloc_zeroed(layout).cast::<bembeyaz_bidi::Workspace>();
        if block.is_null() {
            return None;
        }
        Some(Box::from_raw(block))
    }
}

impl Paragraph {
    fn word_font(&self, index: usize, block_font: i32, params: &Params) -> i32 {
        let slot = self
            .slots
            .get(index)
            .map_or(0, |&s| s & !style::SMALL_CAPS_SLOT);
        match params.slot_fonts.get(usize::from(slot)) {
            Some(&font) if slot != 0 && font != 0 => font,
            _ => block_font,
        }
    }

    fn style_at(&self, index: usize) -> u8 {
        self.styles.get(index).copied().unwrap_or(0)
    }

    fn flag(values: &[bool], index: usize) -> bool {
        values.get(index).copied().unwrap_or(false)
    }

    fn first_line_indent(
        &self,
        first_line: bool,
        m: &mut Measure<'_>,
        font: i32,
        block: BlockStyle,
    ) -> i32 {
        if self.first_line_consumed || !first_line || !self.natural_align {
            return 0;
        }
        if block.text_indent_defined && block.text_indent < 0 {
            return i32::from(block.text_indent);
        }
        if self.indent_spaces == 0 {
            return 0;
        }
        scale_space(m.metrics.space_width(font, 0), m.word_spacing) * i32::from(self.indent_spaces)
    }

    /// The ruby of the group led by `index`, wider than its base: half the overhang, reserved at
    /// the line start.
    fn ruby_extra_start(&self, m: &mut Measure<'_>, index: usize, end: usize, font: i32) -> i32 {
        let Some(ruby) = self.ruby.get(index).filter(|r| !r.is_empty()) else {
            return 0;
        };
        if self.style_at(index) & style::RUBY_CONTINUE != 0 {
            return 0;
        }
        let mut count = 1;
        while index + count < end && self.style_at(index + count) & style::RUBY_CONTINUE != 0 {
            count += 1;
        }
        let mut base = 0;
        for k in index..index + count {
            base += i32::from(m.word(font, self.word(k), self.word_style(k), false));
        }
        let width = self.ruby_width(m, ruby, font);
        if width <= base { 0 } else { (width - base) / 2 }
    }

    /// The same for the last ruby group of the line `[start, end)`, reserved at its end.
    fn ruby_extra_end(&self, m: &mut Measure<'_>, start: usize, end: usize, font: i32) -> i32 {
        if self.ruby.is_empty() || end == 0 || start >= end {
            return 0;
        }
        let mut leader = end - 1;
        while leader > start && self.style_at(leader) & style::RUBY_CONTINUE != 0 {
            leader -= 1;
        }
        let Some(ruby) = self.ruby.get(leader).filter(|r| !r.is_empty()) else {
            return 0;
        };
        if self.style_at(leader) & style::RUBY_CONTINUE != 0 {
            return 0;
        }
        let mut base = 0;
        for k in leader..end {
            base += i32::from(m.word(font, self.word(k), self.word_style(k), false));
        }
        let width = self.ruby_width(m, ruby, font);
        if width <= base { 0 } else { (width - base) / 2 }
    }

    fn ruby_width(&self, m: &mut Measure<'_>, ruby: &[u8], font: i32) -> i32 {
        let _ = self;
        m.scratch.clear();
        if m.scratch.try_reserve(ruby.len() + 1).is_err() {
            m.failed = true;
            return 0;
        }
        m.scratch.extend_from_slice(ruby);
        m.scratch.push(0);
        m.metrics
            .text_advance(font, m.scratch, style::SUP, m.spacing)
    }

    #[allow(clippy::too_many_lines)]
    #[inline(never)]
    fn word_widths(&self, m: &mut Measure<'_>, font: i32, params: &Params, widths: &mut Vec<u16>) {
        for i in 0..self.words.len() {
            let width = m.focus_word(
                self.word_font(i, font, params),
                self.word(i),
                self.word_style(i),
                self.focus.get(i).copied().unwrap_or(0),
                false,
            );
            widths.push(width);
        }
        if self.ruby.is_empty() {
            return;
        }

        let mut groups = Vec::new();
        let mut i = 0;
        while i < self.words.len() {
            if self.ruby.get(i).is_some_and(|r| !r.is_empty())
                && self.style_at(i) & style::RUBY_CONTINUE == 0
            {
                let mut base = i32::from(widths.get(i).copied().unwrap_or(0));
                let mut count = 1;
                while i + count < self.words.len()
                    && self.style_at(i + count) & style::RUBY_CONTINUE != 0
                {
                    base += i32::from(widths.get(i + count).copied().unwrap_or(0));
                    count += 1;
                }
                let ruby = self.ruby.get(i).map_or(&[][..], Vec::as_slice);
                let width = self.ruby_width(m, ruby, font);
                let overlap = 0.max((width - base) / 2);
                if groups.try_reserve(1).is_err() {
                    m.failed = true;
                    return;
                }
                groups.push(Group {
                    start: i,
                    count,
                    left: overlap,
                    right: overlap,
                });
                i += count - 1;
            }
            i += 1;
        }

        let w = |widths: &Vec<u16>, k: usize| i32::from(widths.get(k).copied().unwrap_or(0));
        for (g_index, g) in groups.iter().enumerate() {
            // The character before takes the left overhang.
            if g.start > 0 {
                let before = g.start - 1;
                if is_cjk_ideograph(last_codepoint(self.word(before))) {
                    if let Some(width) = widths.get_mut(before) {
                        add_u16(width, g.left);
                    }
                } else {
                    let max_overhang = w(widths, before) / 2;
                    if let Some(width) = widths.get_mut(before) {
                        add_u16(width, 0.max(g.left - max_overhang));
                    }
                }
            }
            // The character after takes the right one, or the next group collides.
            let next = g.start + g.count;
            let last = g.start + g.count - 1;
            if next >= self.words.len() {
                continue;
            }
            let following = groups.get(g_index + 1);
            if let Some(next_group) = following.filter(|n| n.start == next) {
                let collision = g.right + next_group.left;
                if collision > 0
                    && let Some(width) = widths.get_mut(last)
                {
                    add_u16(width, collision);
                }
                continue;
            }
            if is_cjk_ideograph(first_codepoint(self.word(next))) {
                if let Some(width) = widths.get_mut(last) {
                    add_u16(width, g.right);
                }
            } else {
                let max_overhang = w(widths, next) / 2;
                if let Some(width) = widths.get_mut(last) {
                    add_u16(width, 0.max(g.right - max_overhang));
                }
            }
            // A next group separated only by non-ideographs.
            if let Some(next_group) = following {
                let mut only_non_ideographs = true;
                let mut gap = 0;
                for k in next..next_group.start {
                    if is_cjk_ideograph(first_codepoint(self.word(k))) {
                        only_non_ideographs = false;
                        break;
                    }
                    gap += w(widths, k);
                }
                if only_non_ideographs {
                    let allowed_right = g.right.min(w(widths, last) / 2);
                    let allowed_left = next_group.left.min(w(widths, next_group.start - 1) / 2);
                    let touch = allowed_right + allowed_left - gap;
                    if touch > 0
                        && let Some(width) = widths.get_mut(last)
                    {
                        add_u16(width, touch);
                    }
                }
            }
        }
    }

    /// Minimum-raggedness line breaks; words too wide for any line are split first.
    #[allow(clippy::too_many_lines)]
    #[inline(never)]
    fn line_breaks(
        &mut self,
        m: &mut Measure<'_>,
        block: BlockStyle,
        params: &Params,
        widths: &mut Vec<u16>,
    ) -> Option<Vec<usize>> {
        if self.words.is_empty() {
            return Some(Vec::new());
        }
        let font = params.font;
        let page = i32::from(params.width);
        let indent = self.first_line_indent(true, m, font, block);
        let mut i = 0;
        while i < widths.len() {
            let effective = if i == 0 { page - indent } else { page };
            while i32::from(widths.get(i).copied().unwrap_or(0)) > effective {
                if !self.hyphenate(m, i, effective, params, widths, true) {
                    break;
                }
            }
            i += 1;
        }

        let n = self.words.len();
        let mut dp: Vec<i32> = Vec::new();
        let mut ans: Vec<usize> = Vec::new();
        dp.try_reserve_exact(n).ok()?;
        ans.try_reserve_exact(n).ok()?;
        dp.resize(n, 0);
        ans.resize(n, 0);
        if let (Some(d), Some(a)) = (dp.get_mut(n - 1), ans.get_mut(n - 1)) {
            *d = 0;
            *a = n - 1;
        }
        for i in (0..n.saturating_sub(1)).rev() {
            let mut length = 0;
            let mut best = i32::MAX;
            let mut best_end = 0;
            let effective = if i == 0 { page - indent } else { page };
            for j in i..n {
                let gap = if j > i {
                    m.gap(
                        font,
                        self.word(j - 1),
                        self.word(j),
                        self.word_style(j - 1),
                        Self::flag(&self.continues, j),
                        Self::flag(&self.no_space_before, j),
                    )
                } else {
                    0
                };
                let extra_start = if j == i {
                    self.ruby_extra_start(m, i, n, font)
                } else {
                    0
                };
                length += i32::from(widths.get(j).copied().unwrap_or(0)) + gap + extra_start;
                if length > effective {
                    break;
                }
                if j + 1 < n
                    && !allows_break(
                        Self::flag(&self.continues, j + 1),
                        Self::flag(&self.no_space_before, j + 1),
                    )
                {
                    continue;
                }
                let extra_end = self.ruby_extra_end(m, i, j + 1, font);
                if length + extra_end > effective {
                    continue;
                }
                let cost = if j == n - 1 {
                    0
                } else {
                    let remaining = i64::from(effective - length);
                    let next = i64::from(dp.get(j + 1).copied().unwrap_or(0));
                    i32::try_from(remaining * remaining + next).unwrap_or(i32::MAX)
                };
                // Equal costs favour longer lines (fewer short CJK lines).
                if cost <= best {
                    best = cost;
                    best_end = j;
                }
            }
            if best == i32::MAX {
                // A word that fits nowhere gets its own line and passes on the cost after it.
                best_end = i;
                best = if i + 1 < n {
                    dp.get(i + 1).copied().unwrap_or(0)
                } else {
                    0
                };
            }
            if let (Some(d), Some(a)) = (dp.get_mut(i), ans.get_mut(i)) {
                *d = best;
                *a = best_end;
            }
        }

        let mut breaks = Vec::new();
        let mut current = 0;
        while current < n {
            let mut next = ans.get(current).copied().unwrap_or(current) + 1;
            if next <= current {
                next = current + 1;
            }
            breaks.try_reserve(1).ok()?;
            breaks.push(next);
            current = next;
        }
        Some(breaks)
    }

    /// Greedy line breaks that split the word overflowing a line at its widest fitting break.
    #[inline(never)]
    fn hyphenated_line_breaks(
        &mut self,
        m: &mut Measure<'_>,
        block: BlockStyle,
        params: &Params,
        widths: &mut Vec<u16>,
    ) -> Option<Vec<usize>> {
        let font = params.font;
        let page = i32::from(params.width);
        let indent = self.first_line_indent(true, m, font, block);
        let mut breaks = Vec::new();
        let mut current = 0;
        let mut first_line = !self.first_line_consumed;
        while current < widths.len() {
            let start = current;
            let mut line_width = 0;
            let effective = if first_line { page - indent } else { page };
            while current < widths.len() {
                let first_word = current == start;
                let spacing = if first_word {
                    0
                } else {
                    m.gap(
                        font,
                        self.word(current - 1),
                        self.word(current),
                        self.word_style(current - 1),
                        Self::flag(&self.continues, current),
                        Self::flag(&self.no_space_before, current),
                    )
                };
                let candidate = spacing + i32::from(widths.get(current).copied().unwrap_or(0));
                if line_width + candidate <= effective {
                    line_width += candidate;
                    current += 1;
                    continue;
                }
                let available = effective - line_width - spacing;
                if available > 0
                    && self.hyphenate(m, current, available, params, widths, first_word)
                {
                    current += 1;
                    break;
                }
                if current == start {
                    current += 1;
                }
                break;
            }
            // Never break before a continuation: the whole group moves on.
            while current > start + 1
                && current < widths.len()
                && !allows_break(
                    Self::flag(&self.continues, current),
                    Self::flag(&self.no_space_before, current),
                )
            {
                current -= 1;
            }
            breaks.try_reserve(1).ok()?;
            breaks.push(current);
            first_line = false;
        }
        Some(breaks)
    }

    /// Splits token `index` at its widest break whose prefix (with a hyphen where one is drawn)
    /// fits `available`.
    #[allow(clippy::too_many_lines)]
    #[inline(never)]
    fn hyphenate(
        &mut self,
        m: &mut Measure<'_>,
        index: usize,
        available: i32,
        params: &Params,
        widths: &mut Vec<u16>,
        allow_fallback: bool,
    ) -> bool {
        if available <= 0 || index >= self.words.len() {
            return false;
        }
        let mut word = Vec::new();
        if word.try_reserve(self.word(index).len()).is_err() {
            return false;
        }
        word.extend_from_slice(self.word(index));
        let word_style = self.word_style(index);
        let focus = self.focus.get(index).copied().unwrap_or(0);
        let font = self.word_font(index, params.font, params);

        let Some(candidates) = break_candidates(&word, params.language, allow_fallback) else {
            m.failed = true;
            return false;
        };
        if candidates.is_empty() {
            return false;
        }

        let mut chosen_offset = 0;
        let mut chosen_width = -1;
        let mut chosen_hyphen = true;
        for &(offset, hyphen) in &candidates {
            if offset == 0 || offset >= word.len() {
                continue;
            }
            let prefix_focus = u8::try_from(usize::from(focus).min(offset)).unwrap_or(u8::MAX);
            let width = i32::from(m.focus_word(
                font,
                word.get(..offset).unwrap_or_default(),
                word_style,
                prefix_focus,
                hyphen,
            ));
            if width > available || width <= chosen_width {
                continue;
            }
            chosen_width = width;
            chosen_offset = offset;
            chosen_hyphen = hyphen;
        }
        if chosen_width < 0 {
            return false;
        }

        let prefix_length = count_codepoints(word.get(..chosen_offset).unwrap_or_default());
        let remainder_offset = self
            .absorbed
            .offset_after(self.offsets.at(index), prefix_length);
        if chosen_offset + 1 > MAX_PREFIX {
            return false;
        }
        if self.reserve_tokens(1).is_err()
            || self.offsets.reserve_insert().is_err()
            || widths.try_reserve(1).is_err()
            || (!self.ruby.is_empty() && self.ruby.try_reserve(1).is_err())
        {
            return false;
        }
        // The prefix is a new entry with its hyphen; the remainder is the old entry's tail.
        let Some(prefix) =
            self.store_split(word.get(..chosen_offset).unwrap_or_default(), chosen_hyphen)
        else {
            return false;
        };
        if self.offsets.insert(index + 1, remainder_offset).is_err() {
            return false;
        }
        let Some(&original) = self.words.get(index) else {
            return false;
        };
        let remainder = original.suffix(chosen_offset);
        if let Some(w) = self.words.get_mut(index) {
            *w = prefix;
        }
        let stored_style = word_style & !style::SMALL_CAPS;
        if let Some(s) = self.styles.get_mut(index) {
            *s = stored_style
                | if chosen_hyphen {
                    style::DISCRETIONARY_HYPHEN
                } else {
                    0
                };
        }
        self.words.insert(index + 1, remainder);
        self.styles.insert(index + 1, stored_style);
        // Emphasis follows the text: a break at or after the boundary leaves the rest regular.
        let after = if usize::from(focus) > chosen_offset {
            u8::try_from(usize::from(focus) - chosen_offset).unwrap_or(0)
        } else {
            0
        };
        self.focus.insert(index + 1, after);
        let link = self.links.get(index).copied().unwrap_or(0);
        self.links.insert(index + 1, link);
        let slot = self.slots.get(index).copied().unwrap_or(0);
        self.slots.insert(index + 1, slot);
        let before = u8::try_from(usize::from(focus).min(chosen_offset)).unwrap_or(u8::MAX);
        // An all-bold part carries BOLD with boundary 0; an inserted hyphen counts as covered.
        if usize::from(before) + usize::from(chosen_hyphen) >= usize::from(prefix.len) {
            if let Some(s) = self.styles.get_mut(index) {
                *s |= style::BOLD;
            }
            if let Some(f) = self.focus.get_mut(index) {
                *f = 0;
            }
        } else if let Some(f) = self.focus.get_mut(index) {
            *f = before;
        }
        if index < self.ruby.len() {
            self.ruby.insert(index + 1, Vec::new());
        }
        // The prefix keeps its attachment (a no-break group stays together); the remainder
        // starts the next line.
        self.continues.insert(index + 1, false);
        self.no_space_before.insert(index + 1, false);

        if let Some(w) = widths.get_mut(index) {
            *w = to_u16(chosen_width);
        }
        let remainder_width = m.focus_word(
            font,
            self.word(index + 1),
            word_style,
            self.focus.get(index + 1).copied().unwrap_or(0),
            false,
        );
        widths.insert(index + 1, remainder_width);
        true
    }

    /// Stores a split prefix with its hyphen; out of memory skips the split, it does not drop text.
    fn store_split(&mut self, prefix: &[u8], hyphen: bool) -> Option<Word> {
        let mut buffer = [0u8; MAX_PREFIX];
        let len = prefix.len() + usize::from(hyphen);
        buffer.get_mut(..prefix.len())?.copy_from_slice(prefix);
        if hyphen {
            *buffer.get_mut(prefix.len())? = b'-';
        }
        self.store.append(buffer.get(..len)?)
    }

    /// Lays the paragraph out, handing each line to `emit` (false when it could not be kept),
    /// and drops the words used. `block.is_rtl` is set when the text decides the direction.
    pub fn layout(
        &mut self,
        metrics: &mut dyn Metrics,
        block: &mut BlockStyle,
        params: &Params,
        emit: &mut dyn FnMut(&Line<'_>) -> bool,
    ) {
        if self.words.is_empty() {
            return;
        }
        self.character_spacing = params.character_spacing;
        self.word_spacing = params.word_spacing;
        if !block.direction_defined && self.has_rtl_word {
            for i in 0..self.words.len().min(RTL_PARAGRAPH_PROBE_WORDS) {
                if bembeyaz_bidi::first_strong_is_rtl(self.word(i), RTL_PARAGRAPH_PROBE_DEPTH)
                    == Some(true)
                {
                    block.is_rtl = true;
                    break;
                }
            }
        }
        self.natural_align = block.align == Align::Justify
            || if block.is_rtl {
                block.align == Align::Right
            } else {
                block.align == Align::Left
            };
        self.prewarm(metrics, params);

        let mut scratch = core::mem::take(&mut self.scratch.measure);
        let mut m = Measure {
            metrics,
            scratch: &mut scratch,
            spacing: params.character_spacing,
            word_spacing: params.word_spacing,
            failed: false,
        };
        let mut widths = Vec::new();
        if widths.try_reserve(self.words.len()).is_err() {
            self.dropped = true;
            return;
        }
        self.word_widths(&mut m, params.font, params, &mut widths);
        let breaks = if self.hyphenation {
            self.hyphenated_line_breaks(&mut m, *block, params, &mut widths)
        } else {
            self.line_breaks(&mut m, *block, params, &mut widths)
        };
        let Some(breaks) = breaks else {
            self.dropped = true;
            self.scratch.measure = scratch;
            return;
        };
        let available = if params.include_last_line {
            breaks.len()
        } else {
            breaks.len().saturating_sub(1)
        };
        let line_count = params.max_lines.min(available);
        let end_known = params.include_last_line && line_count == breaks.len();
        for i in 0..line_count {
            let lines_after =
                end_known.then(|| u32::try_from(line_count - 1 - i).unwrap_or(u32::MAX));
            self.extract_line(
                &mut m,
                *block,
                params,
                &widths,
                &breaks,
                i,
                lines_after,
                emit,
            );
        }
        if m.failed {
            self.dropped = true;
        }
        self.scratch.measure = scratch;

        if line_count > 0 {
            self.first_line_consumed = true;
            self.start_emitted = true;
            let consumed = breaks.get(line_count - 1).copied().unwrap_or(0);
            self.consume(consumed);
        }
    }

    /// Loads SD-card font advances for every size of the paragraph before measuring.
    fn prewarm(&self, metrics: &mut dyn Metrics, params: &Params) {
        let mut fonts = [params.font; MAX_SIZE_SLOTS];
        let mut count = 1;
        for slot in 1..usize::from(params.slot_count) {
            let font = params.slot_fonts.get(slot).copied().unwrap_or(0);
            if font != 0
                && !fonts.get(..count).unwrap_or_default().contains(&font)
                && let Some(entry) = fonts.get_mut(count)
            {
                *entry = font;
                count += 1;
            }
        }
        let fonts = fonts.get(..count).unwrap_or_default();
        if !fonts.iter().any(|&f| metrics.is_sd_font(f)) {
            return;
        }
        let mut mask = 0u8;
        for &s in &self.styles {
            mask |= 1 << (s & 3);
        }
        if mask == 0 {
            mask = 1;
        }
        let small_caps = self.slots.iter().any(|&s| s & style::SMALL_CAPS_SLOT != 0);
        let mut segments = Vec::new();
        if segments.try_reserve(self.store.segment_count()).is_err() {
            return;
        }
        segments.extend(self.store.segments());
        for &font in fonts {
            if metrics.is_sd_font(font) {
                metrics.prewarm(
                    font,
                    &segments,
                    self.words.len() > 1,
                    self.hyphenation,
                    mask,
                    small_caps,
                );
            }
        }
    }

    /// Forgets the first `count` tokens.
    fn consume(&mut self, count: usize) {
        let count = count.min(self.words.len());
        for &word in self.words.get(..count).unwrap_or_default() {
            self.store.release(word);
        }
        self.words.drain(..count);
        self.styles.drain(..count);
        self.continues.drain(..count);
        self.no_space_before.drain(..count);
        self.focus.drain(..count);
        self.links.drain(..count);
        self.slots.drain(..count);
        let remaining_start = self.offsets.erase_prefix(count);
        self.absorbed.retire(remaining_start);
        let ruby = count.min(self.ruby.len());
        self.ruby.drain(..ruby);
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    #[inline(never)]
    fn extract_line(
        &mut self,
        m: &mut Measure<'_>,
        block: BlockStyle,
        params: &Params,
        widths: &[u16],
        breaks: &[usize],
        index: usize,
        lines_after: Option<u32>,
        emit: &mut dyn FnMut(&Line<'_>) -> bool,
    ) {
        let font = params.font;
        let line_break = breaks.get(index).copied().unwrap_or(0);
        let start = if index > 0 {
            breaks.get(index - 1).copied().unwrap_or(0)
        } else {
            0
        };
        let count = line_break.saturating_sub(start);
        let visible_offset = self.offsets.at(start);
        let indent = self.first_line_indent(index == 0, m, font, block);
        let extra_start = self.ruby_extra_start(m, start, line_break, font);
        let extra_end = self.ruby_extra_end(m, start, line_break, font);
        let spacing = i32::from(params.character_spacing);
        let width = |k: usize| i32::from(widths.get(k).copied().unwrap_or(0));

        let mut lb = core::mem::take(&mut self.scratch.line);
        let ok = self.build_words(&mut lb, start, count);
        if !ok {
            self.dropped = true;
            self.scratch.line = lb;
            return;
        }

        let mut sum = 0;
        let mut gap_count = 0usize;
        let mut natural = 0;
        for i in 0..count {
            sum += width(start + i);
            if i == 0 {
                continue;
            }
            let boundary = start + i;
            let continues = Self::flag(&self.continues, boundary);
            let no_space = Self::flag(&self.no_space_before, boundary);
            if is_justifiable_gap(continues, no_space, line_word(&lb, i) == b" ") {
                gap_count += 1;
            }
            let left_style = lb.styles.get(i - 1).copied().unwrap_or(0);
            natural += if continues {
                m.kerning(font, line_word(&lb, i - 1), line_word(&lb, i), left_style)
            } else if no_space {
                spacing
            } else {
                m.space(font, line_word(&lb, i - 1), line_word(&lb, i), left_style)
            };
        }

        let page = i32::from(params.width) - indent;
        let last_line = index + 1 == breaks.len();
        let align = if block.is_rtl && !block.text_align_defined && block.align == Align::Left {
            Align::Right
        } else {
            block.align
        };
        let justify = align == Align::Justify && !last_line;
        let spare = page - extra_start - extra_end - sum - natural;
        let extra = if justify {
            justify_extra(spare, gap_count)
        } else {
            0
        };

        // Bidirectional reordering, only for paragraphs with right-to-left text.
        let mut bidi = core::mem::take(&mut self.scratch.bidi);
        let mut reorder = false;
        if (block.is_rtl || self.has_rtl_word)
            && count > 1
            && count <= bembeyaz_bidi::MAX_LINE
            && bidi.order.try_reserve(count).is_ok()
            && bidi.continues.try_reserve(count).is_ok()
            && bidi.no_space.try_reserve(count).is_ok()
        {
            bidi.order.clear();
            bidi.order.resize(count, 0);
            if self.bidi.is_none() {
                self.bidi = new_workspace();
            }
            if let Some(workspace) = self.bidi.as_deref_mut() {
                reorder = bembeyaz_bidi::word_order(
                    count,
                    |i| line_word(&lb, i),
                    block.is_rtl,
                    workspace,
                    &mut bidi.order,
                ) == Some(true);
            }
        }
        let src = |i: usize| -> usize {
            if reorder {
                usize::from(bidi.order.get(i).copied().unwrap_or(0))
            } else {
                i
            }
        };

        lb.xpos.clear();
        if lb.xpos.try_reserve(count).is_err() {
            self.dropped = true;
            self.scratch.line = lb;
            return;
        }
        if reorder {
            if !self.reorder_words(&mut lb, &bidi.order, count) {
                self.dropped = true;
                self.scratch.line = lb;
                return;
            }
            for i in 0..count {
                let s = src(i);
                let mut continues = false;
                if i > 0 {
                    let prev = src(i - 1);
                    // A logical pair may appear either way round once reordered.
                    continues = (s == prev + 1 && Self::flag(&self.continues, start + s))
                        || (prev == s + 1 && Self::flag(&self.continues, start + prev));
                }
                if i == 0 {
                    bidi.continues.clear();
                    bidi.no_space.clear();
                }
                bidi.continues.push(continues);
                bidi.no_space
                    .push(!continues && Self::flag(&self.no_space_before, start + s));
            }
            let rc = |i: usize| bidi.continues.get(i).copied().unwrap_or(false);
            let rn = |i: usize| bidi.no_space.get(i).copied().unwrap_or(false);
            let mut r_sum = 0;
            let mut r_gaps = 0usize;
            let mut r_natural = 0;
            for i in 0..count {
                r_sum += width(start + src(i));
                if i == 0 {
                    continue;
                }
                let left_style = lb.styles.get(i - 1).copied().unwrap_or(0);
                if rn(i) {
                    r_gaps += 1;
                    r_natural += spacing;
                } else if !rc(i) {
                    r_gaps += 1;
                    r_natural +=
                        m.space(font, line_word(&lb, i - 1), line_word(&lb, i), left_style);
                } else {
                    if line_word(&lb, i) == b" " {
                        r_gaps += 1;
                    }
                    r_natural +=
                        m.kerning(font, line_word(&lb, i - 1), line_word(&lb, i), left_style);
                }
            }
            let r_spare = page - extra_start - extra_end - r_sum - r_natural;
            let r_extra = if justify {
                justify_extra(r_spare, r_gaps)
            } else {
                0
            };
            let contribution = if justify {
                r_extra * i32::try_from(r_gaps).unwrap_or(0)
            } else {
                0
            };
            let content = r_sum + r_natural + contribution;
            let mut x = if block.is_rtl {
                match align {
                    Align::Right | Align::Justify => page - content,
                    Align::Center => (page - content) / 2,
                    _ => 0,
                }
            } else {
                match align {
                    Align::Right => page - content,
                    Align::Center => (page - content) / 2,
                    _ => indent,
                }
            };
            for i in 0..count {
                lb.xpos.push(to_i16(x));
                x += width(start + src(i));
                if i + 1 < count && rc(i + 1) {
                    let left_style = lb.styles.get(i).copied().unwrap_or(0);
                    let mut advance =
                        m.kerning(font, line_word(&lb, i), line_word(&lb, i + 1), left_style);
                    // A leading no-break space is not a justifiable gap (issue #2185).
                    if i > 0 && line_word(&lb, i) == b" " && rc(i) && justify {
                        advance += r_extra;
                    }
                    x += advance;
                } else if i + 1 < count {
                    let left_style = lb.styles.get(i).copied().unwrap_or(0);
                    let mut gap = if rn(i + 1) {
                        spacing
                    } else {
                        m.space(font, line_word(&lb, i), line_word(&lb, i + 1), left_style)
                    };
                    if justify {
                        gap += r_extra;
                    }
                    x += gap;
                }
            }
        } else if block.is_rtl {
            let mut x = match align {
                Align::Left => sum + natural,
                Align::Center => (page + sum + natural) / 2,
                _ => page,
            };
            for i in 0..count {
                x -= width(start + i);
                lb.xpos.push(to_i16(x));
                let left_style = lb.styles.get(i).copied().unwrap_or(0);
                if i + 1 < count && Self::flag(&self.continues, start + i + 1) {
                    let mut advance =
                        m.kerning(font, line_word(&lb, i), line_word(&lb, i + 1), left_style);
                    if i > 0
                        && line_word(&lb, i) == b" "
                        && Self::flag(&self.continues, start + i)
                        && justify
                    {
                        advance += extra;
                    }
                    x -= advance;
                } else {
                    let mut gap = 0;
                    if i + 1 < count {
                        gap = if Self::flag(&self.no_space_before, start + i + 1) {
                            spacing
                        } else {
                            m.space(font, line_word(&lb, i), line_word(&lb, i + 1), left_style)
                        };
                    }
                    if i + 1 < count && justify {
                        gap += extra;
                    }
                    x -= gap;
                }
            }
        } else {
            let mut x = match align {
                Align::Right => page - sum - natural,
                Align::Center => (page - sum - natural) / 2,
                _ => indent + extra_start,
            };
            for i in 0..count {
                lb.xpos.push(to_i16(x));
                let left_style = lb.styles.get(i).copied().unwrap_or(0);
                if i + 1 < count && Self::flag(&self.continues, start + i + 1) {
                    let mut advance = width(start + i)
                        + m.kerning(font, line_word(&lb, i), line_word(&lb, i + 1), left_style);
                    if i > 0
                        && line_word(&lb, i) == b" "
                        && Self::flag(&self.continues, start + i)
                        && justify
                    {
                        advance += extra;
                    }
                    x += advance;
                } else {
                    let mut gap = 0;
                    if i + 1 < count {
                        gap = if Self::flag(&self.no_space_before, start + i + 1) {
                            spacing
                        } else {
                            m.space(font, line_word(&lb, i), line_word(&lb, i + 1), left_style)
                        };
                    }
                    if i + 1 < count && justify {
                        gap += extra;
                    }
                    x += width(start + i) + gap;
                }
            }
        }

        // Links, merged per target.
        lb.links.clear();
        for i in 0..count {
            let link = self.links.get(start + src(i)).copied().unwrap_or(0);
            if link == 0 || usize::from(link) > params.link_count {
                continue;
            }
            let x = i32::from(lb.xpos.get(i).copied().unwrap_or(0));
            let w = width(start + src(i));
            let raised = if lb.styles.get(i).is_some_and(|&s| s & style::SUP != 0) {
                m.metrics.ascender(font) * 2 / 5
            } else {
                0
            };
            if let Some(span) = lb.links.iter_mut().find(|s| s.link == link) {
                let left = i32::from(span.x).min(x);
                let right = (i32::from(span.x) + i32::from(span.width)).max(x + w);
                span.x = to_i16(left);
                span.width = to_i16(right - left);
                span.top_lift = span.top_lift.max(to_i16(raised));
            } else if lb.links.try_reserve(1).is_ok() {
                lb.links.push(LinkSpan {
                    link,
                    x: to_i16(x),
                    width: to_i16(w),
                    top_lift: to_i16(raised),
                });
            }
        }

        // Word fonts: lines mixing sizes or using small caps carry a slot per word.
        let mut fonts = [0i32; MAX_WORD_FONTS];
        lb.word_slots.clear();
        if count <= MAX_FONT_SLOT_WORDS && lb.word_slots.try_reserve(count).is_ok() {
            let mut mixed = false;
            for i in 0..count {
                let logical = start + src(i);
                let word_font = self.word_font(logical, font, params);
                let mut slot = 0usize;
                if word_font != font {
                    slot = 1;
                    while slot <= MAX_WORD_FONTS {
                        let existing = fonts.get(slot - 1).copied().unwrap_or(0);
                        if existing == word_font || existing == 0 {
                            break;
                        }
                        slot += 1;
                    }
                    if slot > MAX_WORD_FONTS {
                        slot = 0;
                    }
                    if slot > 0
                        && let Some(f) = fonts.get_mut(slot - 1)
                    {
                        *f = word_font;
                    }
                }
                let small_caps = self
                    .slots
                    .get(logical)
                    .map_or(0, |&s| s & style::SMALL_CAPS_SLOT);
                let value = u8::try_from(slot).unwrap_or(0) | small_caps;
                mixed |= value != 0;
                lb.word_slots.push(value);
            }
            if !mixed {
                lb.word_slots.clear();
            }
        }

        // Source ranges, through reordering and inserted hyphens.
        lb.ranges.clear();
        if lb.ranges.try_reserve(count).is_err() {
            self.dropped = true;
            self.scratch.line = lb;
            return;
        }
        let mut paragraph_start = u16::MAX;
        for i in 0..count {
            let logical = start + src(i);
            if !self.start_emitted && logical == 0 {
                paragraph_start = u16::try_from(i).unwrap_or(u16::MAX);
            }
            let range_start = self.offsets.at(logical);
            let hyphen = self.style_at(logical) & style::DISCRETIONARY_HYPHEN != 0;
            let length = count_codepoints(self.word(logical)).wrapping_sub(u32::from(hyphen));
            let mut range_end = self.absorbed.offset_after(range_start, length);
            if hyphen && let Some(s) = lb.styles.get_mut(i) {
                *s |= style::DISCRETIONARY_HYPHEN;
            }
            if logical + 1 < self.words.len() && self.offsets.at(logical + 1) > range_start {
                range_end = range_end.min(self.offsets.at(logical + 1));
            }
            lb.ranges.push([range_start, range_end]);
        }

        // Focus Reading: each split word's boundary and the x where its regular suffix starts.
        lb.focus.clear();
        lb.suffix_x.clear();
        let has_focus = (0..count).any(|i| self.focus.get(start + src(i)).is_some_and(|&f| f != 0));
        if has_focus {
            if lb.focus.try_reserve(count).is_err() || lb.suffix_x.try_reserve(count).is_err() {
                self.dropped = true;
                self.scratch.line = lb;
                return;
            }
            for i in 0..count {
                let logical = start + src(i);
                let boundary = self.focus.get(logical).copied().unwrap_or(0);
                lb.focus.push(boundary);
                let suffix = if boundary == 0 {
                    0
                } else {
                    let word_font = self.word_font(logical, font, params);
                    let style_at = self.word_style(logical);
                    m.focus_prefix(word_font, line_word(&lb, i), style_at, boundary)
                };
                lb.suffix_x.push(suffix);
            }
        }

        if !self.build_ruby(&mut lb, start, line_break, count) {
            self.dropped = true;
            self.scratch.line = lb;
            return;
        }

        let line = Line {
            text: &lb.text,
            word_starts: &lb.word_starts,
            xpos: &lb.xpos,
            styles: &lb.styles,
            focus: &lb.focus,
            suffix_x: &lb.suffix_x,
            ruby_text: &lb.ruby_text,
            ruby_starts: &lb.ruby_starts,
            links: &lb.links,
            ranges: &lb.ranges,
            paragraph_start,
            word_slots: &lb.word_slots,
            word_fonts: fonts,
            visible_offset,
            lines_after,
            is_rtl: block.is_rtl,
        };
        if !emit(&line) {
            self.dropped = true;
        }
        self.scratch.line = lb;
        self.scratch.bidi = bidi;
    }

    /// The line's words without soft hyphens, and their styles without small caps.
    fn build_words(&self, lb: &mut LineBuffers, start: usize, count: usize) -> bool {
        lb.text.clear();
        lb.word_starts.clear();
        lb.styles.clear();
        let bytes: usize = (start..start + count).map(|k| self.word(k).len() + 1).sum();
        if lb.text.try_reserve(bytes).is_err()
            || lb.word_starts.try_reserve(count).is_err()
            || lb.styles.try_reserve(count).is_err()
        {
            return false;
        }
        for k in start..start + count {
            lb.word_starts
                .push(u32::try_from(lb.text.len()).unwrap_or(u32::MAX));
            let word = self.word(k);
            if contains(word, SOFT_HYPHEN) {
                push_without_soft_hyphens(&mut lb.text, word);
            } else {
                lb.text.extend_from_slice(word);
            }
            lb.text.push(0);
            lb.styles.push(self.word_style(k) & !style::SMALL_CAPS);
        }
        true
    }

    /// Puts the line's words and styles in visual `order`, through the spare buffers.
    fn reorder_words(&self, lb: &mut LineBuffers, order: &[u16], count: usize) -> bool {
        let _ = self;
        lb.spare_text.clear();
        lb.spare_starts.clear();
        lb.spare_styles.clear();
        if lb.spare_text.try_reserve(lb.text.len()).is_err()
            || lb.spare_starts.try_reserve(count).is_err()
            || lb.spare_styles.try_reserve(count).is_err()
        {
            return false;
        }
        for &s in order.get(..count).unwrap_or_default() {
            let s = usize::from(s);
            let begin = lb.word_starts.get(s).copied().unwrap_or(0) as usize;
            let end = lb
                .word_starts
                .get(s + 1)
                .map_or(lb.text.len(), |&n| n as usize);
            lb.spare_starts
                .push(u32::try_from(lb.spare_text.len()).unwrap_or(u32::MAX));
            lb.spare_text
                .extend_from_slice(lb.text.get(begin..end).unwrap_or_default());
            lb.spare_styles.push(lb.styles.get(s).copied().unwrap_or(0));
        }
        core::mem::swap(&mut lb.text, &mut lb.spare_text);
        core::mem::swap(&mut lb.word_starts, &mut lb.spare_starts);
        core::mem::swap(&mut lb.styles, &mut lb.spare_styles);
        true
    }

    /// One ruby text per word of the line, in logical order.
    fn build_ruby(
        &self,
        lb: &mut LineBuffers,
        start: usize,
        line_break: usize,
        count: usize,
    ) -> bool {
        lb.ruby_text.clear();
        lb.ruby_starts.clear();
        let copied = if !self.ruby.is_empty() && start < self.ruby.len() {
            line_break.min(self.ruby.len()) - start
        } else {
            0
        };
        let line_ruby = self.ruby.get(start..start + copied).unwrap_or_default();
        let bytes = count + line_ruby.iter().map(Vec::len).sum::<usize>();
        if lb.ruby_starts.try_reserve(count).is_err() || lb.ruby_text.try_reserve(bytes).is_err() {
            return false;
        }
        for i in 0..count {
            lb.ruby_starts
                .push(u32::try_from(lb.ruby_text.len()).unwrap_or(u32::MAX));
            if let Some(ruby) = line_ruby.get(i) {
                lb.ruby_text.extend_from_slice(ruby);
            }
            lb.ruby_text.push(0);
        }
        true
    }
}

/// Word `i` of the line, without its NUL.
fn line_word(lb: &LineBuffers, i: usize) -> &[u8] {
    let begin = lb.word_starts.get(i).copied().unwrap_or(0) as usize;
    let next = lb
        .word_starts
        .get(i + 1)
        .map_or(lb.text.len(), |&n| n as usize);
    lb.text
        .get(begin..next.saturating_sub(1))
        .unwrap_or_default()
}
