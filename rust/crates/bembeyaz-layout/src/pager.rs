//! Where the chapter parser (lib/Epub's `ChapterHtmlSlimParser`) places what it lays out: the
//! next free row on the page, when a page is full and which of its last lines go to the next one,
//! the first visible character each page shows, and the framed boxes around its content. The
//! pages and their elements belong to the caller, behind [`Pages`]; the pager counts what the
//! caller puts on them.

use crate::boxes::{Boxes, Segment, Side};
use crate::carry::{self, Break, MAX_LINES};

/// A table row's separator sits this far below the row and takes this much room.
const ROW_SEPARATOR_GAP: i16 = 4;

/// The caller's pages: one is current while content is placed, and a page break may set the
/// next one aside before the current one is handed on.
pub trait Pages {
    /// Starts a new current page; false when it could not be made.
    fn new_page(&mut self) -> bool;
    /// Sets aside a next page that takes the current page's last `lines` elements, moved up by
    /// `top`, and the page's notes from `top` down; false when it could not be made.
    fn split(&mut self, lines: u8, top: i16) -> bool;
    /// Hands on the current page, whose first visible character is `visible_offset`. The page
    /// set aside by [`split`](Self::split), if any, becomes current.
    fn emit(&mut self, visible_offset: u32);
    /// Puts a box's part on the current page.
    fn add_box(&mut self, segment: &Segment);
}

/// The paragraph whose line is being placed.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[allow(clippy::struct_excessive_bools)]
pub struct Line {
    /// The line belongs to a paragraph being laid out (not a table cell's).
    pub paragraph: bool,
    /// The paragraph has a drop cap, whose lines stay on its page.
    pub drop_cap: bool,
    /// It is the paragraph's last line.
    pub last_line: bool,
}

/// The chapter's page cursor. Every element the caller puts on the current page is reported to
/// it, so it knows which of the page's last elements are text lines.
#[derive(Default)]
pub struct Pager {
    page_height: u16,
    has_page: bool,
    /// Elements on the current page, and how many of the last are text lines.
    elements: u16,
    trailing_lines: u16,
    /// The next free row.
    y: i16,
    completed: u16,
    visible_offset: u32,
    visible_offset_set: bool,
    /// The page's last lines (oldest first): where each starts and its first visible character.
    recent_tops: [i16; MAX_LINES as usize],
    recent_offsets: [u32; MAX_LINES as usize],
    recent_len: u8,
    /// The laid-out paragraph's lines on the page.
    paragraph_lines: u8,
    /// The heading lines at the page's end that wait to move with what follows them.
    keep_with_next: u8,
    boxes: Boxes,
}

impl Pager {
    /// A pager for pages `page_height` rows tall, with no page yet.
    #[must_use]
    pub fn new(page_height: u16) -> Self {
        Self {
            page_height,
            ..Self::default()
        }
    }

    /// The next free row on the current page.
    #[must_use]
    pub fn y(&self) -> i16 {
        self.y
    }

    /// Moves the next free row (a table row's lines are placed by hand).
    pub fn set_y(&mut self, y: i16) {
        self.y = y;
    }

    /// Moves the next free row down by `rows` (block spacing), wrapping at 16 bits as the C++ did.
    #[allow(clippy::cast_possible_truncation)]
    pub fn advance(&mut self, rows: i32) {
        self.y = (i32::from(self.y) + rows) as i16;
    }

    /// The pages handed on so far.
    #[must_use]
    pub fn completed(&self) -> u16 {
        self.completed
    }

    /// Whether a current page exists.
    #[must_use]
    pub fn has_page(&self) -> bool {
        self.has_page
    }

    /// Whether the current page holds anything.
    #[must_use]
    pub fn has_content(&self) -> bool {
        self.has_page && self.elements > 0
    }

    /// The open boxes.
    #[must_use]
    pub fn boxes_ref(&self) -> &Boxes {
        &self.boxes
    }

    /// Content from `top` to `bottom` was placed, inside every open box.
    pub fn note(&mut self, top: i32, bottom: i32) {
        self.boxes.note(top, bottom);
    }

