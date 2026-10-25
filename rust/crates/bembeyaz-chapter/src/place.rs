//! Laying a text block out and putting its lines, drop cap and rules on the pages.

use crate::blocks::BlockExt;
use crate::chapter::Chapter;
use crate::host::{Host, LineInfo};
use crate::text::Text;
use bembeyaz_css::block::Block;
use bembeyaz_layout::drop_cap::{self, Body, Fonts};
use bembeyaz_layout::pager::Line;
use core::cell::RefCell;

/// A rule is this many rows thick.
const RULE_THICKNESS: u8 = 2;
/// The scale whose font a drop cap enlarges: the family's largest built-in size keeps outlines
/// smoothest.
const DROP_CAP_FONT_SCALE: f32 = 3.0;

struct CapFonts<'a, H> {
    host: &'a mut H,
    body: i32,
    cap: i32,
    bold: bool,
}

impl<H: Host> Fonts for CapFonts<'_, H> {
    fn metrics(&mut self, cap: bool, cp: u32) -> Option<(i32, i32)> {
        if cap {
            self.host.glyph(self.cap, cp, self.bold)
        } else {
            self.host.glyph(self.body, cp, false)
        }
    }
}

impl Chapter {
    pub(crate) fn font_for_scale<H: Host>(&self, host: &mut H, scale: f32) -> i32 {
        let variant = host.font_variant(self.settings.font, scale);
        if variant != 0 {
            variant
        } else {
            self.settings.font
        }
    }

    /// The text's layout font, and its size slots' fonts.
    pub(crate) fn prepare_block_font<H: Host>(
        &self,
        host: &mut H,
        text: &mut Text,
        scale: f32,
    ) -> i32 {
        let font = self.font_for_scale(host, scale);
        text.set_slot_fonts(|s| self.font_for_scale(host, s));
        text.block.font_id = if font == self.settings.font { 0 } else { font };
        font
    }

    /// A pending page-break-after and the block's own page-break-before, once, before the block
    /// (or an image) places anything.
    pub(crate) fn apply_pending_page_break<H: Host>(&mut self, host: &mut H) {
        let mut requested = self.pending_page_break;
        self.pending_page_break = false;
        if let Some(text) = &mut self.text {
            requested |= text.block.page_break_before;
            text.block.page_break_before = false;
        }
        let (pager, mut pages) = self.pages(host);
        pager.page_break(&mut pages, requested);
    }

