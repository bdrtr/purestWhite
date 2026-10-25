//! One paragraph being built (lib/Epub's `ParsedText`): its words, block style, link targets and
//! inline size slots, and its layout into lines the host makes.

use crate::host::{Host, LineInfo, Links};
use alloc::vec::Vec;
use bembeyaz_css::block::Block;
use bembeyaz_hyphenation::Language;
use bembeyaz_layout::{Align, BlockStyle, MAX_SIZE_SLOTS, Metrics, Paragraph, Params, Reclaim};
use core::cell::RefCell;

/// A link target keeps fewer bytes than this (`FootnoteEntry::href`).
const LINK_LEN: usize = 256;

/// The host shared between the paragraph's metrics and the lines it hands out, which the layout
/// never needs at the same time.
pub struct Shared<'a, 'b, H>(pub &'a RefCell<&'b mut H>);

impl<H: Host> Metrics for Shared<'_, '_, H> {
    fn text_advance(&mut self, font: i32, text: &[u8], style: u8, spacing: i8) -> i32 {
        self.0
            .try_borrow_mut()
            .map_or(0, |mut h| h.text_advance(font, text, style, spacing))
    }
    fn kerning(&mut self, font: i32, left: u32, right: u32, style: u8, spacing: i8) -> i32 {
        self.0
            .try_borrow_mut()
            .map_or(0, |mut h| h.kerning(font, left, right, style, spacing))
    }
    fn space_advance(&mut self, font: i32, left: u32, right: u32, style: u8) -> i32 {
        self.0
            .try_borrow_mut()
            .map_or(0, |mut h| h.space_advance(font, left, right, style))
    }
    fn space_width(&mut self, font: i32, style: u8) -> i32 {
        self.0
            .try_borrow_mut()
            .map_or(0, |mut h| h.space_width(font, style))
    }
    fn ascender(&mut self, font: i32) -> i32 {
        self.0.try_borrow_mut().map_or(0, |mut h| h.ascender(font))
    }
    fn is_sd_font(&mut self, font: i32) -> bool {
        self.0
            .try_borrow_mut()
            .is_ok_and(|mut h| h.is_sd_font(font))
    }
    fn prewarm(
        &mut self,
        font: i32,
        segments: &[&[u8]],
        include_space: bool,
        include_hyphen: bool,
        style_mask: u8,
        small_caps: bool,
    ) {
        if let Ok(mut h) = self.0.try_borrow_mut() {
            h.prewarm(
                font,
                segments,
                include_space,
                include_hyphen,
                style_mask,
                small_caps,
            );
        }
    }
}

/// Link targets back to back, each followed by a NUL.
#[derive(Default)]
struct Targets {
    bytes: Vec<u8>,
    ends: Vec<u32>,
}

impl Links for Targets {
    fn target(&self, id: u8) -> &[u8] {
        let Some(i) = usize::from(id).checked_sub(1) else {
            return b"\0";
        };
        let start = i
            .checked_sub(1)
            .and_then(|p| self.ends.get(p))
            .map_or(0, |&e| e as usize);
        self.ends
            .get(i)
            .and_then(|&end| self.bytes.get(start..end as usize))
            .unwrap_or(b"\0")
    }
}