    /// The first visible character the current page shows, unless one is set already. The first
    /// page always starts at the body's start.
    pub fn set_visible_offset(&mut self, offset: u32) {
        if self.visible_offset_set {
            return;
        }
        self.visible_offset = if self.completed == 0 { 0 } else { offset };
        self.visible_offset_set = true;
    }

    /// Starts a page when none is current.
    pub fn ensure_page(&mut self, pages: &mut impl Pages) {
        if !self.has_page {
            self.start_page(pages);
        }
    }

    fn start_page(&mut self, pages: &mut impl Pages) -> bool {
        self.has_page = pages.new_page();
        self.elements = 0;
        self.trailing_lines = 0;
        self.y = 0;
        self.visible_offset_set = false;
        self.has_page
    }

    /// The caller put an element other than a text line on the current page.
    pub fn added(&mut self) {
        if self.has_page {
            self.elements = self.elements.saturating_add(1);
            self.trailing_lines = 0;
        }
    }

    /// Slices the open boxes at the page's end and hands the page on (when there is one).
    fn emit(&mut self, pages: &mut impl Pages) {
        if self.has_page {
            self.add_box_segments(pages);
            pages.emit(self.visible_offset);
        }
        self.has_page = false;
        self.elements = 0;
        self.trailing_lines = 0;
        self.recent_len = 0;
        self.keep_with_next = 0;
        self.paragraph_lines = 0;
    }

    /// Puts each open box's part on the page. Out of line, so the segments are off the stack
    /// while the page is written.
    #[inline(never)]
    fn add_box_segments(&mut self, pages: &mut impl Pages) {
        let page_height = self.page_height;
        let mut segments = [Segment::default(); crate::boxes::MAX_BOXES];
        let mut count = 0;
        self.boxes.break_page(page_height, |segment| {
            if let Some(slot) = segments.get_mut(count) {
                *slot = segment;
                count += 1;
            }
        });
        for segment in segments.iter().take(count) {
            pages.add_box(segment);
            self.added();
        }
    }

    /// Completes the current page and starts the next.
    pub fn complete_page(&mut self, pages: &mut impl Pages) {
        self.emit(pages);
        self.completed = self.completed.wrapping_add(1);
        self.start_page(pages);
    }

    /// Completes the current page if `requested` and it holds anything (a CSS page break).
    pub fn page_break(&mut self, pages: &mut impl Pages, requested: bool) {
        if requested && self.has_content() {
            self.complete_page(pages);
        }
    }

    /// Hands on the last page at the chapter's end, whose first visible character is
    /// `visible_offset` if it has none yet.
    pub fn finish(&mut self, pages: &mut impl Pages, visible_offset: u32) {
        self.set_visible_offset(visible_offset);
        self.emit(pages);
        self.completed = self.completed.wrapping_add(1);
    }

    /// How many of the page's last elements may move: its trailing text lines still remembered,
    /// never all of its elements.
    fn carry_limit(&self) -> u8 {
        if !self.has_page {
            return 0;
        }
        let limit = u16::from(self.recent_len)
            .min(u16::from(MAX_LINES))
            .min(self.elements.saturating_sub(1))
            .min(self.trailing_lines);
        u8::try_from(limit).unwrap_or(0)
    }

    /// The heading lines at the page's end that may move with what follows them.
    #[must_use]
    pub fn keep_with_next_carry(&self) -> u8 {
        carry::lines_to_carry(&Break {
            movable: self.carry_limit(),
            keep_with_next: self.keep_with_next,
            ..Break::default()
        })
        .lines
    }

