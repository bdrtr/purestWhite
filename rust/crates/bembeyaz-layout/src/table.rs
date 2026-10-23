//! The column widths of `ChapterHtmlSlimParser`'s grid tables: a pre-scan of the table's raw
//! markup measures each column's longest word and longest unwrapped line, then the viewport is
//! shared out the way CSS auto table layout does.
//!
//! The scan is a loose byte scanner, not an HTML parser: tags end at the first `>` outside quotes,
//! only the first 47 bytes of a tag are looked at (lowercased), an entity counts as one `n`, and
//! words longer than 39 bytes are measured in pieces. Cells that span rows or columns take a
//! column but are not measured.
//! It reads at most about 32 KB; a table that does not end by then, or that has more than
//! [`MAX_COLUMNS`] cells in a row, is not planned.
//!
//! [`plan_row`] then places one grid row: its column edges (mirrored right to left), cell frames
//! and padding, or whether it stacks instead.

/// Columns a grid table may have.
pub const MAX_COLUMNS: usize = 4;
/// Bytes read before a table that has not ended is given up.
pub const MAX_SCAN_BYTES: usize = 32 * 1024;

const READ_BYTES: usize = 96;
const WORD_BYTES: usize = 40;
const TAG_BYTES: usize = 48;

/// What the scan found per column, in the caller's width units. `bb_table_measure` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Measure {
    /// The longest word.
    pub min_width: [u16; MAX_COLUMNS],
    /// The longest unwrapped line.
    pub pref_width: [u16; MAX_COLUMNS],
    pub columns: u8,
}

/// Where the markup comes from and how wide text is.
pub trait Source {
    /// Fills the start of `buf` with the next bytes and says how many; 0 at the end.
    fn read(&mut self, buf: &mut [u8]) -> usize;
    /// The advance of a word (its bytes followed by a NUL), in the regular or bold style.
    fn advance(&mut self, word: &[u8], bold: bool) -> i32;
}

fn is_whitespace(c: u8) -> bool {
    matches!(c, b' ' | b'\r' | b'\n' | b'\t')
}

/// Keeps the low 16 bits.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn as_u16(value: i32) -> u16 {
    value as u16
}

#[allow(clippy::struct_excessive_bools)]
struct Scan<'a, S: Source> {
    source: &'a mut S,
    space_width: i32,
    out: Measure,
    word: [u8; WORD_BYTES],
    word_len: usize,
    /// Flushed pieces of a word longer than the buffer.
    word_width: i32,
    tag: [u8; TAG_BYTES],
    tag_len: usize,
    in_tag: bool,
    in_entity: bool,
    quote: u8,
    table_nesting: i32,
    column: i32,
    in_cell: bool,
    skip_cell: bool,
    line_width: i32,
    bold: bool,
}

