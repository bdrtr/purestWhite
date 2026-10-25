//! Grid tables: a row's cells are buffered, laid out side by side and placed line by line; rows
//! too rich or too large for a grid are stacked in ordinary flow.

use crate::blocks::BlockExt;
use crate::chapter::Chapter;
use crate::flow::{CELL_HORIZONTAL_PADDING, has, visible};
use crate::host::Host;
use bembeyaz_css::block::Block;
use bembeyaz_css::{BorderSide, BorderStyle, Defined, Direction, FontWeight, LengthField, Style};
use bembeyaz_html::element::{self, Span, name};
use bembeyaz_layout::boxes::{Segment, Side};
use bembeyaz_layout::table::{self, Cell, MAX_COLUMNS, Measure, Source};
use core::cell::RefCell;

/// A row separator is a rule this thick.
const SEPARATOR_THICKNESS: u8 = 1;

fn span(value: Option<&[u8]>) -> u16 {
    match value.map(element::table_span) {
        None => 1,
        Some(Span::Cells(n)) => n.get(),
        Some(Span::Rest) => u16::MAX,
    }
}

/// The table's markup from its start tag, for the column scan.
struct Markup<'a, H> {
    host: &'a mut H,
}

impl<H: Host> Source for Markup<'_, H> {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        self.host.table_read(buf)
    }
    fn advance(&mut self, word: &[u8], bold: bool) -> i32 {
        self.host.table_word(word, bold)
    }
}

/// `CssLength::toPixelsInt16`: toward zero, kept to 16 bits.
#[allow(clippy::cast_possible_truncation)]
fn px16(css: &Style, field: LengthField, em: f32, width: f32) -> i16 {
    css.length(field).pixels(em, width) as i32 as i16
}

