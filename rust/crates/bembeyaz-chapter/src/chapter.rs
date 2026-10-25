//! The chapter parser (lib/Epub's `ChapterHtmlSlimParser`): it reads a chapter's XHTML, builds
//! its paragraphs with their CSS and lays them out onto pages. `flow` handles the document's
//! events, `table` its grid tables, `place` puts what is laid out on the pages.

use crate::anchors::{Anchors, Full};
use crate::boxed::Boxed;
use crate::footnotes::Footnotes;
use crate::host::{Host, LineHandle};
use crate::text::Text;
use alloc::vec::Vec;
use bembeyaz_css::block::Block;
use bembeyaz_css::inline::{Effective, Entry};
use bembeyaz_css::{Ancestor, BorderSide, Style, TextAlign};
use bembeyaz_html::list::List;
use bembeyaz_html::words::{InitialLetter, MAX_WORD};
use bembeyaz_hyphenation::Language;
use bembeyaz_layout::Reclaim;
use bembeyaz_layout::boxes::Segment;
use bembeyaz_layout::pager::{Pager, Pages};
use bembeyaz_layout::table::MAX_COLUMNS;
use bembeyaz_xml::Parser;

/// Open elements kept for descendant and child CSS selectors; deeper ones match without them.
pub(crate) const MAX_CSS_ANCESTORS: usize = 16;
/// The chapter is read this many bytes at a time.
pub const PARSE_BUFFER: usize = 1024;
/// The XML parser's limit for one token, the open elements and the DTD.
const XML_LIMIT: usize = 4 * 1024 * 1024;

/// How the chapter is laid out.
#[derive(Clone, Copy, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct Settings {
    pub font: i32,
    pub line_compression: f32,
    pub extra_paragraph_spacing: bool,
    /// A `TextAlign`; `None` keeps the book's.
    pub paragraph_alignment: u8,
    pub viewport_width: u16,
    pub viewport_height: u16,
    pub hyphenation: bool,
    pub focus_reading: bool,
    pub embedded_style: bool,
    /// 0 shows images, 1 shows their alt text, 2 neither.
    pub image_rendering: u8,
    pub paragraph_indent_spaces: u8,
    pub character_spacing: i8,
    pub word_spacing_percent: u8,
    pub language: Option<Language>,
    pub reclaim: Option<Reclaim>,
}

/// A font-size span that opens a block and holds all of its text sizes the whole block, e.g.
/// `<p><span class="big">Chapter One</span></p>`. Text after the span cancels it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct InlineSize {
    pub scale: f32,
    /// The outermost sizing span.
    pub depth: i32,
    pub valid: bool,
    pub open: bool,
}

impl InlineSize {
    pub const NONE: Self = Self {
        scale: 1.0,
        depth: -1,
        valid: false,
        open: false,
    };
}

/// An open `<ul>` or `<ol>`, at the depth it opened.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ListContext {
    pub list: List,
    pub depth: i32,
}

/// A grid table's state; one row is buffered at a time.
pub(crate) struct Table {
    pub depth: i32,
    pub inside_cell: bool,
    pub row_stacked: bool,
    pub row_rtl: bool,
    pub rows_spanned_remaining: u16,
    pub row_text_bytes: usize,
    pub(crate) row_cells: Vec<Boxed<Text>>,
    pub cell_lines: [Vec<crate::host::LineInfo>; MAX_COLUMNS],
    pub line_offsets: Vec<u32>,
    pub border: BorderSide,
    /// Column widths planned from the table's markup; 0 columns: equal widths.
    pub column_widths: [u16; MAX_COLUMNS],
    pub column_count: u8,
}

pub(crate) const NO_BORDER: BorderSide = BorderSide {
    width: 0,
    style: bembeyaz_css::BorderStyle::None,
};

/// A cell line already placed (or never kept).
pub(crate) const NO_LINE: LineHandle = LineHandle::MAX;

/// How a parse step ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    More,
    Done,
    Error,
}

