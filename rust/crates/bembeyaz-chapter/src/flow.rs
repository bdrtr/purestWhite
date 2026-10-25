//! The document's events: elements open and close, text arrives, and each changes the styles,
//! the paragraph being built and the pages.

use crate::blocks::BlockExt;
use crate::boxed::Boxed;
use crate::chapter::{Chapter, InlineSize, ListContext, MAX_CSS_ANCESTORS, root_alignment};
use crate::host::{Host, Image};
use crate::text::Text;
use bembeyaz_css::block::{self, Axis, Block};
use bembeyaz_css::inline::{self, Entry, Kind};
use bembeyaz_css::{
    Ancestor, BorderSide, BorderStyle, Decoration, Defined, Direction, Display, FontWeight,
    LengthField, ListStyle, Query, Style, TextAlign,
};
use bembeyaz_html::element::{self, name};
use bembeyaz_html::list::{self, List, MAX_MARKER};
use bembeyaz_html::words::{self, InitialLetter, MAX_WORD, Piece, Scan, Words};
use bembeyaz_xml::{Flow, Handler, StartTag, Str};

/// A start tag: its name, attributes and where it begins in the chapter.
pub trait Tag {
    fn name(&self) -> &[u8];
    fn attributes(&self) -> impl Iterator<Item = (&[u8], &[u8])>;
    fn offset(&self) -> u64;
}

impl Tag for StartTag<'_> {
    fn name(&self) -> &[u8] {
        StartTag::name(self).bytes()
    }
    fn attributes(&self) -> impl Iterator<Item = (&[u8], &[u8])> {
        StartTag::attributes(self)
            .entries()
            .map(|(n, v)| (n.bytes(), v.bytes()))
    }
    fn offset(&self) -> u64 {
        StartTag::offset(self)
    }
}

/// A text block buffers this many words before laying out all but its last line.
const SOFT_FLUSH_WORDS: usize = 750;
/// Earlier with the book's CSS on: spans fragment the text into many small words.
const SOFT_FLUSH_WORDS_WITH_CSS: usize = 320;
/// A table cell's side padding when the book sets none.
pub(crate) const CELL_HORIZONTAL_PADDING: i16 = 4;
/// A grid row buffers at most this many words and bytes; larger rows are stacked.
pub(crate) const MAX_ROW_WORDS: usize = 160;
pub(crate) const MAX_ROW_BYTES: usize = 2048;

/// `EpdFontFamily::Style` bits.
pub(crate) mod font {
    pub const BOLD: u8 = 1;
    pub const ITALIC: u8 = 2;
    pub const UNDERLINE: u8 = 4;
    pub const STRIKETHROUGH: u8 = 8;
    pub const SUP: u8 = 16;
    pub const SUB: u8 = 32;
    pub const RUBY_CONTINUE: u8 = 64;
    pub const SMALL_CAPS: u8 = 128;
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\r' | b'\n' | b'\t')
}

fn heading_or_block(kind: u16) -> bool {
    kind & (element::HEADING_LEVEL | element::BLOCK) != 0
}

fn heading_level(kind: u16) -> u8 {
    u8::try_from(kind & element::HEADING_LEVEL).unwrap_or(0)
}

pub(crate) fn has(css: &Style, bits: u32) -> bool {
    css.defined.has(bits)
}

pub(crate) fn visible(side: BorderSide) -> bool {
    side.width > 0 && side.style != BorderStyle::None
}

/// C's `atoi`: spaces, a sign, digits.
pub(crate) fn atoi(text: &[u8]) -> i64 {
    let mut rest = text
        .iter()
        .skip_while(|&&b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C))
        .peekable();
    let negative = match rest.peek() {
        Some(b'-') => {
            rest.next();
            true
        }
        Some(b'+') => {
            rest.next();
            false
        }
        _ => false,
    };
    let mut value: i64 = 0;
    for &b in rest.take_while(|b| b.is_ascii_digit()) {
        value = value.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    if negative { -value } else { value }
}

/// The document's events, handed to the chapter with the host.
pub(crate) struct Events<'a, H> {
    pub chapter: &'a mut Chapter,
    pub host: &'a mut H,
}

impl<H: Host> Handler for Events<'_, H> {
    fn start_element(&mut self, tag: &StartTag<'_>) -> Flow {
        self.chapter.start_element(self.host, tag);
        Flow::Continue(())
    }
    fn end_element(&mut self, name: Str<'_>) -> Flow {
        self.chapter.end_element(self.host, name.bytes());
        Flow::Continue(())
    }
    fn text(&mut self, text: &[u8]) -> Flow {
        self.chapter.character_data(self.host, text);
        Flow::Continue(())
    }
    fn skipped_entity(&mut self, name: Str<'_>) -> Flow {
        self.chapter.skipped_entity(self.host, name.bytes());
        Flow::Continue(())
    }
}

/// The chapter and the host as `bembeyaz-html`'s word collector.
struct Collector<'a, H> {
    chapter: &'a mut Chapter,
    host: &'a mut H,
}