    /// Completes the page, moving its last `lines` text lines (no more than may move) to the top
    /// of the next one with their spacing intact; `paragraph_lines` of them belong to the
    /// paragraph being laid out. False when the next page could not be made, and nothing changed.
    pub fn break_page_carrying(
        &mut self,
        pages: &mut impl Pages,
        lines: u8,
        paragraph_lines: u8,
        visible_offset: u32,
    ) -> bool {
        let lines = lines.min(self.carry_limit());
        let first = usize::from(self.recent_len - lines);
        let carried_top = if lines > 0 {
            self.recent_tops.get(first).copied().unwrap_or(0)
        } else {
            0
        };
        if !pages.split(lines, carried_top) {
            return false;
        }
        let carried_bottom = self.y;
        // The carried lines' tops and offsets, before the page's lines are forgotten.
        let mut carried = [(0i16, 0u32); MAX_LINES as usize];
        let remembered = self
            .recent_tops
            .iter()
            .zip(&self.recent_offsets)
            .skip(first);
        for (slot, (&top, &offset)) in carried.iter_mut().zip(remembered).take(usize::from(lines)) {
            *slot = (top, offset);
        }
        self.elements = self.elements.saturating_sub(u16::from(lines));
        self.trailing_lines = self.trailing_lines.saturating_sub(u16::from(lines));

        self.set_visible_offset(visible_offset);
        self.emit(pages);
        self.completed = self.completed.wrapping_add(1);
        // The page set aside is current now, with the carried lines on it.
        self.has_page = true;
        self.elements = u16::from(lines);
        self.trailing_lines = u16::from(lines);
        self.y = 0;
        self.visible_offset_set = false;
        for &(top, offset) in carried.iter().take(usize::from(lines)) {
            self.set_visible_offset(offset);
            self.remember_line(top.wrapping_sub(carried_top), offset);
        }
        if lines > 0 {
            self.y = carried_bottom.wrapping_sub(carried_top);
            self.note(0, i32::from(self.y));
        }
        self.paragraph_lines = paragraph_lines;
        true
    }

    fn remember_line(&mut self, top: i16, offset: u32) {
        let len = usize::from(self.recent_len);
        if len == usize::from(MAX_LINES) {
            self.recent_tops.copy_within(1.., 0);
            self.recent_offsets.copy_within(1.., 0);
            self.recent_len -= 1;
        }
        let at = usize::from(self.recent_len);
        if let (Some(t), Some(o)) = (
            self.recent_tops.get_mut(at),
            self.recent_offsets.get_mut(at),
        ) {
            *t = top;
            *o = offset;
            self.recent_len += 1;
        }
    }

    /// Where the next text line, `height` rows tall and showing `visible_offset` first, goes:
    /// when it does not fit, the page breaks first, its paragraph's lone lines and a heading
    /// above going along. The caller puts the line there and reports it with
    /// [`line_added`](Self::line_added).
    pub fn line_top(
        &mut self,
        pages: &mut impl Pages,
        height: i32,
        visible_offset: u32,
        line: Line,
    ) -> i16 {
        self.ensure_page(pages);
        if i32::from(self.y) + height > i32::from(self.page_height) {
            let carry = carry::lines_to_carry(&Break {
                movable: self.carry_limit(),
                paragraph: line.paragraph,
                drop_cap: line.drop_cap,
                paragraph_lines: self.paragraph_lines,
                last_line: line.last_line,
                keep_with_next: self.keep_with_next,
            });
            self.break_page_carrying(pages, carry.lines, carry.paragraph_lines, visible_offset);
        }
        self.set_visible_offset(visible_offset);
        self.y
    }

    /// The caller put a text line, `height` rows tall, at the next free row.
    #[allow(clippy::cast_possible_truncation)]
    pub fn line_added(&mut self, height: i32, visible_offset: u32, paragraph: bool) {
        let top = self.y;
        self.note(i32::from(top), i32::from(top) + height);
        self.y = (i32::from(top) + height) as i16;
        if paragraph && self.paragraph_lines < u8::MAX {
            self.paragraph_lines += 1;
        }
        self.remember_line(top, visible_offset);
        if self.has_page {
            self.elements = self.elements.saturating_add(1);
            self.trailing_lines = self.trailing_lines.saturating_add(1);
        }
    }

    /// A block ended: a heading's lines on the page wait to move with what follows it.
    pub fn end_block(&mut self, heading: bool) {
        self.keep_with_next =
            carry::keep_after_block(self.keep_with_next, self.paragraph_lines, heading);
        self.paragraph_lines = 0;
    }

