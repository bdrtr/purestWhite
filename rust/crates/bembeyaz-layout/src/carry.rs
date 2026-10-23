//! Which of a page's last lines move to the next page when the chapter parser (lib/Epub's
//! `ChapterHtmlSlimParser`) breaks it: a paragraph's lone first or last line goes with its
//! neighbours, and a heading's lines go with what follows them.

/// The most lines a break moves, and the most the parser remembers.
pub const MAX_LINES: u8 = 6;

/// The end of the page being broken.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
#[allow(clippy::struct_excessive_bools)]
pub struct Break {
    /// How many of its last elements may move: its trailing text lines the parser still
    /// remembers, never all of its elements; 0 without a page.
    pub movable: u8,
    /// A paragraph is being laid out.
    pub paragraph: bool,
    /// It has a drop cap, whose lines stay.
    pub drop_cap: bool,
    /// The laid-out paragraph's lines on the page.
    pub paragraph_lines: u8,
    /// The line that does not fit is the paragraph's last.
    pub last_line: bool,
    /// The heading lines just before the paragraph, which wait to move with what follows them.
    pub keep_with_next: u8,
}

/// What a break moves: never more than its `movable` lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Carry {
    /// Lines in all.
    pub lines: u8,
    /// Of them, the paragraph's.
    pub paragraph_lines: u8,
}

/// The lines to move when the page breaks before its paragraph's next line, at most
/// `end.movable`: a lone first line or, before a lone last line, the line above it (both when
/// only two are on the page, so neither is left alone); and the heading above when the
/// paragraph's start moves, if it may move too.
#[must_use]
pub fn lines_to_carry(end: &Break) -> Carry {
    let mut paragraph_lines = 0;
    if end.paragraph && !end.drop_cap {
        if end.paragraph_lines == 1 {
            paragraph_lines = 1;
        } else if end.paragraph_lines >= 2 && end.last_line {
            paragraph_lines = if end.paragraph_lines >= 3 { 1 } else { 2 };
        }
    }
    let mut lines = paragraph_lines;
    if end.keep_with_next > 0 && paragraph_lines == end.paragraph_lines {
        lines = lines.saturating_add(end.keep_with_next);
    }
    if lines <= end.movable {
        Carry {
            lines,
            paragraph_lines,
        }
    } else if paragraph_lines <= end.movable {
        Carry {
            lines: paragraph_lines,
            paragraph_lines,
        }
    } else {
        Carry::default()
    }
}

/// The heading lines waiting after a block ends with `paragraph_lines` on the page: a heading's
/// add to those before it, up to [`MAX_LINES`]; any other block lets them stay.
#[must_use]
pub fn keep_after_block(keep: u8, paragraph_lines: u8, heading: bool) -> u8 {
    if heading {
        keep.saturating_add(paragraph_lines).min(MAX_LINES)
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::{Break, Carry, MAX_LINES, keep_after_block, lines_to_carry};

    fn carry(paragraph: bool, lines: u8, last: bool, keep: u8, movable: u8) -> (u8, u8) {
        let c = lines_to_carry(&Break {
            movable,
            paragraph,
            paragraph_lines: lines,
            last_line: last,
            keep_with_next: keep,
            ..Break::default()
        });
        (c.lines, c.paragraph_lines)
    }

    #[test]
    fn paragraph_ends_and_headings_move_together() {
        assert_eq!(carry(true, 1, false, 0, 3), (1, 1));
        assert_eq!(carry(true, 2, true, 0, 3), (2, 2));
        assert_eq!(carry(true, 3, true, 0, 3), (1, 1));
        assert_eq!(carry(true, 3, false, 0, 3), (0, 0));
        assert_eq!(carry(true, 0, true, 0, 3), (0, 0));
        assert_eq!(carry(false, 1, true, 0, 3), (0, 0));
        assert_eq!(carry(true, 1, false, 2, 3), (3, 1));
        assert_eq!(carry(true, 2, true, 2, 3), (2, 2));
        assert_eq!(carry(true, 3, true, 2, 3), (1, 1));
        assert_eq!(carry(false, 0, false, 2, 3), (2, 0));
        assert_eq!(carry(false, 0, false, 4, 3), (0, 0));
        assert_eq!(carry(false, 3, false, 1, 3), (0, 0));
        assert_eq!(carry(true, 1, false, 0, 0), (0, 0));
        assert_eq!(lines_to_carry(&Break::default()), Carry::default());
        // A drop cap's lines stay, and so does the heading above them.
        let drop_cap = Break {
            movable: 3,
            paragraph: true,
            drop_cap: true,
            paragraph_lines: 1,
            keep_with_next: 1,
            ..Break::default()
        };
        assert_eq!(lines_to_carry(&drop_cap), Carry::default());
    }

    #[test]
    fn a_carry_stays_within_what_may_move() {
        for movable in 0..=MAX_LINES {
            for paragraph_lines in [0, 1, 2, 3, 4, 7, 255] {
                for keep in [0, 1, 2, 5, 6, 255] {
                    for flags in 0..8u8 {
                        let end = Break {
                            movable,
                            paragraph: flags & 1 != 0,
                            drop_cap: flags & 2 != 0,
                            paragraph_lines,
                            last_line: flags & 4 != 0,
                            keep_with_next: keep,
                        };
                        let c = lines_to_carry(&end);
                        assert!(
                            c.lines <= movable && c.paragraph_lines <= c.lines,
                            "{end:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn heading_lines_wait() {
        assert_eq!(keep_after_block(1, 2, true), 3);
        assert_eq!(keep_after_block(4, 4, true), MAX_LINES);
        assert_eq!(keep_after_block(250, 250, true), MAX_LINES);
        assert_eq!(keep_after_block(3, 1, false), 0);
    }
}
