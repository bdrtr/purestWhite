#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate std;

use super::{Line, Pager, Pages};
use crate::boxes::{Segment, Side};
use std::vec::Vec;

/// Pages as lists of element tops; `true` marks a text line.
#[derive(Default)]
struct Book {
    current: Option<Vec<(i16, bool)>>,
    aside: Option<Vec<(i16, bool)>>,
    emitted: Vec<(Vec<(i16, bool)>, u32)>,
    boxes: Vec<Segment>,
}

impl Pages for Book {
    fn new_page(&mut self) -> bool {
        self.current = Some(Vec::new());
        true
    }
    fn split(&mut self, lines: u8, top: i16) -> bool {
        let page = self.current.as_mut().expect("a page");
        let tail = page.split_off(page.len() - usize::from(lines));
        self.aside = Some(tail.into_iter().map(|(y, line)| (y - top, line)).collect());
        true
    }
    fn emit(&mut self, visible_offset: u32) {
        self.emitted
            .push((self.current.take().unwrap_or_default(), visible_offset));
        self.current = self.aside.take();
    }
    fn add_box(&mut self, segment: &Segment) {
        self.boxes.push(*segment);
        self.current
            .as_mut()
            .expect("a page")
            .push((segment.y, false));
    }
}

/// Puts a line `height` rows tall through the pager, as the parser does.
fn line(pager: &mut Pager, book: &mut Book, height: i32, offset: u32, end: Line) -> i16 {
    let top = pager.line_top(book, height, offset, end);
    book.current.as_mut().expect("a page").push((top, true));
    pager.line_added(height, offset, end.paragraph);
    top
}

fn paragraph_line(last_line: bool) -> Line {
    Line {
        paragraph: true,
        drop_cap: false,
        last_line,
    }
}

#[test]
fn lines_fill_a_page_and_the_next_starts_at_the_top() {
    let mut book = Book::default();
    let mut pager = Pager::new(48);
    for offset in 0..4 {
        line(&mut pager, &mut book, 16, offset * 10, Line::default());
    }
    assert_eq!(book.emitted.len(), 1);
    assert_eq!(book.emitted[0].0, [(0, true), (16, true), (32, true)]);
    assert_eq!(book.emitted[0].1, 0, "the first page starts at the body");
    assert_eq!(book.current.as_deref(), Some(&[(0, true)][..]));
    assert_eq!(pager.y(), 16);
    assert_eq!(pager.completed(), 1);
    pager.finish(&mut book, 99);
    assert_eq!(
        book.emitted[1].1, 30,
        "the page's first line, not the chapter's end"
    );
}

#[test]
fn a_lone_first_line_moves_with_its_spacing() {
    let mut book = Book::default();
    let mut pager = Pager::new(64);
    line(&mut pager, &mut book, 16, 0, Line::default());
    line(&mut pager, &mut book, 16, 5, Line::default());
    pager.end_block(false);
    pager.advance(8);
    // A new paragraph's first line fits; its second does not, so the first goes along.
    line(&mut pager, &mut book, 16, 10, paragraph_line(false));
    line(&mut pager, &mut book, 16, 20, paragraph_line(false));
    assert_eq!(book.emitted.len(), 1);
    assert_eq!(book.emitted[0].0, [(0, true), (16, true)]);
    assert_eq!(book.current.as_deref(), Some(&[(0, true), (16, true)][..]));
    assert_eq!(pager.y(), 32);
}

#[test]
fn only_trailing_text_lines_move_and_never_a_whole_page() {
    let mut book = Book::default();
    let mut pager = Pager::new(160);
    for offset in 0..3 {
        line(&mut pager, &mut book, 16, offset, Line::default());
    }
    assert_eq!(pager.carry_limit(), 2, "never the whole page");
    for offset in 3..8 {
        line(&mut pager, &mut book, 16, offset, Line::default());
    }
    assert_eq!(pager.carry_limit(), 6, "the most it remembers");
    let top = pager
        .rule_top(&mut book, 4, 10, 0)
        .expect("room for the rule");
    book.current.as_mut().expect("a page").push((top, false));
    pager.rule_added(2, 4, 0);
    assert_eq!(pager.carry_limit(), 0, "a rule ends the run of lines");
    line(&mut pager, &mut book, 16, 9, Line::default());
    assert_eq!(pager.carry_limit(), 1, "only the lines after it");
    pager.finish(&mut book, 0);
    assert_eq!(pager.carry_limit(), 0, "no page");
}

#[test]
fn a_heading_moves_with_the_paragraph_start() {
    let mut book = Book::default();
    let mut pager = Pager::new(72);
    line(&mut pager, &mut book, 16, 0, Line::default());
    line(&mut pager, &mut book, 16, 1, Line::default());
    pager.end_block(false);
    line(&mut pager, &mut book, 16, 2, paragraph_line(true));
    pager.end_block(true);
    assert_eq!(pager.keep_with_next_carry(), 1);
    line(&mut pager, &mut book, 16, 3, paragraph_line(false));
    // The paragraph's second line does not fit: its first and the heading above move.
    line(&mut pager, &mut book, 16, 4, paragraph_line(false));
    assert_eq!(book.emitted[0].0, [(0, true), (16, true)]);
    assert_eq!(
        book.current.as_deref(),
        Some(&[(0, true), (16, true), (32, true)][..])
    );
}