/// The chapter parser.
#[allow(clippy::struct_excessive_bools, reason = "independent flow flags")]
pub struct Chapter {
    pub(crate) settings: Settings,
    /// `contentBase`: the chapter's folder in the book, for image paths.
    pub(crate) content_base: Vec<u8>,
    pub(crate) xml: Option<Boxed<Parser>>,
    /// The chapter's bytes being parsed, `PARSE_BUFFER` of them at a time.
    read_buf: Vec<u8>,
    pub(crate) html_ended: bool,

    pub(crate) depth: i32,
    pub(crate) skip_until: i32,
    pub(crate) bold_until: i32,
    pub(crate) italic_until: i32,
    /// The word being collected, NUL after it.
    pub(crate) word: [u8; MAX_WORD + 1],
    pub(crate) word_len: usize,
    pub(crate) word_offset: u32,
    pub(crate) next_word_continues: bool,
    pub(crate) text: Option<Boxed<Text>>,

    pub(crate) in_ruby: bool,
    pub(crate) ruby_start: i32,
    pub(crate) collecting_ruby: bool,
    pub(crate) ruby_text: Vec<u8>,

    pub(crate) inline_stack: Vec<Entry>,
    pub(crate) block_stack: Vec<Block>,
    pub(crate) current_css: Style,
    pub(crate) ancestors: [Ancestor; MAX_CSS_ANCESTORS],
    pub(crate) effective: Effective,
    pub(crate) pending_page_break: bool,

    pub(crate) drop_cap: InitialLetter,
    /// A paragraph is being laid out, and whether it has a drop cap.
    pub(crate) laying_out_drop_cap: bool,

    pub(crate) table: Table,
    pub(crate) list_item_bullet_only: bool,
    pub(crate) inline_size: InlineSize,
    pub(crate) lists: Vec<ListContext>,

    pub(crate) pager: Pager,
    pub(crate) anchors: Anchors,
    pub(crate) notes: Footnotes,
    pub(crate) paragraph_index: u16,
    pub(crate) list_item_index: u16,
    /// Codepoints of visible `<body>` text so far.
    pub(crate) visible_offset: u32,
    pub(crate) inside_body: bool,
    pub(crate) synthetic_text: bool,
    pub(crate) hidden_depth: u16,

    pub(crate) inside_note_link: bool,
    pub(crate) note_link_depth: i32,
    pub(crate) note_link_id: u8,
    pub(crate) words_placed: i32,
    /// Something could not be stored: the build fails rather than lose text.
    pub(crate) out_of_memory: bool,
}

/// The host as the pager's pages, with what each page carries besides its elements.
pub(crate) struct PagesOf<'a, H> {
    pub host: &'a mut H,
    pub paragraph: u16,
    pub list_item: u16,
    pub out_of_memory: &'a mut bool,
}

impl<H: Host> Pages for PagesOf<'_, H> {
    fn new_page(&mut self) -> bool {
        let made = self.host.new_page();
        *self.out_of_memory |= !made;
        made
    }
    fn split(&mut self, lines: u8, top: i16) -> bool {
        let made = self.host.split(lines, top);
        *self.out_of_memory |= !made;
        made
    }
    fn emit(&mut self, visible_offset: u32) {
        self.host
            .emit(visible_offset, self.paragraph, self.list_item);
    }
    fn add_box(&mut self, segment: &Segment) {
        *self.out_of_memory |= !self.host.add_box(segment);
    }
}

pub(crate) fn root_alignment(paragraph_alignment: u8) -> u8 {
    if paragraph_alignment == TextAlign::None as u8 {
        TextAlign::Justify as u8
    } else {
        paragraph_alignment
    }
}

