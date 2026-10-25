// C ABI of rust/crates/bembeyaz-ffi/src/pager.rs: the chapter parser's page cursor
// (rust/crates/bembeyaz-layout's pager), and the box parts it puts on pages. Use
// lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The page cursor; bembeyaz-chapter's parser owns it (bb_chapter_pager).
typedef struct bb_pager bb_pager;

// One edge: a CSS width and style (CssBorderStyle, 0 for none), as CssBorderSide.
typedef struct {
  uint8_t width;
  uint8_t style;
} bb_box_side;

// The part of a box on one page, to draw as a PageBorderBox.
typedef struct {
  int16_t x;
  int16_t y;
  uint16_t width;
  uint16_t height;
  bb_box_side sides[4];  // top, right, bottom, left; a side cut by a page break is left out
  bool shaded;
} bb_box_segment;

// The next free row on the current page (for the host tests).
int16_t bb_pager_y(const bb_pager* pager);

#ifdef __cplusplus
}
#endif
