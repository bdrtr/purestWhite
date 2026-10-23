//! What the chapter parser (lib/Epub's `ChapterHtmlSlimParser`) makes of an element: its kind from
//! its name, its table spans and whether a link stays inside the book. Names match exactly and
//! case-sensitively, except for the elements whose text is hidden.

use core::num::NonZeroU16;

/// The heading level, 1 to 6 (`h1` to `h6`); 0 for other elements.
pub const HEADING_LEVEL: u16 = 0b111;
/// `p`, `li`, `div`, `br`, `blockquote`, `ul`, `ol`, `pre` or `figure`.
pub const BLOCK: u16 = 1 << 3;
/// `b` or `strong`.
pub const BOLD: u16 = 1 << 4;
/// `i` or `em`.
pub const ITALIC: u16 = 1 << 5;
/// `u` or `ins`.
pub const UNDERLINE: u16 = 1 << 6;
/// `del`, `s` or `strike`.
pub const LINE_THROUGH: u16 = 1 << 7;
/// `img` or `image`.
pub const IMAGE: u16 = 1 << 8;
/// `table`, `tr`, `td` or `th`.
pub const TABLE: u16 = 1 << 9;
/// An element whose text is not shown (see [`crate::is_non_visible_element`]).
pub const HIDDEN: u16 = 1 << 10;

/// The named elements and their kinds; a table keeps the code small.
const KINDS: [(&[u8], u16); 30] = [
    (b"h1", 1),
    (b"h2", 2),
    (b"h3", 3),
    (b"h4", 4),
    (b"h5", 5),
    (b"h6", 6),
    (b"p", BLOCK),
    (b"li", BLOCK),
    (b"div", BLOCK),
    (b"br", BLOCK),
    (b"blockquote", BLOCK),
    (b"ul", BLOCK),
    (b"ol", BLOCK),
    (b"pre", BLOCK),
    (b"figure", BLOCK),
    (b"b", BOLD),
    (b"strong", BOLD),
    (b"i", ITALIC),
    (b"em", ITALIC),
    (b"u", UNDERLINE),
    (b"ins", UNDERLINE),
    (b"del", LINE_THROUGH),
    (b"s", LINE_THROUGH),
    (b"strike", LINE_THROUGH),
    (b"img", IMAGE),
    (b"image", IMAGE),
    (b"table", TABLE),
    (b"tr", TABLE),
    (b"td", TABLE),
    (b"th", TABLE),
];

/// The kind of the element named `name`: a heading level and flags.
#[must_use]
pub fn kind(name: &[u8]) -> u16 {
    let kind = KINDS
        .iter()
        .find(|&&(n, _)| n == name)
        .map_or(0, |&(_, kind)| kind);
    if crate::is_non_visible_element(name) {
        kind | HIDDEN
    } else {
        kind
    }
}

/// How many rows or columns a table cell spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    Cells(NonZeroU16),
    /// The rest of the table.
    Rest,
}

/// A `colspan` or `rowspan` value: decimal digits only, else one cell; 0, or digits passing
/// `u16::MAX` before anything else, mean the rest of the table.
#[must_use]
pub fn table_span(value: &[u8]) -> Span {
    if value.is_empty() {
        return Span::Cells(NonZeroU16::MIN);
    }
    let mut span: u16 = 0;
    for &byte in value {
        if !byte.is_ascii_digit() {
            return Span::Cells(NonZeroU16::MIN);
        }
        let Some(next) = span
            .checked_mul(10)
            .and_then(|s| s.checked_add(u16::from(byte - b'0')))
        else {
            return Span::Rest;
        };
        span = next;
    }
    NonZeroU16::new(span).map_or(Span::Rest, Span::Cells)
}

/// Links with these schemes leave the book.
const EXTERNAL: [&[u8]; 6] = [
    b"http://",
    b"https://",
    b"mailto:",
    b"ftp://",
    b"tel:",
    b"javascript:",
];

/// Whether `href` points inside the book: not empty, and with none of the external schemes.
#[must_use]
pub fn is_internal_link(href: &[u8]) -> bool {
    !href.is_empty() && !EXTERNAL.iter().any(|scheme| href.starts_with(scheme))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::{
        BLOCK, BOLD, HEADING_LEVEL, HIDDEN, IMAGE, ITALIC, LINE_THROUGH, Span, TABLE, UNDERLINE,
        is_internal_link, kind, table_span,
    };
    use core::num::NonZeroU16;

    #[test]
    fn names_match_exactly() {
        for (level, name) in [b"h1", b"h2", b"h3", b"h4", b"h5", b"h6"]
            .iter()
            .enumerate()
        {
            assert_eq!(usize::from(kind(*name) & HEADING_LEVEL), level + 1);
        }
        for name in [&b"h7"[..], b"h", b"h10", b"H1", b"", b"hr"] {
            assert_eq!(kind(name), 0, "{name:?}");
        }
        assert_eq!(kind(b"figure"), BLOCK);
        assert_eq!(kind(b"P"), 0);
        assert_eq!(kind(b"strong"), BOLD);
        assert_eq!(kind(b"em"), ITALIC);
        assert_eq!(kind(b"ins"), UNDERLINE);
        assert_eq!(kind(b"strike"), LINE_THROUGH);
        assert_eq!(kind(b"image"), IMAGE);
        assert_eq!(kind(b"IMG"), 0);
        assert_eq!(kind(b"th"), TABLE);
        assert_eq!(kind(b"thead"), 0);
        // Hidden elements match in any case.
        assert_eq!(kind(b"HEAD"), HIDDEN);
        assert_eq!(kind(b"rp"), HIDDEN);
    }

    #[test]
    fn spans_are_digits_only() {
        let cells = |n| Span::Cells(NonZeroU16::new(n).unwrap());
        assert_eq!(table_span(b""), cells(1));
        assert_eq!(table_span(b"007"), cells(7));
        assert_eq!(table_span(b"65535"), cells(65535));
        assert_eq!(table_span(b"0"), Span::Rest);
        assert_eq!(table_span(b"65536"), Span::Rest);
        assert_eq!(table_span(b"99999999999"), Span::Rest);
        assert_eq!(table_span(b"99999999999x"), Span::Rest);
        assert_eq!(table_span(b"6553x"), cells(1));
        assert_eq!(table_span(b"1a"), cells(1));
        assert_eq!(table_span(b"-1"), cells(1));
        assert_eq!(table_span(b" 2"), cells(1));
    }

    #[test]
    fn links_inside_the_book() {
        assert!(!is_internal_link(b""));
        assert!(is_internal_link(b"#note"));
        assert!(!is_internal_link(b"https://x"));
        assert!(!is_internal_link(b"javascript:void(0)"));
        assert!(is_internal_link(b"HTTP://x"));
        assert!(is_internal_link(b"ftp:x"));
        // A scheme is its whole prefix.
        assert!(is_internal_link(b"telephone.xhtml"));
        assert!(is_internal_link(b"https:/x"));
    }
}
