// C ABI of rust/crates/bembeyaz-ffi/src/boxes.rs: the chapter parser's open boxes, elements with a
// CSS border or a background shade, framed on every page their content spans. Use
// lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The most boxes open at once; more nested boxes are not framed.
#define BB_BOXES_MAX 8

// The open boxes (bembeyaz-layout's, 228 bytes): storage that bb_boxes_init sets up. It holds no
// memory of its own.
typedef struct {
  uint32_t opaque[57];
} bb_boxes;

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

// Starts with no box open.
void bb_boxes_init(bb_boxes* boxes);
// Whether BB_BOXES_MAX boxes are open.
bool bb_boxes_full(const bb_boxes* boxes);
// Whether the innermost box belongs to the element at depth.
bool bb_boxes_innermost_is(const bb_boxes* boxes, int depth);

// Opens a box for the element at depth, from its margin edges left and right, unless none is
// left or it is narrower than 8. Only sides with a width and a style are drawn. The border joins
// the element's padding (top, right, bottom, left), which grows to keep at least 4 between the
// content and a drawn edge or shade.
bool bb_boxes_open(bb_boxes* boxes, int depth, const bb_box_side sides[4], bool shaded, int16_t left, int16_t right,
                   int16_t padding[4]);
// Content from top to bottom was placed on the page, inside every open box.
void bb_boxes_note(bb_boxes* boxes, int top, int bottom);
// The page, page_height rows tall, is complete: writes each open box's part on it, outermost
// first, returns how many, and starts the boxes afresh on the next page.
size_t bb_boxes_break(bb_boxes* boxes, uint16_t page_height, bb_box_segment out[BB_BOXES_MAX]);

// The height the innermost box needs of its own: its padding when it has no content and none on
// earlier pages, else 0.
int bb_boxes_empty_height(const bb_boxes* boxes);
// Closes the innermost box and writes its last part on a page page_height rows tall, if it has
// one. When placed, the box was empty and given its bb_boxes_empty_height from top.
bool bb_boxes_close(bb_boxes* boxes, bool placed, int top, uint16_t page_height, bb_box_segment* out);

#ifdef __cplusplus
}
#endif
