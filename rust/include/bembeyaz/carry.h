// C ABI of rust/crates/bembeyaz-ffi/src/carry.rs: which of a page's last lines the chapter
// parser's page break moves to the next page. Use lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h
// instead.
#pragma once

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The most lines a break moves, and the most the parser remembers.
#define BB_CARRY_MAX_LINES 6

// The end of the page being broken.
typedef struct {
  // How many of the page's last elements may move: its trailing text lines the parser still
  // remembers, never all of its elements; 0 without a page.
  uint8_t movable;
  bool paragraph;           // a paragraph is being laid out
  bool drop_cap;            // it has a drop cap, whose lines stay
  uint8_t paragraph_lines;  // the laid-out paragraph's lines on the page
  bool last_line;           // the line that does not fit is the paragraph's last
  uint8_t keep_with_next;   // heading lines just before, waiting to move with what follows
} bb_carry_break;

// What a break moves: lines is never more than the break's movable.
typedef struct {
  uint8_t lines;            // in all
  uint8_t paragraph_lines;  // of them, the paragraph's
} bb_carry;

// The lines to move when the page breaks before its paragraph's next line: a lone first line or,
// before a lone last line, the line above it (both when only two are on the page, so neither is
// left alone); and the heading above when the paragraph's start moves, if it may move too. With
// no paragraph, the heading lines alone, all or none.
void bb_carry_lines(const bb_carry_break* end, bb_carry* out);
// The heading lines waiting after a block ends with paragraph_lines on the page: a heading's add
// to those before it, up to BB_CARRY_MAX_LINES; any other block lets them stay.
uint8_t bb_carry_keep_after_block(uint8_t keep, uint8_t paragraph_lines, bool heading);

#ifdef __cplusplus
}
#endif
