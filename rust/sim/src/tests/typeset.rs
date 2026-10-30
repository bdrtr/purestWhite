//! Pages of text: as many as asked, the text within the margins, each page's number at its foot,
//! and the text started over when it runs out.

use crate::typeset::{FILLER, FOOT, SIDE, TOP, typeset};
use purestwhite_render::{HEIGHT, Text, WIDTH, is_black};

fn font() -> Vec<u8> {
    super::font()
}

/// The black pixels of `frame` in the rows `rows` and the columns `columns`.
fn ink(frame: &[u8], rows: std::ops::Range<i32>, columns: std::ops::Range<i32>) -> usize {
    rows.flat_map(|y| columns.clone().map(move |x| (x, y)))
        .filter(|&(x, y)| is_black(frame, x, y))
        .count()
}

#[test]
fn pages_keep_their_text_within_the_margins_and_number_at_the_foot() {
    let bytes = font();
    let mut table = vec![0u8; 8192];
    let mut text = Text::open(&bytes, 0, &mut table).unwrap();
    let pages = typeset(FILLER, &mut text, 3);
    assert_eq!(pages.len(), 3);
    for page in &pages {
        assert!(
            ink(page, TOP..HEIGHT - FOOT, SIDE..WIDTH - SIDE) > 2000,
            "a page of text"
        );
        assert_eq!(ink(page, 0..HEIGHT, 0..SIDE), 0, "the left margin");
        assert_eq!(
            ink(page, 0..HEIGHT, WIDTH - SIDE..WIDTH),
            0,
            "the right margin"
        );
        assert_eq!(ink(page, 0..TOP, 0..WIDTH), 0, "the top margin");
        assert!(
            ink(page, HEIGHT - FOOT..HEIGHT, 0..WIDTH) > 0,
            "the page's number"
        );
    }
    // The pages differ: the text goes on, and their numbers do.
    assert!(pages[0] != pages[1] && pages[1] != pages[2]);
}

#[test]
fn a_short_text_starts_over_and_no_text_gives_numbered_pages() {
    let bytes = font();
    let mut table = vec![0u8; 8192];
    let mut text = Text::open(&bytes, 0, &mut table).unwrap();
    let pages = typeset("One line.", &mut text, 2);
    assert!(ink(&pages[1], TOP..HEIGHT - FOOT, 0..WIDTH) > 0);
    let empty = typeset("", &mut text, 2);
    assert_eq!(ink(&empty[0], 0..HEIGHT - FOOT, 0..WIDTH), 0);
    assert!(ink(&empty[0], HEIGHT - FOOT..HEIGHT, 0..WIDTH) > 0);
}