    /// Puts a line on the page; `paragraph` when it belongs to the block being laid out (not a
    /// table cell's), with the lines after it.
    #[allow(
        clippy::option_option,
        reason = "None: a cell's line; Some: the paragraph's lines after"
    )]
    pub(crate) fn add_line<H: Host>(
        &mut self,
        host: &mut H,
        line: LineInfo,
        offset: u32,
        lines_after: Option<Option<u32>>,
    ) {
        let height = line.height();
        let end = Line {
            paragraph: lines_after.is_some(),
            drop_cap: self.laying_out_drop_cap,
            last_line: lines_after == Some(Some(0)),
        };
        let y = {
            let (pager, mut pages) = self.pages(host);
            pager.line_top(&mut pages, height, offset, end)
        };
        if !self.pager.has_page() {
            host.drop_line(line.handle);
            return;
        }
        // Each page gets the note references of the words it shows.
        self.words_placed = self.words_placed.wrapping_add(i32::from(line.words));
        let words = self.words_placed;
        self.notes
            .take(words, |number, href| host.add_footnote(number, href));
        if host.push_line(line.handle, y) {
            self.pager.line_added(height, offset, lines_after.is_some());
        }
    }

    /// Lays the text block out onto the pages, all of it or (`include_last_line` false) all but its
    /// last line, and with its bottom spacing when the paragraph ends.
    pub(crate) fn make_pages<H: Host>(
        &mut self,
        host: &mut H,
        include_last_line: bool,
        paragraph_end: bool,
    ) {
        let Some(text) = &self.text else {
            return;
        };
        // Latch before the block is replaced.
        if text.dropped_words() {
            self.out_of_memory = true;
        }
        let empty = text.is_empty();
        {
            let (pager, mut pages) = self.pages(host);
            pager.ensure_page(&mut pages);
        }
        if !empty {
            self.apply_pending_page_break(host);
        }
        let line_height = host.line_height(self.settings.font, self.settings.line_compression);
        self.layout_current_block(host, include_last_line);
        let Some(text) = &self.text else {
            return;
        };
        let block = text.block;
        if include_last_line {
            // A heading's lines on this page wait to move with what follows it.
            self.pager.end_block(block.keep_with_next);
            self.laying_out_drop_cap = false;
        }
        if text.dropped_words() {
            self.out_of_memory = true;
        }
        if include_last_line {
            // Notes whose word index is the block's size.
            if self.pager.has_page() {
                self.notes
                    .take_all(|number, href| host.add_footnote(number, href));
            }
            if !paragraph_end {
                return;
            }
            if block.margin_bottom > 0 {
                self.pager.advance(i32::from(block.margin_bottom));
            }
            if block.padding_bottom > 0 {
                self.pager.advance(i32::from(block.padding_bottom));
            }
            if self.settings.extra_paragraph_spacing {
                self.pager.advance(line_height / 2);
            }
        }
    }

    fn layout_current_block<H: Host>(&mut self, host: &mut H, include_last_line: bool) {
        let Some(mut text) = self.text.take() else {
            return;
        };
        let scale = if self.inline_size.valid {
            self.inline_size.scale
        } else {
            text.block.font_scale
        };
        let font = self.prepare_block_font(host, &mut text, scale);
        let inset = i32::from(text.block.total_horizontal_inset());
        let viewport = self.settings.viewport_width;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = if inset < i32::from(viewport) {
            (i32::from(viewport) - inset) as u16
        } else {
            viewport
        };
        let mut top_spacing_applied = self.words_placed != 0;
        if self.drop_cap.len > 0 && self.drop_cap.span_depth < 0 && !text.is_empty() {
            // The lines beside the letter stay on its page.
            self.laying_out_drop_cap = true;
            self.apply_top_spacing(&text, &mut top_spacing_applied);
            self.layout_drop_cap_lines(
                host,
                &mut text,
                font,
                width,
                include_last_line,
                &mut top_spacing_applied,
            );
        }
        self.layout_text(
            host,
            &mut text,
            font,
            width,
            include_last_line,
            usize::MAX,
            &mut top_spacing_applied,
        );
        self.text = Some(text);
    }

    fn apply_top_spacing(&mut self, text: &Text, applied: &mut bool) {
        if *applied {
            return;
        }
        if text.block.margin_top > 0 {
            self.pager.advance(i32::from(text.block.margin_top));
        }
        if text.block.padding_top > 0 {
            self.pager.advance(i32::from(text.block.padding_top));
        }
        *applied = true;
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_text<H: Host>(
        &mut self,
        host: &mut H,
        text: &mut Text,
        font: i32,
        width: u16,
        include_last_line: bool,
        max_lines: usize,
        top_spacing_applied: &mut bool,
    ) {
        let spacing = (
            self.settings.character_spacing,
            self.settings.word_spacing_percent,
        );
        let language = self.settings.language;
        let block = text.block;
        let shared = RefCell::new(host);
        let mut applied = *top_spacing_applied;
        text.layout(
            &shared,
            font,
            width,
            include_last_line,
            spacing,
            max_lines,
            language,
            &mut |host, line, offset, lines_after| {
                if !applied {
                    if block.margin_top > 0 {
                        self.pager.advance(i32::from(block.margin_top));
                    }
                    if block.padding_top > 0 {
                        self.pager.advance(i32::from(block.padding_top));
                    }
                    applied = true;
                }
                self.add_line(host, line, offset, Some(lines_after));
            },
        );
        *top_spacing_applied = applied;
    }

    #[allow(clippy::too_many_arguments)]
    fn layout_drop_cap_lines<H: Host>(
        &mut self,
        host: &mut H,
        text: &mut Text,
        font: i32,
        width: u16,
        include_last_line: bool,
        top_spacing_applied: &mut bool,
    ) {
        let lines = self.drop_cap.lines;
        let letter = self.drop_cap.text;
        let bold = self.drop_cap.bold;
        self.drop_cap = bembeyaz_html::words::InitialLetter::NONE;
        let len = letter.iter().position(|&b| b == 0).unwrap_or(letter.len());

        let cap_font = self.font_for_scale(host, DROP_CAP_FONT_SCALE);
        let body = Body {
            line_height: host.line_height(font, self.settings.line_compression),
            ascender: host.ascender(font),
            space_width: host.space_width(font, 0),
            width,
        };
        let cap = drop_cap::place(
            lines,
            letter.get(..len).unwrap_or_default(),
            &body,
            &mut CapFonts {
                host: &mut *host,
                body: font,
                cap: cap_font,
                bold,
            },
        );
        let offset = self.visible_offset;
        let top = {
            let (pager, mut pages) = self.pages(host);
            i32::from(pager.drop_cap_top(&mut pages, cap.height, offset))
        };
        if !self.pager.has_page() {
            return;
        }
        #[allow(clippy::cast_possible_truncation)]
        let y = (top + cap.baseline) as i16;
        if host.push_drop_cap(
            cap_font,
            cap.scale,
            bold,
            letter.get(..=len).unwrap_or(b"\0"),
            text.block.left_inset(),
            y,
        ) {
            self.pager.added();
            self.pager.note(top, top + cap.height);
        }
        // The spanned lines wrap beside the letter, without a first-line indent.
        text.suppress_first_line_indent();
        text.block.text_indent = 0;
        text.block.text_indent_defined = true;
        #[allow(clippy::cast_possible_truncation)]
        let cap_width = cap.width as i16;
        text.block.margin_left = text.block.margin_left.wrapping_add(cap_width);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let beside = (i32::from(width) - cap.width) as u16;
        self.layout_text(
            host,
            text,
            font,
            beside,
            include_last_line,
            usize::from(lines),
            top_spacing_applied,
        );
        text.block.margin_left = text.block.margin_left.wrapping_sub(cap_width);
        self.pager.reach(top + cap.height);
    }

    /// A horizontal rule, a quarter of the line wide, after the text before it.
    pub(crate) fn emit_rule<H: Host>(&mut self, host: &mut H, style: &Block) {
        if self.word_len > 0 {
            self.flush_word(host);
        }
        if let Some(text) = &self.text {
            let parent = text.block;
            self.start_new_text_block(host, &parent, true);
        }
        #[allow(clippy::cast_possible_truncation)]
        let line_height =
            host.line_height(self.settings.font, self.settings.line_compression) as i16;
        let spacing = line_height / 2;
        let pick = |margin: i16, padding: i16| {
            (if margin > 0 { margin } else { spacing }).wrapping_add(if padding > 0 {
                padding
            } else {
                0
            })
        };
        let top = pick(style.margin_top, style.padding_top);
        let bottom = pick(style.margin_bottom, style.padding_bottom);
        #[allow(clippy::cast_possible_truncation)]
        let available = ((i32::from(self.settings.viewport_width)
            - i32::from(style.total_horizontal_inset())) as i16)
            .max(1);
        let width = (available / 4).max(1);
        let x = style.left_inset().wrapping_add((available - width) / 2);
        let total = top
            .wrapping_add(i16::from(RULE_THICKNESS))
            .wrapping_add(bottom);
        let offset = self.visible_offset;
        let y = {
            let (pager, mut pages) = self.pages(host);
            pager.rule_top(&mut pages, top, total, offset)
        };
        let Some(y) = y else {
            return;
        };
        if !host.push_rule(width, RULE_THICKNESS, x, y) {
            return;
        }
        self.pager
            .rule_added(i16::from(RULE_THICKNESS), bottom, offset);
        let completed = self.pager.completed();
        self.out_of_memory |= self.anchors.record_pending(completed).is_err();
    }
}