impl Chapter {
    pub(crate) fn open_table<H: Host>(
        &mut self,
        host: &mut H,
        tag: &impl crate::flow::Tag,
        css: &Style,
        rtl: bool,
        border: Option<&[u8]>,
    ) {
        // Nested tables flatten into the enclosing cell.
        if self.table.depth > 0 {
            if self.table.depth == 1 && self.table.inside_cell && self.word_len > 0 {
                self.flush_word(host);
            }
            self.next_word_continues = false;
            self.table.depth += 1;
            return;
        }
        if self.word_len > 0 {
            self.flush_word(host);
        }
        if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
            self.make_pages(host, true, true);
            self.text = None;
        }
        self.place_pending_anchor(host);
        self.inline_size = crate::chapter::InlineSize::NONE;
        self.drop_cap.pending = false;
        self.push_block_text_entry(css);
        let t = &mut self.table;
        t.depth = 1;
        t.inside_cell = false;
        t.row_stacked = false;
        t.row_rtl = rtl;
        t.rows_spanned_remaining = 0;
        t.row_text_bytes = 0;
        t.row_cells.clear();
        self.out_of_memory |= t.row_cells.try_reserve(MAX_COLUMNS).is_err();
        t.border = crate::chapter::NO_BORDER;
        t.column_count = 0;
        // Rows stream in one at a time, so shared column widths are planned from the markup.
        if host.table_open(tag.offset()) {
            let space = host.space_width(self.settings.font, 0);
            let measure = table::measure_columns(&mut Markup { host: &mut *host }, space);
            host.table_close();
            if let Some(measure) = measure {
                self.plan_columns(&measure);
            }
        }
        for &side in &css.borders {
            if !visible(self.table.border) && visible(side) {
                self.table.border = side;
            }
        }
        if !visible(self.table.border) && border.is_some_and(|b| crate::flow::atoi(b) > 0) {
            self.table.border = BorderSide {
                width: 1,
                style: BorderStyle::Solid,
            };
        }
    }

    fn plan_columns(&mut self, measure: &Measure) {
        self.table.column_count = match table::plan_columns(
            measure,
            i32::from(self.settings.viewport_width),
            i32::from(CELL_HORIZONTAL_PADDING),
        ) {
            Some(widths) => {
                let count = usize::from(measure.columns).min(MAX_COLUMNS);
                if let (Some(dst), Some(src)) = (
                    self.table.column_widths.get_mut(..count),
                    widths.get(..count),
                ) {
                    dst.copy_from_slice(src);
                }
                measure.columns
            }
            None => 0,
        };
    }

    pub(crate) fn open_row<H: Host>(&mut self, host: &mut H, css: &Style, rtl: bool) {
        self.finish_row(host);
        if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
            // Text before the first row is typically a <caption>.
            self.make_pages(host, true, true);
        }
        self.text = None;
        let t = &mut self.table;
        t.row_text_bytes = 0;
        t.row_stacked = t.rows_spanned_remaining > 0;
        t.row_rtl = rtl;
        if t.rows_spanned_remaining != u16::MAX && t.rows_spanned_remaining > 0 {
            t.rows_spanned_remaining -= 1;
        }
        self.push_block_text_entry(css);
    }

    pub(crate) fn open_cell<H: Host>(
        &mut self,
        host: &mut H,
        kind: u16,
        named: u16,
        css: &Style,
        colspan: Option<&[u8]>,
        rowspan: Option<&[u8]>,
    ) {
        if self.word_len > 0 {
            self.flush_word(host);
        }
        self.close_cell(host);
        if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
            self.make_pages(host, true, true);
        }
        self.text = None;
        let columns = span(colspan);
        let rows = span(rowspan);
        if columns > 1 || rows > 1 {
            self.fallback_row_to_stacked(host);
        }
        if rows > 1 {
            let remaining = if rows == u16::MAX { u16::MAX } else { rows - 1 };
            self.table.rows_spanned_remaining = self.table.rows_spanned_remaining.max(remaining);
        }

        let mut cell = Block::DEFAULT;
        self.apply_block_font_scale(
            &mut cell,
            css,
            u8::try_from(kind & element::HEADING_LEVEL).unwrap_or(0),
        );
        #[allow(clippy::cast_precision_loss)]
        let em = host.ascender(self.settings.font) as f32;
        let viewport = self.settings.viewport_width;
        let height = self.settings.viewport_height;
        // Author padding is bounded, so even a stacked cell keeps room for content.
        let padding = |field: LengthField, limit: i32| {
            #[allow(clippy::cast_possible_truncation)]
            let px = i32::from(px16(css, field, em, f32::from(viewport))).clamp(0, limit) as i16;
            px
        };
        cell.padding_left = if has(css, Defined::length(LengthField::PaddingLeft)) {
            padding(LengthField::PaddingLeft, i32::from(viewport / 4))
        } else {
            CELL_HORIZONTAL_PADDING
        };
        cell.padding_right = if has(css, Defined::length(LengthField::PaddingRight)) {
            padding(LengthField::PaddingRight, i32::from(viewport / 4))
        } else {
            CELL_HORIZONTAL_PADDING
        };
        cell.padding_top = padding(LengthField::PaddingTop, i32::from(height / 4));
        cell.padding_bottom = padding(LengthField::PaddingBottom, i32::from(height / 4));
        cell.text_align_defined = true;
        let css_rtl = has(css, Defined::DIRECTION) && css.direction == Direction::Rtl;
        cell.alignment = if has(css, Defined::TEXT_ALIGN) {
            css.text_align as u8
        } else if self.effective.text_align_defined {
            self.effective.text_align as u8
        } else if css_rtl {
            bembeyaz_css::TextAlign::Right as u8
        } else {
            bembeyaz_css::TextAlign::Left as u8
        };
        if has(css, Defined::DIRECTION) {
            cell.direction_defined = true;
            cell.is_rtl = css_rtl;
        }
        self.set_text(cell);
        for &side in &css.borders {
            if !visible(self.table.border) && visible(side) {
                self.table.border = side;
            }
        }
        self.push_block_style(&cell);
        self.table.inside_cell = true;
        self.words_placed = 0;
        self.place_pending_anchor(host);
        self.push_block_text_entry(css);
        if named == name::TH
            && (!has(css, Defined::FONT_WEIGHT) || css.font_weight == FontWeight::Bold)
        {
            self.bold_until = self.bold_until.min(self.depth);
        }
    }

    /// The cells buffered so far go into ordinary flow, and so does the rest of the row.
    pub(crate) fn fallback_row_to_stacked<H: Host>(&mut self, host: &mut H) {
        if self.table.row_stacked {
            return;
        }
        let active = self.text.take();
        self.table.row_stacked = true;
        // Taken and put back, keeping its capacity for the next row.
        let mut cells = core::mem::take(&mut self.table.row_cells);
        for cell in cells.drain(..) {
            self.text = Some(cell);
            self.words_placed = 0;
            if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
                self.make_pages(host, true, true);
            }
        }
        self.table.row_cells = cells;
        self.text = active;
        self.words_placed = 0;
    }

    pub(crate) fn close_cell<H: Host>(&mut self, host: &mut H) {
        if !self.table.inside_cell {
            return;
        }
        self.table.inside_cell = false;
        let bottom_padding = self.block_stack.last().map_or(0, |b| b.padding_bottom);
        self.block_stack.pop();
        let Some(text) = &self.text else {
            return;
        };
        // Latched before the cell leaves `text`.
        if text.dropped_words() {
            self.out_of_memory = true;
        }
        if !self.table.row_stacked && self.table.row_cells.len() >= MAX_COLUMNS {
            self.fallback_row_to_stacked(host);
        }
        if self.table.row_stacked {
            self.words_placed = 0;
            if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
                self.make_pages(host, true, true);
            } else {
                self.pager.advance(i32::from(bottom_padding));
            }
            self.text = None;
            return;
        }
        if let Some(text) = self.text.take() {
            if self.table.row_cells.try_reserve(1).is_ok() {
                self.table.row_cells.push(text);
            } else {
                self.out_of_memory = true;
            }
        }
    }

    fn add_row_separator<H: Host>(&mut self, host: &mut H) {
        let Some(y) = self.pager.row_separator_top(self.settings.viewport_width) else {
            return;
        };
        #[allow(clippy::cast_possible_wrap)]
        let width = self.settings.viewport_width as i16;
        if host.push_rule(width, SEPARATOR_THICKNESS, 0, y) {
            self.pager.row_separator_added();
        }
    }

    /// Lays the buffered row out as a grid, or stacks it when it does not fit one.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn finish_row<H: Host>(&mut self, host: &mut H) {
        self.close_cell(host);
        if self.table.row_cells.is_empty() {
            if self.table.row_stacked {
                self.add_row_separator(host);
            }
            self.table.row_stacked = false;
            return;
        }
        let font = self.settings.font;
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        let line_height = ((host.line_height(font, 1.0) as f32 * self.settings.line_compression)
            as i32 as i16)
            .max(1);
        let column_count = self.table.row_cells.len();
        let mut cells = [Cell::default(); MAX_COLUMNS];
        for (cell, t) in cells.iter_mut().zip(&self.table.row_cells) {
            *cell = Cell {
                padding_top: t.block.padding_top,
                padding_bottom: t.block.padding_bottom,
                padding_left: t.block.padding_left,
                padding_right: t.block.padding_right,
            };
        }
        let cells = cells
            .get(..column_count.min(MAX_COLUMNS))
            .unwrap_or_default();
        let planned = (usize::from(self.table.column_count) == column_count)
            .then(|| self.table.column_widths.get(..column_count))
            .flatten();
        let grid = if column_count > MAX_COLUMNS {
            None
        } else {
            table::plan_row(
                cells,
                planned,
                self.settings.viewport_width,
                line_height,
                self.table.row_rtl,
            )
        };
        let Some(grid) = grid else {
            self.fallback_row_to_stacked(host);
            self.add_row_separator(host);
            self.table.row_stacked = false;
            return;
        };
        let max_top = i32::from(grid.top_padding);
        let max_bottom = i32::from(grid.bottom_padding);
        self.clear_cell_lines(host);
        let mut max_lines = 0;
        let mut row_cells = core::mem::take(&mut self.table.row_cells);
        for (column, text) in row_cells.iter_mut().enumerate() {
            let scale = text.block.font_scale;
            let cell_font = self.prepare_block_font(host, text, scale);
            let cell = cells.get(column).copied().unwrap_or_default();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let width = (i32::from(grid.width.get(column).copied().unwrap_or(0))
                - i32::from(cell.padding_left)
                - i32::from(cell.padding_right)) as u16;
            let spacing = (
                self.settings.character_spacing,
                self.settings.word_spacing_percent,
            );
            let language = self.settings.language;
            let shared = RefCell::new(&mut *host);
            let lines = &mut self.table.cell_lines;
            let offsets = &mut self.table.line_offsets;
            let mut oom = false;
            text.layout(
                &shared,
                cell_font,
                width,
                true,
                spacing,
                usize::MAX,
                language,
                &mut |h, line, offset, _| {
                    let Some(column_lines) = lines.get_mut(column) else {
                        h.drop_line(line.handle);
                        return;
                    };
                    let index = column_lines.len();
                    if column_lines.try_reserve(1).is_err() {
                        oom = true;
                        h.drop_line(line.handle);
                        return;
                    }
                    column_lines.push(line);
                    if offsets.len() <= index {
                        if offsets.try_reserve(index + 1 - offsets.len()).is_err() {
                            oom = true;
                            return;
                        }
                        offsets.resize(index + 1, u32::MAX);
                    }
                    if let Some(o) = offsets.get_mut(index) {
                        *o = (*o).min(offset);
                    }
                },
            );
            self.out_of_memory |= oom;
            max_lines = max_lines.max(
                self.table
                    .cell_lines
                    .get(column)
                    .map_or(0, alloc::vec::Vec::len),
            );
        }
        // Layout can drop lines too; latched before the cells go.
        if row_cells.iter().any(|t| t.dropped_words()) {
            self.out_of_memory = true;
        }
        row_cells.clear();
        self.table.row_cells = row_cells;

        // Bordered tables frame every cell, overlapping by a pixel; a row split by a page break is
        // framed per page.
        let mut border = self.table.border;
        border.width = border.width.min(3);
        let bordered = visible(border);
        let gap: i32 = if bordered {
            i32::from(border.width) + 2
        } else {
            0
        };
        let page_height = i32::from(self.settings.viewport_height);
        let frame = |chapter: &mut Self, host: &mut H, top: i32, bottom: i32| {
            let height = (bottom + 1).min(page_height) - top;
            if height <= 0 {
                return;
            }
            let side = Side {
                width: border.width,
                style: border.style as u8,
            };
            for column in 0..column_count {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let segment = Segment {
                    x: grid.x.get(column).copied().unwrap_or(0),
                    y: top as i16,
                    width: grid.frame_width.get(column).copied().unwrap_or(0),
                    height: height as u16,
                    sides: [side; 4],
                    shaded: false,
                };
                if !host.add_box(&segment) {
                    return;
                }
                chapter.pager.added();
            }
        };
        let mut slice_top: i32 = -1;
        for index in 0..max_lines {
            let offset = self
                .table
                .line_offsets
                .get(index)
                .copied()
                .unwrap_or(self.visible_offset);
            let mut row_height = line_height;
            for column in 0..column_count {
                if let Some(line) = self.table.cell_lines.get(column).and_then(|l| l.get(index)) {
                    #[allow(clippy::cast_possible_truncation)]
                    let h = line.height() as i16;
                    row_height = row_height.max(h);
                }
            }
            let top_gap = if slice_top < 0 { max_top + gap } else { 0 };
            let bottom_gap = max_bottom + gap;
            if self
                .pager
                .row_overflows(top_gap + bottom_gap, i32::from(row_height))
            {
                if bordered && slice_top >= 0 {
                    let bottom = i32::from(self.pager.y()) + bottom_gap;
                    frame(self, host, slice_top, bottom);
                }
                slice_top = -1;
                let (pager, mut pages) = self.pages(host);
                pager.break_table_row(&mut pages, offset);
            } else {
                let (pager, mut pages) = self.pages(host);
                pager.ensure_page(&mut pages);
            }
            if !self.pager.has_page() {
                self.clear_cell_lines(host);
                return;
            }
            if slice_top < 0 {
                slice_top = i32::from(self.pager.y());
                #[allow(clippy::cast_possible_truncation)]
                self.pager.set_y((slice_top + gap + max_top) as i16);
            }
            let row_y = i32::from(self.pager.y());
            for column in 0..column_count {
                let Some(line) = self
                    .table
                    .cell_lines
                    .get_mut(column)
                    .and_then(|l| l.get_mut(index))
                else {
                    continue;
                };
                // Placed: the handle goes with the line.
                let placed = *line;
                line.handle = crate::chapter::NO_LINE;
                let line = placed;
                host.set_line_left(line.handle, grid.x.get(column).copied().unwrap_or(0));
                let pad_top = i32::from(cells.get(column).map_or(0, |c| c.padding_top));
                #[allow(clippy::cast_possible_truncation)]
                self.pager.set_y((row_y - max_top + pad_top) as i16);
                self.add_line(host, line, offset, None);
            }
            #[allow(clippy::cast_possible_truncation)]
            self.pager.set_y((row_y + i32::from(row_height)) as i16);
        }
        #[allow(clippy::cast_possible_truncation)]
        self.pager
            .set_y((i32::from(self.pager.y()) + max_bottom).min(page_height) as i16);
        self.pager.clear_keep_with_next();
        if bordered && slice_top >= 0 && self.pager.has_page() {
            #[allow(clippy::cast_possible_truncation)]
            self.pager
                .set_y((i32::from(self.pager.y()) + gap).min(page_height) as i16);
            let bottom = i32::from(self.pager.y());
            frame(self, host, slice_top, bottom);
        } else {
            self.add_row_separator(host);
        }
        self.table.row_stacked = false;
        self.clear_cell_lines(host);
    }

    /// `</table>`: the last row, then flow storage for what follows.
    pub(crate) fn close_table<H: Host>(&mut self, host: &mut H) {
        self.finish_row(host);
        if self.text.as_ref().is_some_and(|t| !t.is_empty()) {
            self.make_pages(host, true, true);
        }
        self.text = None;
        let t = &mut self.table;
        t.depth = 0;
        t.inside_cell = false;
        t.row_stacked = false;
        t.rows_spanned_remaining = 0;
        t.row_text_bytes = 0;
        t.row_cells.clear();
        self.next_word_continues = false;
        let style = self
            .block_stack
            .last()
            .map_or(Block::DEFAULT, BlockExt::without_bottom);
        self.set_text(style);
        self.words_placed = 0;
    }
}