impl<S: Source> Scan<'_, S> {
    fn column_index(&self) -> Option<usize> {
        usize::try_from(self.column)
            .ok()
            .filter(|&c| c < MAX_COLUMNS)
    }

    fn flush_chunk(&mut self) {
        if self.word_len == 0 {
            return;
        }
        if let Some(nul) = self.word.get_mut(self.word_len) {
            *nul = 0;
        }
        let word = self.word.get(..=self.word_len).unwrap_or_default();
        self.word_width += self.source.advance(word, self.bold);
        self.word_len = 0;
    }

    fn finish_word(&mut self) {
        self.flush_chunk();
        let Some(column) = self.column_index() else {
            return;
        };
        if self.word_width == 0 {
            return;
        }
        if let Some(min) = self.out.min_width.get_mut(column) {
            *min = as_u16(i32::from(*min).max(self.word_width));
        }
        self.line_width += if self.line_width > 0 {
            self.space_width
        } else {
            0
        } + self.word_width;
        self.word_width = 0;
    }

    fn finish_line(&mut self) {
        self.finish_word();
        if let Some(column) = self.column_index()
            && let Some(pref) = self.out.pref_width.get_mut(column)
        {
            *pref = as_u16(i32::from(*pref).max(self.line_width));
        }
        self.line_width = 0;
    }

    /// `Some(true)` when the table ends, `Some(false)` when it cannot be planned.
    fn handle_tag(&mut self) -> Option<bool> {
        let (tag, closing, spans) = self.read_tag();
        if tag == Tag::Table {
            self.table_nesting += if closing { -1 } else { 1 };
            return (self.table_nesting == 0).then_some(true);
        }
        if self.table_nesting != 1 {
            self.finish_word();
            return None;
        }
        match tag {
            Tag::Row if !closing => {
                if self.in_cell {
                    self.finish_line();
                }
                self.in_cell = false;
                self.column = -1;
            }
            Tag::Cell { header } => {
                if self.in_cell {
                    self.finish_line();
                }
                self.in_cell = !closing;
                if !closing {
                    // Spanning cells lay out as stacked rows, so they don't shape the grid.
                    self.skip_cell = spans;
                    self.column += 1;
                    if self.column_index().is_none() {
                        return Some(false);
                    }
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let count = (self.column + 1) as u8;
                    self.out.columns = self.out.columns.max(count);
                    self.bold = header;
                }
            }
            Tag::Break => self.finish_line(),
            // Block markup inside a cell collapses to a word boundary.
            Tag::Block => self.finish_word(),
            _ => {}
        }
        None
    }

    /// The tag just read, whether it closes, and whether it has `colspan` or `rowspan`.
    fn read_tag(&self) -> (Tag, bool, bool) {
        let text = self.tag.get(..self.tag_len).unwrap_or_default();
        // A NUL in the tag ends it.
        let text = text
            .get(..text.iter().position(|&c| c == 0).unwrap_or(text.len()))
            .unwrap_or_default();
        let closing = text.first() == Some(&b'/');
        let rest = text.get(usize::from(closing)..).unwrap_or_default();
        let name_len = rest
            .iter()
            .position(|&c| is_whitespace(c) || c == b'/')
            .unwrap_or(rest.len());
        let tag = match rest.get(..name_len).unwrap_or_default() {
            b"table" => Tag::Table,
            b"tr" => Tag::Row,
            b"td" => Tag::Cell { header: false },
            b"th" => Tag::Cell { header: true },
            b"br" => Tag::Break,
            b"p" | b"div" | b"li" | [b'h', _] => Tag::Block,
            _ => Tag::Other,
        };
        let spans = contains(text, b"colspan") || contains(text, b"rowspan");
        (tag, closing, spans)
    }

    fn byte(&mut self, c: u8) -> Option<bool> {
        if self.in_tag {
            if self.quote != 0 {
                if c == self.quote {
                    self.quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                self.quote = c;
            } else if c == b'>' {
                self.in_tag = false;
                return self.handle_tag();
            }
            if self.tag_len + 1 < TAG_BYTES
                && let Some(slot) = self.tag.get_mut(self.tag_len)
            {
                *slot = c.to_ascii_lowercase();
                self.tag_len += 1;
            }
            return None;
        }
        if c == b'<' {
            self.in_tag = true;
            self.tag_len = 0;
            return None;
        }
        if !self.in_cell || self.skip_cell || self.table_nesting != 1 {
            return None;
        }
        if self.in_entity {
            self.in_entity = c != b';';
            return None;
        }
        if is_whitespace(c) {
            self.finish_word();
            return None;
        }
        if self.word_len + 1 >= WORD_BYTES {
            self.flush_chunk();
        }
        // An entity measures roughly as one character.
        self.in_entity = c == b'&';
        if let Some(slot) = self.word.get_mut(self.word_len) {
            *slot = if self.in_entity { b'n' } else { c };
            self.word_len += 1;
        }
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tag {
    Table,
    Row,
    Cell { header: bool },
    Break,
    Block,
    Other,
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Measures the table whose markup `source` reads, starting at its `<table` tag; `None` when it cannot be planned (fewer than two columns, too many, or no end in reach).
pub fn measure_columns(source: &mut impl Source, space_width: i32) -> Option<Measure> {
    let mut scan = Scan {
        source,
        space_width,
        out: Measure::default(),
        word: [0; WORD_BYTES],
        word_len: 0,
        word_width: 0,
        tag: [0; TAG_BYTES],
        tag_len: 0,
        in_tag: false,
        in_entity: false,
        quote: 0,
        table_nesting: 0,
        column: -1,
        in_cell: false,
        skip_cell: false,
        line_width: 0,
        bold: false,
    };
    let mut buffer = [0u8; READ_BYTES];
    let mut scanned = 0;
    while scanned < MAX_SCAN_BYTES {
        let count = scan.source.read(&mut buffer).min(READ_BYTES);
        if count == 0 {
            return None;
        }
        scanned += count;
        for &c in buffer.get(..count).unwrap_or_default() {
            match scan.byte(c) {
                Some(true) => return (scan.out.columns >= 2).then_some(scan.out),
                Some(false) => return None,
                None => {}
            }
        }
    }
    None
}

/// Shares `available` across the measured columns: natural widths when they fit, otherwise each
/// column keeps its longest word and shares the rest in proportion to how much wrapping it would
/// otherwise need. `padding` is a cell's horizontal padding on each side. `None` for fewer than two
/// columns or more than [`MAX_COLUMNS`].
#[must_use]
pub fn plan_columns(measure: &Measure, available: i32, padding: i32) -> Option<[u16; MAX_COLUMNS]> {
    let columns = usize::from(measure.columns);
    if !(2..=MAX_COLUMNS).contains(&columns) {
        return None;
    }
    // In i64 so the products cannot overflow.
    let padding = i64::from(padding) * 2;
    let available = i64::from(available);
    let mut min_width = [0i64; MAX_COLUMNS];
    let mut pref_width = [0i64; MAX_COLUMNS];
    let mut min_total = 0;
    let mut pref_total = 0;
    for c in 0..columns {
        let (Some(min), Some(pref), Some(&measured_min), Some(&measured_pref)) = (
            min_width.get_mut(c),
            pref_width.get_mut(c),
            measure.min_width.get(c),
            measure.pref_width.get(c),
        ) else {
            return None;
        };
        *min = i64::from(measured_min) + padding;
        *pref = (*min).max(i64::from(measured_pref) + padding);
        min_total += *min;
        pref_total += *pref;
    }
    let mut widths = [0u16; MAX_COLUMNS];
    let mut assigned = 0;
    for c in 0..columns {
        let (Some(&min), Some(&pref), Some(out)) =
            (min_width.get(c), pref_width.get(c), widths.get_mut(c))
        else {
            return None;
        };
        let mut width = if pref_total <= available {
            pref + ((available - pref_total) * pref)
                .checked_div(pref_total)
                .unwrap_or(0)
        } else if min_total <= available {
            min + ((available - min_total) * (pref - min))
                .checked_div(pref_total - min_total)
                .unwrap_or(0)
        } else {
            (available * min).checked_div(min_total).unwrap_or(0)
        };
        if c + 1 == columns {
            width = available - assigned;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            *out = width.max(padding + 1) as u16;
        }
        assigned += width;
    }
    Some(widths)
}

/// A grid row's cell: the padding its block style gives it.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Cell {
    pub padding_top: i16,
    pub padding_bottom: i16,
    pub padding_left: i16,
    pub padding_right: i16,
}

/// Where a grid row's cells go, in logical column order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Row {
    pub width: [u16; MAX_COLUMNS],
    /// The left edge, kept to 16 bits as page positions are.
    pub x: [i16; MAX_COLUMNS],
    /// The cell frame's width: one pixel into the next column, but not past the page.
    pub frame_width: [u16; MAX_COLUMNS],
    /// The largest top padding (at least 0), which the whole row reserves.
    pub top_padding: i16,
    /// The same for the bottom.
    pub bottom_padding: i16,
}

/// Unplanned (equal) columns are at least this many line heights wide, enough for a few glyphs,
/// so ordinary three-column tables stay tabular in portrait; planned columns already fit their
/// longest word.
const MIN_CELL_LINE_HEIGHTS: i32 = 3;

/// Lays out a row of `cells` across `viewport`: in the `planned` widths (one per cell), or sharing
/// it equally;
/// right to left when `rtl`. `None` when the row should stack instead: fewer than two cells (or
/// more than [`MAX_COLUMNS`]), a cell whose padding fills its column, or an unplanned column
/// narrower than three `line_height`s.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn plan_row(
    cells: &[Cell],
    planned: Option<&[u16]>,
    viewport: u16,
    line_height: i16,
    rtl: bool,
) -> Option<Row> {
    let columns = cells.len();
    if !(2..=MAX_COLUMNS).contains(&columns) {
        return None;
    }
    let mut row = Row::default();
    // At most MAX_COLUMNS, so the count fits.
    let equal = viewport / columns as u16;
    for ((cell, width), c) in cells.iter().zip(row.width.iter_mut()).zip(0..) {
        *width = planned.and_then(|p| p.get(c)).copied().unwrap_or(equal);
        row.top_padding = row.top_padding.max(cell.padding_top);
        row.bottom_padding = row.bottom_padding.max(cell.padding_bottom);
        let w = i32::from(*width);
        if w <= i32::from(cell.padding_left) + i32::from(cell.padding_right)
            || (planned.is_none() && w < i32::from(line_height) * MIN_CELL_LINE_HEIGHTS)
        {
            return None;
        }
    }
    // Walk the page from the left; right-to-left rows start with the last logical column.
    let viewport = i32::from(viewport);
    let mut edge = 0;
    for c in 0..columns {
        let c = if rtl { columns - 1 - c } else { c };
        let (Some(&width), Some(x), Some(frame)) = (
            row.width.get(c),
            row.x.get_mut(c),
            row.frame_width.get_mut(c),
        ) else {
            return None;
        };
        let width = i32::from(width);
        *x = edge as i16;
        *frame = if edge + width >= viewport {
            viewport - edge
        } else {
            width + 1
        } as u16;
        edge += width;
    }
    Some(row)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::cast_possible_wrap,
        clippy::cast_possible_truncation,
        clippy::unwrap_used
    )]

    use super::{MAX_COLUMNS, Measure, Source, measure_columns, plan_columns};

    /// Markup read in pieces of `chunk` bytes; a byte is 10 units wide, a bold one 12.
    struct Markup<'a> {
        text: &'a [u8],
        chunk: usize,
    }

    impl Source for Markup<'_> {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let n = self.text.len().min(buf.len()).min(self.chunk);
            buf[..n].copy_from_slice(&self.text[..n]);
            self.text = &self.text[n..];
            n
        }
        fn advance(&mut self, word: &[u8], bold: bool) -> i32 {
            assert_eq!(word.last(), Some(&0));
            let len = word.iter().position(|&c| c == 0).unwrap() as i32;
            len * if bold { 12 } else { 10 }
        }
    }

    fn measure(text: &str) -> Option<Measure> {
        measure_columns(
            &mut Markup {
                text: text.as_bytes(),
                chunk: 96,
            },
            5,
        )
    }

    #[test]
    fn measures_longest_word_and_line_per_column() {
        let m = measure("<table><tr><th>Name</th><td>a bb ccc</td></tr><tr><td>longest</td><td>x<br>yy zz</td></tr></table>")
            .unwrap();
        assert_eq!(m.columns, 2);
        assert_eq!(m.min_width[..2], [70, 30]);
        // "Name" is bold: 4 * 12 = 48 < "longest" 70; "a bb ccc" = 10+5+20+5+30.
        assert_eq!(m.pref_width[..2], [70, 70]);
    }

    #[test]
    fn spanning_cells_nested_tables_and_entities() {
        let m = measure(r#"<table><tr><td colspan="2">wide wide wide</td><td>a&amp;b</td><td><table><tr><td>inner-inner</td></tr></table>z</td></tr></table>"#).unwrap();
        assert_eq!(m.columns, 3);
        assert_eq!(m.min_width[..3], [0, 30, 10]);
    }

    #[test]
    fn too_many_columns_one_column_or_no_end_is_not_planned() {
        assert!(
            measure("<table><tr><td>a</td><td>b</td><td>c</td><td>d</td><td>e</td></tr></table>")
                .is_none()
        );
        assert!(measure("<table><tr><td>a</td></tr></table>").is_none());
        assert!(measure("<table><tr><td>a</td><td>b</td></tr>").is_none());
        let long = std::format!(
            "<table><tr><td>a</td><td>b</td></tr>{}</table>",
            " ".repeat(40 * 1024)
        );
        assert!(
            measure_columns(
                &mut Markup {
                    text: long.as_bytes(),
                    chunk: 96
                },
                5
            )
            .is_none()
        );
    }

    #[test]
    fn the_result_does_not_depend_on_how_reads_are_split() {
        let text = "<table><tr><td>supercalifragilisticexpialidocious-and-more-than-forty-bytes x</td><td>a b</td></tr></table>";
        let whole = measure(text);
        assert!(whole.is_some());
        for chunk in 1..20 {
            assert_eq!(
                measure_columns(
                    &mut Markup {
                        text: text.as_bytes(),
                        chunk
                    },
                    5
                ),
                whole,
                "{chunk}"
            );
        }
    }

    #[test]
    fn plans_like_css_auto_layout() {
        let m = Measure {
            min_width: [20, 40, 0, 0],
            pref_width: [100, 60, 0, 0],
            columns: 2,
        };
        // Natural widths fit: shared in proportion.
        assert_eq!(plan_columns(&m, 400, 4).unwrap()[..2], [245, 155]);
        // Only minimum widths fit.
        assert_eq!(plan_columns(&m, 100, 4).unwrap()[..2], [47, 53]);
        // Not even those.
        assert_eq!(plan_columns(&m, 40, 4).unwrap()[..2], [14, 26]);
        assert!(plan_columns(&Measure { columns: 1, ..m }, 400, 4).is_none());
        assert!(
            plan_columns(
                &Measure {
                    columns: (MAX_COLUMNS + 1) as u8,
                    ..m
                },
                400,
                4
            )
            .is_none()
        );
    }

    #[test]
    fn corrupted_markup_never_panics() {
        let text = b"<table><tr><th a='>'>x y</th><td colspan=2>q</td><td>&lt;&#9;</td></tr><tr><td>1<br/>2</td></tr></table>";
        let mut seed = 0x1234_5678_u32;
        for i in 0..text.len() {
            for _ in 0..6 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let mut copy = text.to_vec();
                copy[i] = seed.to_le_bytes()[0];
                let m = measure_columns(
                    &mut Markup {
                        text: &copy,
                        chunk: 1 + (seed as usize % 9),
                    },
                    5,
                );
                if let Some(m) = m {
                    let _ = plan_columns(&m, (seed % 900) as i32, (seed % 5) as i32);
                }
            }
        }
    }

    #[test]
    fn rows_share_mirror_frame_and_stack() {
        use super::{Cell, plan_row};
        let plain = [Cell::default(); 3];
        let row = plan_row(&plain, None, 480, 16, false).unwrap();
        assert_eq!(row.width[..3], [160, 160, 160]);
        assert_eq!(row.x[..3], [0, 160, 320]);
        assert_eq!(row.frame_width[..3], [161, 161, 160]);
        let rtl = plan_row(&plain, None, 480, 16, true).unwrap();
        assert_eq!(rtl.x[..3], [320, 160, 0]);
        assert_eq!(rtl.frame_width[..3], [160, 161, 161]);
        assert_eq!(
            plan_row(&plain, None, 481, 16, false).unwrap().frame_width[2],
            161
        );
        // Too narrow unless planned.
        assert_eq!(plan_row(&plain, None, 143, 16, false), None);
        assert!(plan_row(&plain, None, 144, 16, false).is_some());
        let planned = [100, 20, 360, 0];
        let row = plan_row(&plain, Some(&planned[..3]), 480, 16, false).unwrap();
        assert_eq!(row.x[..3], [0, 100, 120]);
        // Padding as wide as the column stacks the row.
        let padded = [
            Cell {
                padding_top: 9,
                padding_bottom: 13,
                padding_left: 120,
                padding_right: 119,
            },
            Cell {
                padding_top: 3,
                padding_bottom: -5,
                ..Cell::default()
            },
        ];
        let row = plan_row(&padded, None, 480, 16, false).unwrap();
        assert_eq!((row.top_padding, row.bottom_padding), (9, 13));
        let wide = [
            Cell {
                padding_right: 120,
                ..padded[0]
            },
            padded[1],
        ];
        assert_eq!(plan_row(&wide, None, 480, 16, false), None);
        assert_eq!(plan_row(&plain[..1], None, 480, 16, false), None);
        assert_eq!(plan_row(&[Cell::default(); 5], None, 480, 16, false), None);
    }
}