/// The paragraph being built.
pub struct Text {
    pub paragraph: Paragraph,
    pub block: Block,
    targets: Targets,
    /// Slot 0 follows the block's own size; the others are absolute multiples of the body size.
    slot_scales: [f32; MAX_SIZE_SLOTS],
    slot_fonts: [i32; MAX_SIZE_SLOTS],
    slot_count: u8,
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

/// Gets each laid-out line with its paragraph offset and the lines after it.
pub(crate) type OnLine<'f, H> = dyn FnMut(&mut H, LineInfo, u32, Option<u32>) + 'f;

impl Text {
    #[must_use]
    pub fn new(
        hyphenation: bool,
        focus_reading: bool,
        block: Block,
        indent_spaces: u8,
        reclaim: Option<Reclaim>,
    ) -> Self {
        Self {
            paragraph: Paragraph::new(hyphenation, focus_reading, indent_spaces, reclaim),
            block,
            targets: Targets::default(),
            slot_scales: [1.0; MAX_SIZE_SLOTS],
            slot_fonts: [0; MAX_SIZE_SLOTS],
            slot_count: 1,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.paragraph.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paragraph.len() == 0
    }

    /// The word up to its NUL.
    #[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
    pub fn add_word(
        &mut self,
        word: &[u8],
        style: u8,
        attach: bool,
        offset: u32,
        link: u8,
        slot: u8,
        no_space_before: bool,
    ) {
        self.paragraph.add_word(
            until_nul(word),
            style,
            false,
            attach,
            offset,
            link,
            slot,
            no_space_before,
        );
    }

    /// A link target's id (from 1), or 0 when the target is empty, too long or one too many.
    pub fn add_link_target(&mut self, href: &[u8]) -> u8 {
        let href = until_nul(href);
        if href.is_empty()
            || href.len() >= LINK_LEN
            || self.targets.ends.len() >= usize::from(u8::MAX)
        {
            return 0;
        }
        let t = &mut self.targets;
        if t.bytes.try_reserve(href.len() + 1).is_err() || t.ends.try_reserve(1).is_err() {
            return 0;
        }
        t.bytes.extend_from_slice(href);
        t.bytes.push(0);
        t.ends
            .push(u32::try_from(t.bytes.len()).unwrap_or(u32::MAX));
        u8::try_from(t.ends.len()).unwrap_or(0)
    }

    #[must_use]
    pub fn link_target_matches(&self, id: u8, href: &[u8]) -> bool {
        id > 0
            && usize::from(id) <= self.targets.ends.len()
            && until_nul(self.targets.target(id)) == until_nul(href)
    }

    /// The slot for words at `scale`: a matching one, a new one, or the nearest once all are taken.
    pub fn size_slot_for(&mut self, scale: f32) -> u8 {
        let mut nearest = 1u8;
        for slot in 1..self.slot_count {
            let s = self
                .slot_scales
                .get(usize::from(slot))
                .copied()
                .unwrap_or(1.0);
            let n = self
                .slot_scales
                .get(usize::from(nearest))
                .copied()
                .unwrap_or(1.0);
            if (s - scale).abs() < 0.01 {
                return slot;
            }
            if (s - scale).abs() < (n - scale).abs() {
                nearest = slot;
            }
        }
        if let Some(s) = self.slot_scales.get_mut(usize::from(self.slot_count)) {
            *s = scale;
            self.slot_count += 1;
            return self.slot_count - 1;
        }
        nearest
    }

    /// Resolves each extra size slot's font with `font_for`.
    pub fn set_slot_fonts(&mut self, mut font_for: impl FnMut(f32) -> i32) {
        for slot in 1..usize::from(self.slot_count) {
            if let (Some(&scale), Some(font)) =
                (self.slot_scales.get(slot), self.slot_fonts.get_mut(slot))
            {
                *font = font_for(scale);
            }
        }
    }

    /// Lays the words out, at most `max_lines`, and hands each line the host made to `on_line`
    /// with its first visible character and the lines after it (None while the rest is not laid
    /// out). A line the host could not make is dropped.
    #[allow(clippy::too_many_arguments)]
    pub fn layout<H: Host>(
        &mut self,
        host: &RefCell<&mut H>,
        font: i32,
        width: u16,
        include_last_line: bool,
        spacing: (i8, u8),
        max_lines: usize,
        language: Option<Language>,
        on_line: &mut OnLine<'_, H>,
    ) {
        if self.is_empty() {
            return;
        }
        self.block.character_spacing = spacing.0;
        let mut style = BlockStyle {
            align: Align::from_index(self.block.alignment),
            text_align_defined: self.block.text_align_defined,
            direction_defined: self.block.direction_defined,
            is_rtl: self.block.is_rtl,
            text_indent_defined: self.block.text_indent_defined,
            text_indent: self.block.text_indent,
        };
        let params = Params {
            font,
            slot_fonts: self.slot_fonts,
            slot_count: self.slot_count,
            width,
            include_last_line,
            character_spacing: spacing.0,
            word_spacing: spacing.1,
            max_lines,
            language,
            link_count: self.targets.ends.len(),
        };
        let Self {
            paragraph,
            block,
            targets,
            ..
        } = self;
        let mut metrics = Shared(host);
        paragraph.layout(&mut metrics, &mut style, &params, &mut |line| {
            block.is_rtl = line.is_rtl;
            let Ok(mut h) = host.try_borrow_mut() else {
                return false;
            };
            let Some(info) = h.make_line(line, block, targets) else {
                return false;
            };
            on_line(&mut h, info, line.visible_offset, line.lines_after);
            true
        });
        self.block.is_rtl = style.is_rtl;
    }

    pub fn suppress_first_line_indent(&mut self) {
        self.paragraph.suppress_first_line_indent();
    }

    pub fn reset_first_line_indent(&mut self) {
        self.paragraph.reset_first_line_indent();
    }

    #[must_use]
    pub fn dropped_words(&self) -> bool {
        self.paragraph.dropped_words()
    }
}