impl<H: Host> Words for Collector<'_, H> {
    fn buffered(&mut self) -> usize {
        self.chapter.word_len.min(MAX_WORD + 1)
    }
    fn read(&mut self, out: &mut [u8; MAX_WORD]) -> usize {
        let n = self.chapter.word_len.min(MAX_WORD);
        if let (Some(dst), Some(src)) = (out.get_mut(..n), self.chapter.word.get(..n)) {
            dst.copy_from_slice(src);
        }
        n
    }
    fn set(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(MAX_WORD);
        if let (Some(dst), Some(src)) = (self.chapter.word.get_mut(..n), bytes.get(..n)) {
            dst.copy_from_slice(src);
        }
        self.chapter.word_len = n;
    }
    fn push(&mut self, byte: u8) {
        let n = self.chapter.word_len;
        if n < MAX_WORD
            && let Some(slot) = self.chapter.word.get_mut(n)
        {
            *slot = byte;
            self.chapter.word_len = n + 1;
        }
    }
    fn offset(&mut self) -> u32 {
        self.chapter.word_offset
    }
    fn set_offset(&mut self, offset: u32) {
        self.chapter.word_offset = offset;
    }
    fn set_continues(&mut self, continues: bool) {
        self.chapter.next_word_continues = continues;
    }
    fn flush(&mut self) {
        self.chapter.flush_word(self.host);
    }
    fn suppress_indent(&mut self) {
        if let Some(text) = &mut self.chapter.text {
            text.suppress_first_line_indent();
        }
    }
    fn line_break(&mut self) -> bool {
        let Some(text) = &self.chapter.text else {
            return false;
        };
        let style = text.block.without_top().without_bottom();
        self.chapter.break_text_line(self.host, &style);
        self.chapter.text.is_some()
    }
    fn initial(&mut self) -> InitialLetter {
        self.chapter.drop_cap
    }
    fn set_initial(&mut self, letter: &InitialLetter) {
        self.chapter.drop_cap = *letter;
    }
    fn block_empty(&mut self) -> bool {
        self.chapter.text.as_ref().is_none_or(|t| t.is_empty())
    }
}

impl Chapter {
    /// Replaces the text block with an empty one styled `block`, in the same allocation when
    /// there is one.
    #[inline(never)]
    pub(crate) fn set_text(&mut self, block: Block) {
        let s = &self.settings;
        let text = Text::new(
            s.hyphenation,
            s.focus_reading,
            block,
            s.paragraph_indent_spaces,
            s.reclaim,
        );
        if let Some(current) = &mut self.text {
            **current = text;
        } else {
            self.text = Boxed::try_new(text);
            self.out_of_memory |= self.text.is_none();
        }
    }

    fn make_entry(&self, kind: Kind, css: &Style) -> Option<Entry> {
        inline::entry(kind, css, self.depth)
    }

    fn push_entry(&mut self, entry: Entry) {
        if self.inline_stack.try_reserve(1).is_err() {
            self.out_of_memory = true;
            return;
        }
        self.inline_stack.push(entry);
        self.update_effective();
    }

    pub(crate) fn push_block_text_entry(&mut self, css: &Style) {
        let kind = Kind::Block {
            sized_ancestor: self.inline_stack.iter().any(|e| e.has_font_scale),
            scale: self.block_stack.last().map_or(1.0, |b| b.font_scale),
        };
        if let Some(entry) = self.make_entry(kind, css) {
            self.push_entry(entry);
        }
    }

    fn push_decoration_entry(&mut self, decoration: Decoration, css: &Style) {
        if let Some(entry) = self.make_entry(Kind::Decoration(decoration), css) {
            self.push_entry(entry);
        }
    }

    /// Folds the block's and the open elements' styles into the words' style.
    pub(crate) fn update_effective(&mut self) {
        let block = self.block_stack.last();
        self.effective = inline::fold(
            &self.current_css,
            block.is_some_and(|b| b.direction_defined),
            block.is_some_and(|b| b.is_rtl),
            &self.inline_stack,
        );
        // The empty text block takes the flow direction; inline direction stays for inheritance.
        if let Some(text) = &mut self.text
            && text.is_empty()
        {
            text.block.direction_defined = self.effective.paragraph_direction_defined;
            text.block.is_rtl = self.effective.paragraph_rtl;
        }
    }

    /// The previous block is on its pages: the waiting id lands on the current page.
    pub(crate) fn place_pending_anchor<H: Host>(&mut self, host: &mut H) {
        let Self {
            anchors,
            pager,
            paragraph_index,
            list_item_index,
            out_of_memory,
            ..
        } = self;
        let mut pages = crate::chapter::PagesOf {
            host,
            paragraph: *paragraph_index,
            list_item: *list_item_index,
            out_of_memory,
        };
        if anchors.place_pending(pager, &mut pages).is_err() {
            *pages.out_of_memory = true;
        }
    }