#[test]
fn heading_lines_move_unless_they_are_the_whole_page() {
    let mut book = Book::default();
    let mut pager = Pager::new(160);
    line(&mut pager, &mut book, 16, 0, Line::default());
    pager.end_block(false);
    for offset in 1..3 {
        line(
            &mut pager,
            &mut book,
            16,
            offset,
            paragraph_line(offset == 2),
        );
    }
    pager.end_block(true);
    assert_eq!(pager.keep_with_next_carry(), 2);
    line(&mut pager, &mut book, 16, 3, paragraph_line(true));
    pager.end_block(true);
    assert_eq!(pager.keep_with_next_carry(), 3);
    let mut whole = Book::default();
    let mut pager = Pager::new(160);
    for offset in 0..3 {
        line(
            &mut pager,
            &mut whole,
            16,
            offset,
            paragraph_line(offset == 2),
        );
    }
    pager.end_block(true);
    assert_eq!(pager.keep_with_next_carry(), 0, "never the whole page");
}

#[test]
fn a_drop_cap_keeps_its_lines() {
    let mut book = Book::default();
    let mut pager = Pager::new(64);
    let drop_cap = Line {
        paragraph: true,
        drop_cap: true,
        last_line: false,
    };
    for offset in 0..4 {
        line(&mut pager, &mut book, 16, offset, drop_cap);
    }
    line(&mut pager, &mut book, 16, 4, drop_cap);
    assert_eq!(book.emitted[0].0.len(), 4);
    assert_eq!(book.current.as_deref(), Some(&[(0, true)][..]));
}

#[test]
fn a_box_is_sliced_at_the_break_and_closed_on_the_next_page() {
    let mut book = Book::default();
    let mut pager = Pager::new(48);
    pager.ensure_page(&mut book);
    let side = Side { width: 1, style: 1 };
    let mut padding = [0i16; 4];
    assert!(pager.open_box(1, [side; 4], false, 0, 100, &mut padding));
    for offset in 0..4 {
        line(&mut pager, &mut book, 16, offset, Line::default());
    }
    pager.close_box(&mut book, 0, 0);
    assert_eq!(book.boxes.len(), 2);
    assert_eq!(
        book.boxes[0].sides[2],
        Side::default(),
        "cut open at the break"
    );
    assert_eq!(
        book.boxes[1].sides[0],
        Side::default(),
        "continued from the last page"
    );
    assert_eq!(book.emitted[0].0.last(), Some(&(0, false)));
}

#[test]
fn an_empty_box_gets_its_own_height() {
    let mut book = Book::default();
    let mut pager = Pager::new(100);
    line(&mut pager, &mut book, 16, 0, Line::default());
    let side = Side { width: 2, style: 1 };
    let mut padding = [0i16; 4];
    assert!(pager.open_box(
        1,
        [side, Side::default(), Side::default(), Side::default()],
        false,
        0,
        50,
        &mut padding
    ));
    assert_eq!(pager.box_empty_height(), 6);
    pager.close_box(&mut book, 3, 5);
    assert_eq!(book.boxes.len(), 1);
    assert_eq!(book.boxes[0].y, 19);
    assert_eq!(pager.y(), 16 + 3 + 6 + 5);
}

#[test]
fn an_image_too_tall_for_its_margin_keeps_what_room_is_left() {
    let mut book = Book::default();
    let mut pager = Pager::new(100);
    line(&mut pager, &mut book, 16, 0, Line::default());
    let top = pager.image_top(&mut book, 10, 95, 4, 7).expect("a page");
    assert_eq!(book.emitted.len(), 1);
    assert_eq!(top, 5, "100 - 95 rows left above it");
    book.current.as_mut().expect("a page").push((top, false));
    pager.image_added(95, 4, 7);
    assert_eq!(pager.y(), 104);
    assert!(pager.has_content());
}

#[test]
fn a_page_break_needs_content() {
    let mut book = Book::default();
    let mut pager = Pager::new(100);
    pager.ensure_page(&mut book);
    pager.page_break(&mut book, true);
    assert!(book.emitted.is_empty());
    line(&mut pager, &mut book, 16, 0, Line::default());
    pager.page_break(&mut book, false);
    assert!(book.emitted.is_empty());
    pager.page_break(&mut book, true);
    assert_eq!(book.emitted.len(), 1);
    assert_eq!(pager.y(), 0);
}

#[test]
fn table_rows_break_by_hand_and_separators_need_room() {
    let mut book = Book::default();
    let mut pager = Pager::new(40);
    assert_eq!(pager.row_separator_top(100), None, "no page");
    line(&mut pager, &mut book, 16, 0, Line::default());
    assert_eq!(pager.row_separator_top(0), None, "no width");
    assert_eq!(pager.row_separator_top(100), Some(17));
    pager.row_separator_added();
    assert_eq!(pager.y(), 20);
    assert!(pager.row_overflows(8, 16));
    assert!(!pager.row_overflows(4, 16));
    assert!(pager.break_table_row(&mut book, 3));
    assert_eq!(book.emitted.len(), 1);
    assert_eq!(pager.y(), 0);
    assert!(!pager.has_content());
}