    /// Lets the page's last heading lines stay where they are.
    pub fn clear_keep_with_next(&mut self) {
        self.keep_with_next = 0;
    }

    /// An empty line (`<br/>` with no text before it) on a page that holds something.
    #[allow(clippy::cast_possible_truncation)]
    pub fn blank_line(&mut self, line_height: i32) {
        if self.has_content() {
            self.y = (i32::from(self.page_height)).min(i32::from(self.y) + line_height) as i16;
        }
    }

    /// Where a drop cap spanning `height` rows starts: on a new page, with the heading above it,
    /// when the lines beside it would not fit. The caller puts it there and reports it with
    /// [`added`](Self::added) and [`note`](Self::note).
    pub fn drop_cap_top(
        &mut self,
        pages: &mut impl Pages,
        height: i32,
        visible_offset: u32,
    ) -> i16 {
        self.ensure_page(pages);
        if self.elements > 0 && i32::from(self.y) + height > i32::from(self.page_height) {
            let carry = self.keep_with_next_carry();
            self.break_page_carrying(pages, carry, 0, visible_offset);
        }
        self.y
    }

    /// The next free row is at least `y` (below a drop cap's spanned lines), wrapping at 16 bits
    /// as the C++ did.
    #[allow(clippy::cast_possible_truncation)]
    pub fn reach(&mut self, y: i32) {
        self.y = i32::from(self.y).max(y) as i16;
    }

    /// Where a horizontal rule goes, after `top` rows of spacing: on a new page when it and its
    /// spacing (`total` rows) do not fit. None when no page could be made. The caller puts it
    /// there and reports it with [`rule_added`](Self::rule_added).
    #[allow(clippy::cast_possible_truncation)]
    pub fn rule_top(
        &mut self,
        pages: &mut impl Pages,
        top: i16,
        total: i16,
        visible_offset: u32,
    ) -> Option<i16> {
        if !self.has_page && !pages.new_page() {
            return None;
        }
        if !self.has_page {
            self.has_page = true;
            self.elements = 0;
            self.trailing_lines = 0;
            self.y = 0;
        }
        if self.elements > 0 && i32::from(self.y) + i32::from(total) > i32::from(self.page_height) {
            self.set_visible_offset(visible_offset);
            self.emit(pages);
            self.completed = self.completed.wrapping_add(1);
            if !self.start_page(pages) {
                return None;
            }
        }
        self.y = (i32::from(self.y) + i32::from(top)) as i16;
        Some(self.y)
    }

    /// The caller put a rule `thickness` rows tall at the next free row, with `bottom` rows of
    /// spacing below it.
    #[allow(clippy::cast_possible_truncation)]
    pub fn rule_added(&mut self, thickness: i16, bottom: i16, visible_offset: u32) {
        self.set_visible_offset(visible_offset);
        self.added();
        let y = i32::from(self.y);
        self.note(y, y + i32::from(thickness));
        self.keep_with_next = 0;
        self.y = (y + i32::from(thickness) + i32::from(bottom)) as i16;
    }

    /// Where a table row's separator goes, if one fits on a page that holds something.
    #[must_use]
    pub fn row_separator_top(&self, page_width: u16) -> Option<i16> {
        (self.has_content()
            && page_width > 0
            && i32::from(self.y) + i32::from(ROW_SEPARATOR_GAP) <= i32::from(self.page_height))
        .then(|| self.y.wrapping_add(1))
    }

    /// The caller put a row separator where [`row_separator_top`](Self::row_separator_top) said.
    pub fn row_separator_added(&mut self) {
        self.added();
        self.y = self.y.wrapping_add(ROW_SEPARATOR_GAP);
    }

    /// Whether a table row line `height` rows tall, with `gaps` rows around it, overflows a page
    /// that holds something.
    #[must_use]
    pub fn row_overflows(&self, gaps: i32, height: i32) -> bool {
        self.has_content() && i32::from(self.y) + gaps + height > i32::from(self.page_height)
    }