    /// Adds the collected word to the text block, in the style of the open elements.
    pub(crate) fn flush_word<H: Host>(&mut self, host: &mut H) {
        if self.text.is_none() {
            // The block could not be made: the build fails.
            self.word_len = 0;
            self.next_word_continues = false;
            return;
        }
        let preserve = self.effective.preserve_whitespace;
        if preserve && let Some(text) = &mut self.text {
            text.suppress_first_line_indent();
            if text.block.alignment == TextAlign::Justify as u8 {
                text.block.alignment = TextAlign::Left as u8;
            }
        }
        let e = &self.effective;
        let mut style = 0u8;
        if self.bold_until < self.depth || e.bold {
            style |= font::BOLD;
        }
        if self.italic_until < self.depth || e.italic {
            style |= font::ITALIC;
        }
        if e.decoration.0 & Decoration::UNDERLINE.0 != 0 {
            style |= font::UNDERLINE;
        }
        if e.decoration.0 & Decoration::LINE_THROUGH.0 != 0 {
            style |= font::STRIKETHROUGH;
        }
        if e.small_caps {
            style |= font::SMALL_CAPS;
        }
        if e.sup {
            style |= font::SUP;
        } else if e.sub {
            style |= font::SUB;
        }

        if let Some(slot) = self.word.get_mut(self.word_len) {
            *slot = 0;
        }
        let word_bytes = self.word_len;
        if self.table.inside_cell
            && !self.table.row_stacked
            && self.table.row_text_bytes + word_bytes > MAX_ROW_BYTES
        {
            self.fallback_row_to_stacked(host);
        }
        let Some(text) = &mut self.text else {
            return;
        };
        let mut link = 0;
        if self.inside_note_link {
            let href = self.notes.href();
            if !text.link_target_matches(self.note_link_id, href) {
                self.note_link_id = text.add_link_target(href);
            }
            link = self.note_link_id;
        }
        // Inline font-size spans size their words apart from the block.
        let scale = inline::word_scale(&self.inline_stack, text.block.font_scale);
        #[allow(
            clippy::float_cmp,
            reason = "a span's scale differs only when set apart"
        )]
        let slot = if scale > 0.0 && (self.inline_size.valid || scale != text.block.font_scale) {
            text.size_slot_for(scale.clamp(0.5, 3.0))
        } else {
            0
        };
        text.add_word(
            self.word.get(..word_bytes).unwrap_or_default(),
            style,
            self.next_word_continues,
            self.word_offset,
            link,
            slot,
            preserve,
        );
        if self.table.inside_cell && !self.table.row_stacked {
            self.table.row_text_bytes += word_bytes;
            let row_words =
                text.len() + self.table.row_cells.iter().map(|t| t.len()).sum::<usize>();
            if row_words > MAX_ROW_WORDS {
                self.fallback_row_to_stacked(host);
            }
        }
        self.word_len = 0;
        self.next_word_continues = false;
        self.list_item_bullet_only = false;
    }

    /// A `<br/>` or a hard line break.
    pub(crate) fn break_text_line<H: Host>(&mut self, host: &mut H, style: &Block) {
        let has_text = self.text.as_ref().is_some_and(|t| !t.is_empty());
        if !has_text {
            let height = host.line_height(self.settings.font, self.settings.line_compression);
            self.pager.blank_line(height);
        }
        self.start_new_text_block(host, style, false);
        if (has_text || self.effective.preserve_whitespace)
            && let Some(text) = &mut self.text
        {
            text.suppress_first_line_indent();
        }
    }

    /// Starts a text block with `style`, unless the current one is still empty.
    pub(crate) fn start_new_text_block<H: Host>(
        &mut self,
        host: &mut H,
        style: &Block,
        paragraph_end: bool,
    ) {
        self.next_word_continues = false;
        if let Some(text) = &mut self.text {
            if text.is_empty() {
                if paragraph_end {
                    text.reset_first_line_indent();
                }
                // Containers deposit their vertical spacing on the empty block; the first child
                // inherits it.
                text.block = text.block.combined(style, Axis::Vertical);
                self.inline_size = InlineSize::NONE;
                self.place_pending_anchor(host);
                return;
            }
            // A <li>'s bullet alone: a nested block's text stays beside it.
            if self.list_item_bullet_only {
                text.block = text.block.combined(style, Axis::Vertical);
                self.list_item_bullet_only = false;
                self.place_pending_anchor(host);
                return;
            }
            self.make_pages(host, true, paragraph_end);
        }
        self.place_pending_anchor(host);
        self.drop_cap.pending = false;
        self.inline_size = InlineSize::NONE;
        self.set_text(*style);
        self.words_placed = 0;
        self.list_item_bullet_only = false;
    }

    /// Frames an element with a CSS border or shade on every page its content spans.
    fn open_box<H: Host>(&mut self, host: &mut H, own: &mut Block, css: &Style) {
        let shaded = has(css, Defined::SHADED) && css.shaded;
        let bordered = css.borders.iter().any(|&b| visible(b));
        if self.table.depth > 0 || self.pager.boxes_ref().is_full() || (!bordered && !shaded) {
            return;
        }
        if self.word_len > 0 {
            self.flush_word(host);
        }
        if self.text.as_ref().is_some_and(|t| !t.is_empty()) && !self.list_item_bullet_only {
            let style = self
                .block_stack
                .last()
                .copied()
                .unwrap_or(Block::DEFAULT)
                .without_top()
                .without_bottom();
            self.start_new_text_block(host, &style, true);
        }
        let side = |s: BorderSide| bembeyaz_layout::boxes::Side {
            width: s.width,
            style: s.style as u8,
        };
        let sides = css.borders.map(side);
        let parent = self.block_stack.last().copied().unwrap_or(Block::DEFAULT);
        let left = parent.left_inset().wrapping_add(own.margin_left);
        #[allow(clippy::cast_possible_truncation)]
        let right = (i32::from(self.settings.viewport_width)
            - i32::from(parent.right_inset())
            - i32::from(own.margin_right)) as i16;
        let mut padding = [
            own.padding_top,
            own.padding_right,
            own.padding_bottom,
            own.padding_left,
        ];
        if !self
            .pager
            .open_box(self.depth, sides, shaded, left, right, &mut padding)
        {
            return;
        }
        [
            own.padding_top,
            own.padding_right,
            own.padding_bottom,
            own.padding_left,
        ] = padding;
    }

    fn close_box<H: Host>(&mut self, host: &mut H) {
        // An empty element (a rule drawn with borders) gets its own height, with the spacing that
        // still sits on the empty text block.
        let (mut above, mut below) = (0, 0);
        if self.pager.box_empty_height() > 0
            && let Some(text) = &mut self.text
            && text.is_empty()
        {
            let b = &mut text.block;
            above = i32::from(b.margin_top);
            below = i32::from(b.margin_bottom);
            b.margin_top = 0;
            b.padding_top = 0;
            b.margin_bottom = 0;
            b.padding_bottom = 0;
        }
        let (pager, mut pages) = self.pages(host);
        pager.close_box(&mut pages, above, below);
    }

    /// A `::first-letter` rule with an initial letter makes the block's first letter a drop cap.
    fn arm_first_letter<H: Host>(&mut self, host: &H, tag: &[u8], classes: &[u8], id: &[u8]) {
        let Some(rules) = host.rules() else {
            return;
        };
        if !rules.has_first_letter_rules() || self.table.depth > 0 || self.drop_cap.len > 0 {
            return;
        }
        let depth = usize::try_from(self.depth).unwrap_or(0);
        let ancestors = if depth <= MAX_CSS_ANCESTORS {
            self.ancestors.get(..depth).unwrap_or_default()
        } else {
            &[]
        };
        let first_letter = rules.resolve(&Query {
            tag,
            classes,
            id,
            ancestors,
            first_letter: true,
        });
        let lines = inline::drop_cap_lines(&first_letter);
        if lines == 0 {
            return;
        }
        self.drop_cap.pending = true;
        self.drop_cap.lines = lines;
        self.drop_cap.bold = has(&first_letter, Defined::FONT_WEIGHT)
            && first_letter.font_weight == FontWeight::Bold;
    }

    /// A captured letter whose block has no other text is an ordinary word.
    fn cancel_drop_cap_to_word(&mut self) {
        let len = usize::from(self.drop_cap.len).min(MAX_WORD.saturating_sub(self.word_len));
        let start = self.word_len;
        if let (Some(dst), Some(src)) = (
            self.word.get_mut(start..start + len),
            self.drop_cap.text.get(..len),
        ) {
            dst.copy_from_slice(src);
        }
        self.word_len += len;
        self.drop_cap = InitialLetter::NONE;
    }

    pub(crate) fn push_block_style(&mut self, accumulated: &Block) {
        // The break belongs to the element's first block only.
        let entry = Block {
            page_break_before: false,
            ..*accumulated
        };
        if self.block_stack.try_reserve(1).is_err() {
            self.out_of_memory = true;
            return;
        }
        self.block_stack.push(entry);
    }

    pub(crate) fn apply_block_font_scale(&self, block: &mut Block, css: &Style, heading: u8) {
        block.font_scale = inline::block_scale(
            css,
            heading,
            self.block_stack.last().map_or(1.0, |b| b.font_scale),
            &self.inline_stack,
        );
        block.font_scale_defined = true;
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn start_element<H: Host>(&mut self, host: &mut H, tag: &impl Tag) {
        let tag_name = tag.name();
        let kind = element::kind(tag_name);
        let named = kind & element::NAME;
        if tag_name.eq_ignore_ascii_case(b"body") {
            // Case-insensitive, as ProgressMapper matches it.
            self.inside_body = true;
        }
        if self.inside_body && (self.hidden_depth > 0 || kind & element::HIDDEN != 0) {
            self.hidden_depth = self.hidden_depth.saturating_add(1);
        }
        if self.skip_until < self.depth {
            self.depth += 1;
            return;
        }
        // bembeyaz-kosync counts <p> and <li> the same way for KOReader positions.
        if named == name::P {
            self.paragraph_index = self.paragraph_index.wrapping_add(1);
        }
        if named == name::LI {
            self.list_item_index = self.list_item_index.wrapping_add(1);
        }

        let mut classes: &[u8] = &[];
        let mut style_attr: &[u8] = &[];
        let mut dir: &[u8] = &[];
        let mut id: &[u8] = &[];
        let mut hidden = false;
        for (attr, value) in tag.attributes() {
            match attr {
                b"class" => classes = value,
                b"style" => style_attr = value,
                b"id" => {
                    // The id waits until the previous block is on its pages; ids on spans are
                    // dropped unless the table of contents names them.
                    id = value;
                    let navigable = named != name::SPAN;
                    let Self {
                        anchors,
                        pager,
                        paragraph_index,
                        list_item_index,
                        out_of_memory,
                        ..
                    } = self;
                    let mut pages = crate::chapter::PagesOf {
                        host: &mut *host,
                        paragraph: *paragraph_index,
                        list_item: *list_item_index,
                        out_of_memory,
                    };
                    if anchors.open(pager, &mut pages, id, navigable).is_err() {
                        *pages.out_of_memory = true;
                    }
                }
                b"dir" => dir = value,
                b"hidden" => hidden = true,
                _ => {}
            }
        }
        let attribute =
            |wanted: &[u8]| tag.attributes().find(|(n, _)| *n == wanted).map(|(_, v)| v);

        let mut css = Style::DEFAULT;
        if let Some(rules) = host.rules() {
            let depth = usize::try_from(self.depth).unwrap_or(0);
            let ancestors = if depth <= MAX_CSS_ANCESTORS {
                self.ancestors.get(..depth).unwrap_or_default()
            } else {
                &[]
            };
            css = rules.resolve(&Query {
                tag: tag_name,
                classes,
                id,
                ancestors,
                first_letter: false,
            });
            if let Some(slot) = self.ancestors.get_mut(depth) {
                *slot = Ancestor::new(tag_name, classes, id);
            }
            if !style_attr.is_empty() {
                css.apply(&bembeyaz_css::parse_inline(style_attr));
            }
        }
        if named == name::PRE && !has(&css, Defined::WHITE_SPACE) {
            css.preserve_whitespace = true;
            css.defined.set(Defined::WHITE_SPACE);
        }
        // The hidden attribute overrides CSS display.
        if hidden {
            css.display = Display::None;
            css.defined.set(Defined::DISPLAY);
        }
        // The dir attribute overrides CSS direction.
        if dir.eq_ignore_ascii_case(b"rtl") {
            css.direction = Direction::Rtl;
            css.defined.set(Defined::DIRECTION);
        } else if dir.eq_ignore_ascii_case(b"ltr") {
            css.direction = Direction::Ltr;
            css.defined.set(Defined::DIRECTION);
        }
        // Direction is inherited.
        if !has(&css, Defined::DIRECTION) && self.effective.direction_defined {
            css.direction = self.effective.direction;
            css.defined.set(Defined::DIRECTION);
        }
        if has(&css, Defined::DISPLAY) && css.display == Display::None {
            self.skip_until = self.depth;
            self.depth += 1;
            return;
        }
        let rtl = has(&css, Defined::DIRECTION) && css.direction == Direction::Rtl;

        if named == name::TABLE {
            self.open_table(host, tag, &css, rtl, attribute(b"border"));
            self.depth += 1;
            return;
        }
        if self.table.depth == 1 && named == name::TR {
            self.open_row(host, &css, rtl);
            self.depth += 1;
            return;
        }
        if self.table.depth == 1 && (named == name::TD || named == name::TH) {
            self.open_cell(
                host,
                kind,
                named,
                &css,
                attribute(b"colspan"),
                attribute(b"rowspan"),
            );
            self.depth += 1;
            return;
        }
        if self.table.depth >= 1
            && self.table.inside_cell
            && (heading_or_block(kind) || named == name::HR || kind & element::IMAGE != 0)
        {
            // Rich cells use full-width flow; a grid keeps plain inline content only.
            if self.word_len > 0 {
                self.flush_word(host);
            }
            self.fallback_row_to_stacked(host);
        }

        if kind & element::IMAGE != 0 {
            self.open_image(host, &css, tag);
            return;
        }

        if named == name::RUBY {
            // A base right after text continues the same visual word.
            if self.word_len > 0 {
                self.flush_word(host);
                self.next_word_continues = true;
            }
            self.in_ruby = true;
            self.ruby_start = self
                .text
                .as_ref()
                .map_or(0, |t| i32::try_from(t.len()).unwrap_or(i32::MAX));
            self.ruby_text.clear();
            self.depth += 1;
            return;
        }
        if named == name::RT {
            if self.word_len > 0 {
                self.flush_word(host);
            }
            self.collecting_ruby = true;
            self.depth += 1;
            return;
        }
        if kind & element::HIDDEN != 0 {
            self.skip_until = self.depth;
            self.depth += 1;
            return;
        }
        // Page-break markers.
        for (attr, value) in tag.attributes() {
            if (attr == b"role" && value == b"doc-pagebreak")
                || (attr == b"epub:type" && value == b"pagebreak")
            {
                self.skip_until = self.depth;
                self.depth += 1;
                return;
            }
        }

        if named == name::A {
            let href = attribute(b"href");
            let internal = href
                .is_some_and(|h| element::is_internal_link(h) && !h.starts_with(b"javascript:"));
            if internal {
                let href = href.unwrap_or_default();
                // Note indices are block-relative, so linked rows use ordinary flow.
                if self.table.depth >= 1 && self.table.inside_cell && !self.table.row_stacked {
                    self.fallback_row_to_stacked(host);
                }
                if self.word_len > 0 {
                    self.flush_word(host);
                    self.next_word_continues = true;
                }
                self.inside_note_link = true;
                self.note_link_depth = self.depth;
                self.note_link_id = self.text.as_mut().map_or(0, |t| t.add_link_target(href));
                // The block keeps the target only when it fits.
                let target: &[u8] = if self.note_link_id != 0 { href } else { b"" };
                self.out_of_memory |= self.notes.open(target).is_err();
                if let Some(entry) = self.make_entry(Kind::Link, &css) {
                    self.push_entry(entry);
                }
                self.depth += 1;
                return;
            }
        }

        #[allow(clippy::cast_precision_loss)]
        let em = host.ascender(self.settings.font) as f32;
        let viewport = self.settings.viewport_width;
        let user_aligned = block::from_css(&css, em, self.settings.paragraph_alignment, viewport);

        if named == name::HR {
            let mut rule = block::from_css(&css, em, TextAlign::Left as u8, viewport);
            if !self.settings.embedded_style {
                rule.margin_left = 0;
                rule.margin_right = 0;
                rule.margin_top = 0;
                rule.margin_bottom = 0;
                rule.padding_left = 0;
                rule.padding_right = 0;
                rule.padding_top = 0;
                rule.padding_bottom = 0;
                rule.text_indent_defined = false;
                rule.text_indent = 0;
            }
            self.emit_rule(host, &rule);
            self.depth += 1;
            return;
        }

        if kind & element::HEADING_LEVEL != 0 {
            if self.word_len > 0 {
                self.flush_word(host);
            }
            self.current_css = css;
            let mut heading = block::from_css(&css, em, TextAlign::Center as u8, viewport);
            heading.text_align_defined = true;
            if self.settings.embedded_style && has(&css, Defined::TEXT_ALIGN) {
                heading.alignment = css.text_align as u8;
            }
            self.apply_block_font_scale(&mut heading, &css, heading_level(kind));
            heading.keep_with_next = true;
            self.open_box(host, &mut heading, &css);
            let parent = self.block_stack.last().copied().unwrap_or(Block::DEFAULT);
            let accumulated = parent.combined(&heading, Axis::Horizontal);
            self.push_block_style(&accumulated);
            self.start_new_text_block(host, &accumulated.without_bottom(), true);
            self.push_block_text_entry(&css);
            self.arm_first_letter(host, tag_name, classes, id);
            self.bold_until = self.bold_until.min(self.depth);
            self.update_effective();
        } else if kind & element::BLOCK != 0 {
            if named == name::BR {
                if self.word_len > 0 {
                    self.flush_word(host);
                }
                let style = self
                    .block_stack
                    .last()
                    .copied()
                    .unwrap_or(Block::DEFAULT)
                    .without_top()
                    .without_bottom();
                self.break_text_line(host, &style);
            } else {
                if self.word_len > 0 {
                    self.flush_word(host);
                }
                self.current_css = css;
                let mut style = user_aligned;
                self.apply_block_font_scale(&mut style, &css, heading_level(kind));
                self.open_box(host, &mut style, &css);
                let parent = self.block_stack.last().copied().unwrap_or(Block::DEFAULT);
                let accumulated = parent.combined(&style, Axis::Horizontal);
                self.push_block_style(&accumulated);
                self.start_new_text_block(host, &accumulated.without_bottom(), true);
                if self.text.is_none() {
                    return;
                }
                self.push_block_text_entry(&css);
                self.update_effective();
                if named != name::LI {
                    self.arm_first_letter(host, tag_name, classes, id);
                }
                if named == name::LI {
                    // The innermost list decides the marker; none leaves the block empty.
                    let mut marker = [0u8; MAX_MARKER];
                    let len = list::marker(self.lists.last_mut().map(|l| &mut l.list), &mut marker);
                    if len > 0 {
                        let offset = self.visible_offset;
                        if let Some(text) = &mut self.text {
                            text.add_word(
                                marker.get(..len).unwrap_or_default(),
                                0,
                                false,
                                offset,
                                0,
                                0,
                                false,
                            );
                        }
                        self.list_item_bullet_only = true;
                    }
                } else if named == name::UL || named == name::OL {
                    let list = List {
                        ordered: named == name::OL,
                        unmarked: has(&css, Defined::LIST_STYLE)
                            && css.list_style == ListStyle::None,
                        counter: 0,
                    };
                    if self.lists.try_reserve(1).is_err() {
                        self.out_of_memory = true;
                    } else {
                        self.lists.push(ListContext {
                            list,
                            depth: self.depth,
                        });
                    }
                }
            }
        } else if kind & element::UNDERLINE != 0 {
            if self.word_len > 0 {
                self.flush_word(host);
                self.next_word_continues = true;
            }
            self.push_decoration_entry(Decoration::UNDERLINE, &css);
        } else if kind & element::LINE_THROUGH != 0 {
            if self.word_len > 0 {
                self.flush_word(host);
                self.next_word_continues = true;
            }
            self.push_decoration_entry(Decoration::LINE_THROUGH, &css);
        } else if kind & element::BOLD != 0 {
            if self.word_len > 0 {
                self.flush_word(host);
                self.next_word_continues = true;
            }
            self.bold_until = self.bold_until.min(self.depth);
            if let Some(entry) = self.make_entry(Kind::Bold, &css) {
                self.push_entry(entry);
            }
        } else if kind & element::ITALIC != 0 {
            if self.word_len > 0 {
                self.flush_word(host);
                self.next_word_continues = true;
            }
            self.italic_until = self.italic_until.min(self.depth);
            if let Some(entry) = self.make_entry(Kind::Italic, &css) {
                self.push_entry(entry);
            }
        } else if named == name::SUP || named == name::SUB {
            if self.word_len > 0 {
                self.flush_word(host);
                self.next_word_continues = true;
            }
            let kind = if named == name::SUP {
                Kind::Sup
            } else {
                Kind::Sub
            };
            if let Some(entry) = self.make_entry(kind, &css) {
                self.push_entry(entry);
            }
        } else if named == name::SPAN || !heading_or_block(kind) {
            let empty_block = self.text.as_ref().is_some_and(|t| t.is_empty());
            if has(&css, Defined::length(LengthField::FontSize))
                && self.table.depth == 0
                && self.word_len == 0
                && empty_block
                && (!self.inline_size.valid || self.inline_size.open)
            {
                let base = if self.inline_size.valid {
                    self.inline_size.scale
                } else {
                    self.text.as_ref().map_or(1.0, |t| t.block.font_scale)
                };
                self.inline_size.scale = inline::span_scale(&css, base);
                if !self.inline_size.valid {
                    self.inline_size.depth = self.depth;
                }
                self.inline_size.valid = true;
                self.inline_size.open = true;
            }
            // A floated or oversized span opening a paragraph is a hand-made drop cap.
            if self.table.depth == 0
                && self.drop_cap.len == 0
                && self.drop_cap.span_depth < 0
                && self.word_len == 0
                && empty_block
            {
                let lines = inline::drop_cap_lines(&css);
                if lines > 0 {
                    self.drop_cap.span_depth = self.depth;
                    self.drop_cap.lines = lines;
                    self.drop_cap.pending = false;
                    self.drop_cap.bold = self.effective.bold
                        || (has(&css, Defined::FONT_WEIGHT) && css.font_weight == FontWeight::Bold);
                }
            }
            let kind = Kind::Inline {
                sets_paragraph: named == name::HTML || named == name::BODY,
                in_table: self.table.depth >= 1,
                tag_scale: inline::tag_scale(tag_name).unwrap_or(0.0),
            };
            if let Some(entry) = self.make_entry(kind, &css) {
                if self.word_len > 0 {
                    self.flush_word(host);
                    self.next_word_continues = true;
                }
                self.push_entry(entry);
            }
        }
        self.depth += 1;
    }

    /// An `<img>` or `<image>`: placed on the page when it can be sized, its alt text otherwise.
    fn open_image<H: Host>(&mut self, host: &mut H, css: &Style, tag: &impl Tag) {
        let mut src: &[u8] = &[];
        let mut alt: &[u8] = &[];
        for (attr, value) in tag.attributes() {
            if attr == b"src" || (src.is_empty() && (attr == b"href" || attr == b"xlink:href")) {
                src = value;
            } else if attr == b"alt" {
                alt = value;
            }
        }
        if let Some(hash) = src.iter().position(|&b| b == b'#') {
            src = src.get(..hash).unwrap_or_default();
        }
        // 0 shows images, 1 their alt text only, 2 neither.
        if self.settings.image_rendering == 2 {
            self.skip_until = self.depth;
            self.depth += 1;
            return;
        }
        if !src.is_empty() && self.settings.image_rendering != 1 && self.place_image(host, css, src)
        {
            self.depth += 1;
            return;
        }
        if !alt.is_empty() {
            let parent = self.block_stack.last().copied().unwrap_or(Block::DEFAULT);
            let centered = Block {
                text_align_defined: true,
                alignment: TextAlign::Center as u8,
                ..Block::DEFAULT
            };
            self.start_new_text_block(
                host,
                &parent
                    .combined(&centered, Axis::Horizontal)
                    .without_bottom(),
                true,
            );
            self.italic_until = self.italic_until.min(self.depth);
            self.depth += 1;
            let mut label = alloc::vec::Vec::new();
            if label.try_reserve(alt.len() + 9).is_err() {
                self.out_of_memory = true;
            } else {
                label.extend_from_slice(b"[Image: ");
                label.extend_from_slice(alt);
                label.push(b']');
                self.synthetic_text = true;
                self.character_data(host, &label);
                self.synthetic_text = false;
            }
            // Its children are skipped (depth is past the image already).
            self.skip_until = self.depth - 1;
            return;
        }
        self.skip_until = self.depth;
        self.depth += 1;
    }

    /// Sizes and places an image; false when it cannot be sized (the alt text stands in). An image
    /// sized but not placed for lack of memory is dropped and still counts as an open element (the
    /// C++ parser lost track of the depth there).
    fn place_image<H: Host>(&mut self, host: &mut H, css: &Style, src: &[u8]) -> bool {
        // The path from the chapter's folder.
        let mut path = alloc::vec::Vec::new();
        if path
            .try_reserve(self.content_base.len() + src.len())
            .is_err()
        {
            self.out_of_memory = true;
            return false;
        }
        path.extend_from_slice(&self.content_base);
        path.extend_from_slice(src);
        let len = bembeyaz_path::decode_uri_escapes(&mut path);
        path.truncate(len);
        let len = bembeyaz_path::normalise(&mut path);
        path.truncate(len);
        let Image::Sized { width, height } = host.image(&path) else {
            return false;
        };
        // Percentages resolve against the container, not the whole page.
        let viewport = i32::from(self.settings.viewport_width);
        let mut container = viewport;
        if let Some(text) = &self.text {
            let inset = i32::from(text.block.total_horizontal_inset());
            if inset > 0 && inset < viewport {
                container = viewport - inset;
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let em = host.ascender(self.settings.font) as f32;
        let (display_width, display_height) = bembeyaz_css::image::size(
            css,
            (width, height),
            em,
            container,
            self.settings.viewport_height,
        );

        // Text before the image goes first.
        if self.word_len > 0 {
            self.flush_word(host);
        }
        if let Some(text) = &self.text
            && !text.is_empty()
        {
            let style = text.block;
            self.start_new_text_block(host, &style, true);
        }
        // The container's top margin sits on the empty block; its bottom one is still stacked.
        let (mut margin_top, mut margin_bottom) = (0i16, 0i16);
        if let Some(text) = &self.text
            && text.is_empty()
        {
            margin_top = text.block.top_inset();
            if self.block_stack.len() > 1 {
                margin_bottom = self.block_stack.last().map_or(0, BlockExt::bottom_inset);
            }
        }
        self.apply_pending_page_break(host);
        let offset = self.visible_offset;
        let y = {
            let (pager, mut pages) = self.pages(host);
            pager.image_top(
                &mut pages,
                margin_top,
                display_height,
                margin_bottom,
                offset,
            )
        };
        let Some(y) = y else {
            return true;
        };
        if !self.pager.has_page() {
            return true;
        }
        let x = (viewport - display_width) / 2;
        if !host.push_image(display_width, display_height, x, y) {
            return true;
        }
        self.pager
            .image_added(display_height, margin_bottom, offset);
        // The image took the empty block's vertical spacing.
        if let Some(text) = &mut self.text
            && text.is_empty()
        {
            text.block = Block {
                alignment: root_alignment(self.settings.paragraph_alignment),
                ..Block::DEFAULT
            };
        }
        true
    }

    pub fn character_data<H: Host>(&mut self, host: &mut H, s: &[u8]) {
        let counted = self.inside_body && self.hidden_depth == 0 && !self.synthetic_text;
        let start = self.visible_offset;
        if counted {
            let n = u32::try_from(bembeyaz_utf8::codepoints(s).count()).unwrap_or(u32::MAX);
            self.visible_offset = self.visible_offset.wrapping_add(n);
        }
        // Nested tables' text needs an enclosing cell.
        if self.table.depth > 1 && !self.table.inside_cell {
            return;
        }
        if self.skip_until < self.depth {
            return;
        }
        if self.collecting_ruby {
            if self.ruby_text.try_reserve(s.len()).is_err() {
                self.out_of_memory = true;
            } else {
                self.ruby_text.extend_from_slice(s);
            }
            return;
        }
        if self.table.depth == 1 && !self.table.inside_cell && s.iter().all(|&b| is_space(b)) {
            return;
        }
        // Flow storage for text (a caption) after a row.
        if self.text.is_none() {
            let style = self
                .block_stack
                .last()
                .map_or(Block::DEFAULT, BlockExt::without_bottom);
            self.set_text(style);
            self.words_placed = 0;
        }
        // A note reference's text is its number ("[1]" is 1).
        if self.inside_note_link {
            self.notes.text(s);
        }
        if self.inline_size.valid && !self.inline_size.open && s.iter().any(|&b| !is_space(b)) {
            self.inline_size.valid = false;
        }
        let piece = Piece {
            counted,
            start,
            preserve: self.effective.preserve_whitespace,
            footnote_link: self.inside_note_link,
        };
        if words::scan(
            s,
            piece,
            &mut Collector {
                chapter: self,
                host,
            },
        ) == Scan::Stopped
        {
            return;
        }
        let Some(text) = &self.text else {
            return;
        };
        // Keep the word count bounded; the last line stays to keep the paragraph's flow.
        let limit = if self.settings.embedded_style {
            SOFT_FLUSH_WORDS_WITH_CSS
        } else {
            SOFT_FLUSH_WORDS
        };
        if text.len() > limit && !self.in_ruby {
            self.make_pages(host, false, true);
        }
    }

    /// An entity the document does not declare: an HTML one's text, the others as written.
    pub(crate) fn skipped_entity<H: Host>(&mut self, host: &mut H, name: &[u8]) {
        if let Some(value) = bembeyaz_html::entity(name) {
            self.character_data(host, value.text());
            return;
        }
        if name.len() + 2 <= 48 {
            let mut buf = [0u8; 48];
            if let Some(dst) = buf.get_mut(1..=name.len()) {
                dst.copy_from_slice(name);
            }
            buf[0] = b'&';
            if let Some(end) = buf.get_mut(name.len() + 1) {
                *end = b';';
            }
            self.character_data(host, buf.get(..name.len() + 2).unwrap_or_default());
        } else {
            self.character_data(host, b"&");
            self.character_data(host, name);
            self.character_data(host, b";");
        }
    }

    #[allow(clippy::too_many_lines)]
    pub fn end_element<H: Host>(&mut self, host: &mut H, tag_name: &[u8]) {
        let kind = element::kind(tag_name);
        let named = kind & element::NAME;
        if self.hidden_depth > 0 {
            self.hidden_depth -= 1;
        }
        if named == name::RT {
            self.collecting_ruby = false;
            if self.in_ruby
                && let Some(text) = &mut self.text
            {
                let words = i32::try_from(text.len()).unwrap_or(i32::MAX);
                let base = words - self.ruby_start;
                let len = words::collapse_space(&mut self.ruby_text);
                self.ruby_text.truncate(len);
                if !self.ruby_text.is_empty() {
                    let start = usize::try_from(self.ruby_start).unwrap_or(0);
                    if base > 0 {
                        text.paragraph.set_ruby_group(
                            start,
                            usize::try_from(base).unwrap_or(0),
                            &self.ruby_text,
                        );
                        self.ruby_start = words;
                    } else if self.ruby_start > 0 {
                        let mut leader = self.ruby_start - 1;
                        while leader >= 0
                            && text
                                .paragraph
                                .word_style(usize::try_from(leader).unwrap_or(0))
                                & font::RUBY_CONTINUE
                                != 0
                        {
                            leader -= 1;
                        }
                        if leader >= 0 {
                            let at = usize::try_from(leader).unwrap_or(0);
                            let mut joined = alloc::vec::Vec::new();
                            let previous = text.paragraph.ruby_at(at);
                            if joined
                                .try_reserve(previous.len() + self.ruby_text.len())
                                .is_ok()
                            {
                                joined.extend_from_slice(previous);
                                joined.extend_from_slice(&self.ruby_text);
                                text.paragraph.set_ruby(at, &joined);
                            } else {
                                self.out_of_memory = true;
                            }
                        }
                    }
                }
            }
            self.ruby_text.clear();
            // The next base joins the previous one with no space.
            if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
                self.next_word_continues = true;
            }
            self.depth -= 1;
            return;
        }
        if named == name::RUBY && self.in_ruby {
            self.in_ruby = false;
            self.ruby_start = -1;
            self.ruby_text.clear();
            if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
                self.next_word_continues = true;
            }
            self.depth -= 1;
            return;
        }
        // Style that changes once depth drops: flush the word with the current style first.
        let pops = self
            .inline_stack
            .last()
            .is_some_and(|e| e.depth == self.depth - 1);
        let clears_bold = self.bold_until == self.depth - 1;
        let clears_italic = self.italic_until == self.depth - 1;
        let style_changes = pops || clears_bold || clears_italic;
        let block_tag = heading_or_block(kind);
        let table_tag = kind & element::TABLE != 0;
        let skipped = self.depth > self.skip_until;

        if !skipped && self.table.depth > 1 && named == name::TABLE {
            if self.word_len > 0 {
                self.flush_word(host);
            }
            self.next_word_continues = false;
            self.table.depth -= 1;
            self.depth -= 1;
            return;
        }
        if self.word_len > 0 {
            let inline_tag =
                !block_tag && !table_tag && kind & element::IMAGE == 0 && self.depth != 1;
            let flush = style_changes
                || block_tag
                || table_tag
                || self.depth == 1
                || kind
                    & (element::BOLD
                        | element::ITALIC
                        | element::UNDERLINE
                        | element::LINE_THROUGH
                        | element::IMAGE)
                    != 0;
            if flush {
                self.flush_word(host);
                if inline_tag {
                    self.next_word_continues = true;
                }
            }
        }
        // A captured letter without body text is ordinary block content.
        if block_tag
            && named != name::BR
            && !skipped
            && self.text.as_ref().is_some_and(|t| t.is_empty())
            && self.drop_cap.len > 0
        {
            self.cancel_drop_cap_to_word();
            self.flush_word(host);
        }
        self.depth -= 1;
        if self.drop_cap.span_depth == self.depth {
            self.drop_cap.span_depth = -1;
        }
        if self.inline_size.valid && self.inline_size.depth == self.depth {
            self.inline_size.open = false;
        }
        if self.inside_note_link && self.depth == self.note_link_depth {
            let words = self
                .text
                .as_ref()
                .map_or(0, |t| i32::try_from(t.len()).unwrap_or(i32::MAX));
            self.out_of_memory |= self.notes.close(self.words_placed + words).is_err();
            self.inside_note_link = false;
            self.note_link_id = 0;
        }
        if self.skip_until == self.depth {
            self.skip_until = i32::MAX;
        }
        if !skipped && self.table.depth == 1 && (named == name::TD || named == name::TH) {
            self.close_cell(host);
            self.next_word_continues = false;
        }
        if !skipped && self.table.depth == 1 && named == name::TR {
            self.finish_row(host);
            self.next_word_continues = false;
        }
        if !skipped && self.table.depth == 1 && named == name::TABLE {
            self.close_table(host);
        }
        if self.bold_until == self.depth {
            self.bold_until = i32::MAX;
        }
        if self.italic_until == self.depth {
            self.italic_until = i32::MAX;
        }
        if self
            .inline_stack
            .last()
            .is_some_and(|e| e.depth == self.depth)
        {
            self.inline_stack.pop();
            self.update_effective();
        }
        if block_tag && !skipped {
            self.current_css = Style::DEFAULT;
            self.update_effective();
            // <br/> is not a container.
            if named != name::BR && self.block_stack.len() > 1 {
                // The closing element's bottom spacing goes after its content.
                let closing = self.block_stack.last().copied().unwrap_or(Block::DEFAULT);
                if let Some(text) = &mut self.text {
                    text.block = text.block.with_bottom_of(&closing);
                }
                let break_after = closing.page_break_after;
                self.block_stack.pop();
                let parent = self.block_stack.last().copied().unwrap_or(Block::DEFAULT);
                self.start_new_text_block(host, &parent.without_top().without_bottom(), true);
                self.update_effective();
                if self.pager.boxes_ref().innermost_is(self.depth) {
                    self.close_box(host);
                }
                // After the element's own text was laid out above.
                if break_after {
                    self.pending_page_break = true;
                }
            }
            // An item's bullet that never got text does not merge into the next sibling.
            if named == name::LI {
                self.list_item_bullet_only = false;
            }
        }
        // A display:none list never pushed a context, so the depth must match.
        if (named == name::UL || named == name::OL)
            && self.lists.last().is_some_and(|l| l.depth == self.depth)
        {
            self.lists.pop();
        }
        if named == name::BODY {
            self.inside_body = false;
        }
        if named == name::HTML {
            self.html_ended = true;
        }
    }
}
