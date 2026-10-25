//! What the chapter parser needs from the firmware: its fonts, the chapter's bytes, the pages and
//! the elements on them. The pages, lines and images are C++ objects; the parser holds handles to
//! lines it has laid out but not yet placed (a table row's cells).

use bembeyaz_css::Rules;
use bembeyaz_css::block::Block;
use bembeyaz_layout::boxes::Segment;
use bembeyaz_layout::{Line, Metrics};

/// A laid-out line the host keeps until it is placed or dropped.
pub type LineHandle = u32;

/// A laid-out line: its handle, and what placing it needs.
#[derive(Clone, Copy, Debug, Default)]
pub struct LineInfo {
    pub handle: LineHandle,
    pub words: u16,
    /// The line's height: its font's line height with the extra ascent of taller inline words.
    pub base_height: i32,
    /// The room ruby takes above the line.
    pub ruby_shift: i32,
}

impl LineInfo {
    #[must_use]
    pub fn height(&self) -> i32 {
        self.base_height + self.ruby_shift
    }
}

/// An image's size from its header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Image {
    /// No decoder reads the format.
    Unsupported,
    /// The host could not size it.
    Unsized,
    Sized {
        width: i16,
        height: i16,
    },
}

/// The chapter's link targets, by 1-based id; each is followed by a NUL.
pub trait Links {
    fn target(&self, id: u8) -> &[u8];
}

/// The firmware's side of a chapter.
pub trait Host: Metrics {
    /// Starts a new current page; false when it could not be made.
    fn new_page(&mut self) -> bool;
    /// Sets aside a next page that takes the current page's last `lines` elements, moved up by
    /// `top`, and the page's notes from `top` down; false when it could not be made.
    fn split(&mut self, lines: u8, top: i16) -> bool;
    /// Hands on the current page, with its first visible character and the `<p>` and `<li>`
    /// counts so far. The page set aside, if any, becomes current.
    fn emit(&mut self, visible_offset: u32, paragraph: u16, list_item: u16);
    /// Puts a box's part on the current page; false when it could not be made.
    fn add_box(&mut self, segment: &Segment) -> bool;

    /// Makes a laid-out line of a paragraph whose style is `block`; None when it could not be made.
    fn make_line(&mut self, line: &Line<'_>, block: &Block, links: &dyn Links) -> Option<LineInfo>;
    /// Moves a kept line's left margin to `x` (a table cell's column).
    fn set_line_left(&mut self, line: LineHandle, x: i16);
    /// Puts a kept line on the current page at row `y`, with its link areas; false when it could
    /// not be placed (it is gone either way).
    fn push_line(&mut self, line: LineHandle, y: i16) -> bool;
    /// Forgets a kept line.
    fn drop_line(&mut self, line: LineHandle);
    fn push_rule(&mut self, width: i16, thickness: u8, x: i16, y: i16) -> bool;
    fn push_drop_cap(
        &mut self,
        font: i32,
        scale: u16,
        bold: bool,
        text: &[u8],
        x: i16,
        y: i16,
    ) -> bool;
    /// Puts the image last sized by [`image`](Self::image) on the current page.
    fn push_image(&mut self, width: i32, height: i32, x: i32, y: i16) -> bool;
    /// Adds a note reference (number and target, each followed by a NUL) to the current page.
    fn add_footnote(&mut self, number: &[u8], href: &[u8]);

    /// The line height of `font` (lines `compression` apart), as the renderer gives it.
    fn line_height(&mut self, font: i32, compression: f32) -> i32;
    /// The renderer's variant of `font` at `scale`, or 0.
    fn font_variant(&mut self, font: i32, scale: f32) -> i32;
    /// A glyph's advance (12.4 fixed point) and the top of its ink.
    fn glyph(&mut self, font: i32, cp: u32, bold: bool) -> Option<(i32, i32)>;

    /// Reads the chapter's next bytes; also whether more follow. A read of nothing with more to
    /// come is an error.
    fn read(&mut self, buf: &mut [u8]) -> (usize, bool);
    /// Opens a second reader at `offset` of the chapter, for a table's column scan.
    fn table_open(&mut self, offset: u64) -> bool;
    fn table_read(&mut self, buf: &mut [u8]) -> usize;
    /// Closes the second reader once the scan is done.
    fn table_close(&mut self);
    /// The width of `word` (followed by a NUL) in the section font, readied first on an SD font.
    fn table_word(&mut self, word: &[u8], bold: bool) -> i32;
    /// Sizes the image at `path` in the book, keeping it ready for [`push_image`](Self::push_image).
    fn image(&mut self, path: &[u8]) -> Image;
    /// The book's stylesheets, when it has any.
    fn rules(&self) -> Option<&Rules>;
}