    /// Completes the page in the middle of a table row, whose next line shows `visible_offset`
    /// first, and starts the next; false when it could not be made.
    pub fn break_table_row(&mut self, pages: &mut impl Pages, visible_offset: u32) -> bool {
        self.set_visible_offset(visible_offset);
        self.emit(pages);
        self.completed = self.completed.wrapping_add(1);
        self.start_page(pages)
    }

    /// Where an image `height` rows tall goes, with `margin_top` and `margin_bottom` rows around
    /// it: on a new page when it does not fit, with the heading above when both fit there, and
    /// with less top margin when even a new page has no room for all of it. None when no page
    /// could be made. The caller puts it there and reports it with
    /// [`image_added`](Self::image_added).
    pub fn image_top(
        &mut self,
        pages: &mut impl Pages,
        margin_top: i16,
        height: i32,
        margin_bottom: i16,
        visible_offset: u32,
    ) -> Option<i16> {
        let mut margin_top = i32::from(margin_top);
        if self.has_content()
            && i32::from(self.y) + margin_top + height + i32::from(margin_bottom)
                > i32::from(self.page_height)
        {
            let mut carry = self.keep_with_next_carry();
            if carry > 0 {
                let first = usize::from(self.recent_len - carry.min(self.recent_len));
                let top = self.recent_tops.get(first).copied().unwrap_or(0);
                let carried = i32::from(self.y) - i32::from(top);
                if carried + margin_top + height > i32::from(self.page_height) {
                    carry = 0;
                }
            }
            self.break_page_carrying(pages, carry, 0, visible_offset);
        } else if !self.has_page && !self.start_page(pages) {
            return None;
        }
        // Never past the page's bottom, where the image would fail its bounds check when drawn:
        // a fresh page has no room for the margin of an image as tall as the page.
        if i32::from(self.y) + margin_top + height > i32::from(self.page_height) {
            let room = i32::from(self.page_height) - height - i32::from(self.y);
            margin_top = room.max(0);
        }
        self.advance(margin_top);
        Some(self.y)
    }

    /// The caller put an image `height` rows tall at the next free row, with `margin_bottom` rows
    /// below it.
    pub fn image_added(&mut self, height: i32, margin_bottom: i16, visible_offset: u32) {
        self.set_visible_offset(visible_offset);
        self.added();
        let y = i32::from(self.y);
        self.note(y, y + height);
        self.keep_with_next = 0;
        self.advance(height + i32::from(margin_bottom));
    }

    /// Opens a box for the element at `depth` (see [`Boxes::open`]).
    pub fn open_box(
        &mut self,
        depth: i32,
        sides: [Side; 4],
        shaded: bool,
        left: i16,
        right: i16,
        padding: &mut [i16; 4],
    ) -> bool {
        self.boxes.open(depth, sides, shaded, left, right, padding)
    }

    /// The height the innermost box needs of its own (see [`Boxes::empty_height`]).
    #[must_use]
    pub fn box_empty_height(&self) -> i32 {
        self.boxes.empty_height()
    }

    /// Closes the innermost box. An empty one gets its own height after `above` rows (on a new
    /// page when it does not fit, without them) and keeps `below` rows under it.
    #[allow(clippy::cast_possible_truncation)]
    pub fn close_box(&mut self, pages: &mut impl Pages, above: i32, below: i32) {
        let height = self.boxes.empty_height();
        let mut placed = None;
        if height > 0 {
            let mut above = above;
            self.ensure_page(pages);
            if self.elements > 0 && i32::from(self.y) + above + height > i32::from(self.page_height)
            {
                self.complete_page(pages);
                above = 0;
            }
            let top = i32::from(self.y) + above;
            placed = Some(top);
            self.y = (top + height + below) as i16;
        }
        let segment = self.boxes.close(placed, self.page_height);
        if let Some(top) = placed {
            self.note(top, top + height);
        }
        if let Some(segment) = segment
            && self.has_page
        {
            pages.add_box(&segment);
            self.added();
        }
    }
}

#[cfg(test)]
mod tests;