impl Chapter {
    #[must_use]
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            content_base: Vec::new(),
            xml: None,
            read_buf: Vec::new(),
            html_ended: false,
            depth: 0,
            skip_until: i32::MAX,
            bold_until: i32::MAX,
            italic_until: i32::MAX,
            word: [0; MAX_WORD + 1],
            word_len: 0,
            word_offset: 0,
            next_word_continues: false,
            text: None,
            in_ruby: false,
            ruby_start: -1,
            collecting_ruby: false,
            ruby_text: Vec::new(),
            inline_stack: Vec::new(),
            block_stack: Vec::new(),
            current_css: Style::DEFAULT,
            ancestors: [Ancestor::default(); MAX_CSS_ANCESTORS],
            effective: Effective {
                bold: false,
                italic: false,
                decoration: bembeyaz_css::Decoration::NONE,
                small_caps: false,
                preserve_whitespace: false,
                direction_defined: false,
                direction: bembeyaz_css::Direction::Ltr,
                text_align_defined: false,
                text_align: TextAlign::Left,
                sup: false,
                sub: false,
                paragraph_direction_defined: false,
                paragraph_rtl: false,
            },
            pending_page_break: false,
            drop_cap: InitialLetter::NONE,
            laying_out_drop_cap: false,
            table: Table {
                depth: 0,
                inside_cell: false,
                row_stacked: false,
                row_rtl: false,
                rows_spanned_remaining: 0,
                row_text_bytes: 0,
                row_cells: Vec::new(),
                cell_lines: Default::default(),
                line_offsets: Vec::new(),
                border: NO_BORDER,
                column_widths: [0; MAX_COLUMNS],
                column_count: 0,
            },
            list_item_bullet_only: false,
            inline_size: InlineSize::NONE,
            lists: Vec::new(),
            pager: Pager::new(settings.viewport_height),
            anchors: Anchors::new(),
            notes: Footnotes::new(),
            paragraph_index: 0,
            list_item_index: 0,
            visible_offset: 0,
            inside_body: false,
            synthetic_text: false,
            hidden_depth: 0,
            inside_note_link: false,
            note_link_depth: -1,
            note_link_id: 0,
            words_placed: 0,
            out_of_memory: false,
        }
    }

    /// Replaces the settings; a pager with no page yet takes the new page height.
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
        if !self.pager.has_page() {
            self.pager = Pager::new(settings.viewport_height);
        }
    }

    /// Sets the chapter's folder in the book, for image paths.
    ///
    /// # Errors
    /// When it cannot be stored.
    pub fn set_content_base(&mut self, base: &[u8]) -> Result<(), Full> {
        self.content_base.clear();
        self.content_base
            .try_reserve(base.len())
            .map_err(|_| Full)?;
        self.content_base.extend_from_slice(base);
        Ok(())
    }

    /// Sets the open table's column widths, as a scan of its markup would (the host tests drive
    /// the parser without a file to scan).
    pub fn plan_table_columns(&mut self, widths: &[u16]) {
        let count = widths.len().min(MAX_COLUMNS);
        if let (Some(dst), Some(src)) = (
            self.table.column_widths.get_mut(..count),
            widths.get(..count),
        ) {
            dst.copy_from_slice(src);
        }
        self.table.column_count = u8::try_from(count).unwrap_or(0);
    }

    /// Starts the page cursor afresh, with no page (the host tests take the current page away).
    pub fn restart_pages(&mut self) {
        self.pager = Pager::new(self.settings.viewport_height);
    }

    /// Adds an id the table of contents starts a chapter at.
    pub fn add_toc_anchor(&mut self, name: &[u8]) {
        self.out_of_memory |= self.anchors.add_toc(name).is_err();
    }

    /// The anchors placed so far.
    #[must_use]
    pub fn anchors(&self) -> &Anchors {
        &self.anchors
    }

    /// The pager, for tests that look at the page cursor.
    #[must_use]
    pub fn pager(&self) -> &Pager {
        &self.pager
    }

    pub(crate) fn pages<'a, H: Host>(
        &'a mut self,
        host: &'a mut H,
    ) -> (&'a mut Pager, PagesOf<'a, H>) {
        (
            &mut self.pager,
            PagesOf {
                host,
                paragraph: self.paragraph_index,
                list_item: self.list_item_index,
                out_of_memory: &mut self.out_of_memory,
            },
        )
    }

    /// Starts the parse: the root block style and the first text block. False when the XML parser
    /// cannot be made.
    pub fn begin<H: Host>(&mut self, host: &mut H) -> bool {
        self.html_ended = false;
        let mut root = Block::DEFAULT;
        root.alignment = root_alignment(self.settings.paragraph_alignment);
        self.block_stack.clear();
        if self.block_stack.try_reserve(8).is_ok() {
            self.block_stack.push(root);
        } else {
            self.out_of_memory = true;
        }
        self.pager = Pager::new(self.settings.viewport_height);
        self.drop_cap = InitialLetter::NONE;
        self.lists.clear();
        self.table.depth = 0;
        self.table.inside_cell = false;
        self.table.row_stacked = false;
        self.table.rows_spanned_remaining = 0;
        self.table.row_text_bytes = 0;
        self.table.row_cells.clear();
        self.clear_cell_lines(host);

        let mut first = Block::DEFAULT;
        first.text_align_defined = true;
        first.alignment = root.alignment;
        self.start_new_text_block(host, &first, true);

        let mut xml = Parser::new(XML_LIMIT);
        // HTML entities (like &nbsp;), with or without an external DTD (which is not read).
        xml.skip_undefined_entities();
        self.xml = Boxed::try_new(xml);
        self.xml.is_some()
    }

    /// Whether text was lost for lack of memory.
    #[must_use]
    pub fn lost_text(&self) -> bool {
        self.out_of_memory || self.text.as_ref().is_some_and(|t| t.dropped_words())
    }

    /// Reads and parses the next piece of the chapter.
    pub fn step<H: Host>(&mut self, host: &mut H) -> Step {
        if self.lost_text() {
            return Step::Error;
        }
        // Off the stack, as the parse goes deep from here.
        let mut buf = core::mem::take(&mut self.read_buf);
        if buf.len() < PARSE_BUFFER {
            if buf.try_reserve(PARSE_BUFFER - buf.len()).is_err() {
                return Step::Error;
            }
            buf.resize(PARSE_BUFFER, 0);
        }
        let (len, more) = host.read(&mut buf);
        if len == 0 && more {
            self.read_buf = buf;
            return Step::Error;
        }
        let Some(mut xml) = self.xml.take() else {
            self.read_buf = buf;
            return Step::Error;
        };
        let result = xml.feed(
            buf.get(..len).unwrap_or_default(),
            !more,
            &mut crate::flow::Events {
                chapter: self,
                host,
            },
        );
        self.xml = Some(xml);
        self.read_buf = buf;
        match result {
            Ok(()) if more => Step::More,
            Ok(()) => Step::Done,
            // Trailing data after </html> is ignored.
            Err(_) if self.html_ended => Step::Done,
            Err(_) => Step::Error,
        }
    }

    /// Ends the parse: the trailing page. False when text was lost.
    pub fn finish<H: Host>(&mut self, host: &mut H) -> bool {
        if self.lost_text() {
            return false;
        }
        self.xml = None;
        self.read_buf = Vec::new();
        if self.text.is_some() {
            self.make_pages(host, true, true);
            if self.out_of_memory {
                return false;
            }
            let completed = self.pager.completed();
            if self.anchors.record_pending(completed).is_err() {
                return false;
            }
            let offset = self.visible_offset;
            let (pager, mut pages) = self.pages(host);
            pager.finish(&mut pages, offset);
            self.text = None;
        }
        true
    }

    /// Ends the parse without its trailing page.
    pub fn abort(&mut self) {
        self.xml = None;
        self.read_buf = Vec::new();
    }

    pub(crate) fn clear_cell_lines<H: Host>(&mut self, host: &mut H) {
        for lines in &mut self.table.cell_lines {
            for line in lines.drain(..) {
                if line.handle != NO_LINE {
                    host.drop_line(line.handle);
                }
            }
        }
        self.table.line_offsets.clear();
    }
}
