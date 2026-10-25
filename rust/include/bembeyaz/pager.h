// C ABI of rust/crates/bembeyaz-ffi/src/pager.rs: the chapter parser's page cursor. It places what
// the parser lays out (the next free row, page breaks and the lines they carry to the next page,
// each page's first visible character) and frames the open boxes; the pages and their elements
// stay the parser's, behind bb_pages. Use lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The most boxes open at once; more nested boxes are not framed.
#define BB_BOXES_MAX 8

// The page cursor (bembeyaz-layout's, 284 bytes): storage that bb_pager_init sets up. It holds no
// memory of its own.
typedef struct {
  uint32_t opaque[71];
} bb_pager;

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

// The caller's pages. One is current while content is placed; a page break may set the next one
// aside before the current one is handed on. The callbacks must not use the pager.
typedef struct {
  void* context;
  // Starts a new current page; false when it could not be made.
  bool (*new_page)(void* context);
  // Sets aside a next page that takes the current page's last `lines` elements, moved up by `top`,
  // and the page's notes from `top` down; false when it could not be made.
  bool (*split)(void* context, uint8_t lines, int16_t top);
  // Hands on the current page, whose first visible character is visible_offset. The page set
  // aside by split, if any, becomes current; else there is none.
  void (*emit)(void* context, uint32_t visible_offset);
  // Puts a box's part on the current page.
  void (*add_box)(void* context, const bb_box_segment* segment);
} bb_pages;

// The paragraph whose line is being placed.
typedef struct {
  bool paragraph;  // a paragraph being laid out, not a table cell
  bool drop_cap;   // with a drop cap, whose lines stay on its page
  bool last_line;  // the line is the paragraph's last
} bb_pager_line;

// No page yet, for pages page_height rows tall.
void bb_pager_init(bb_pager* pager, uint16_t page_height);

// The next free row, which a table row's lines are placed at by hand.
int16_t bb_pager_y(const bb_pager* pager);
void bb_pager_set_y(bb_pager* pager, int16_t y);
// Moves it down by rows (block spacing), wrapping at 16 bits.
void bb_pager_advance(bb_pager* pager, int rows);
// The pages handed on so far.
uint16_t bb_pager_completed(const bb_pager* pager);
bool bb_pager_has_page(const bb_pager* pager);
// Whether the current page holds anything.
bool bb_pager_has_content(const bb_pager* pager);

// Content from top to bottom was placed, inside every open box.
void bb_pager_note(bb_pager* pager, int top, int bottom);
// The caller put an element other than a text line on the current page.
void bb_pager_added(bb_pager* pager);
// The current page's first visible character, unless it has one; the first page's is 0.
void bb_pager_set_visible_offset(bb_pager* pager, uint32_t offset);

// Starts a page when none is current.
void bb_pager_ensure_page(bb_pager* pager, const bb_pages* pages);
// Completes the current page and starts the next.
void bb_pager_complete_page(bb_pager* pager, const bb_pages* pages);
// Completes the current page if requested and it holds anything.
void bb_pager_page_break(bb_pager* pager, const bb_pages* pages, bool requested);
// Hands on the chapter's last page, whose first visible character is offset if it has none yet.
void bb_pager_finish(bb_pager* pager, const bb_pages* pages, uint32_t offset);

// The heading lines at the page's end that may move with what follows them.
uint8_t bb_pager_keep_with_next_carry(const bb_pager* pager);
// Completes the page, moving its last `lines` text lines (paragraph_lines of them the laid-out
// paragraph's) to the top of the next with their spacing; false when no page could be made.
bool bb_pager_break_carrying(bb_pager* pager, const bb_pages* pages, uint8_t lines, uint8_t paragraph_lines,
                             uint32_t offset);

// Where the next text line, height rows tall and showing offset first, goes: when it does not fit,
// the page breaks first, its paragraph's lone lines and a heading above going along. Put the line
// there, then report it with bb_pager_line_added.
int16_t bb_pager_line_top(bb_pager* pager, const bb_pages* pages, int height, uint32_t offset,
                          const bb_pager_line* line);
void bb_pager_line_added(bb_pager* pager, int height, uint32_t offset, bool paragraph);
// A block ended: a heading's lines on the page wait to move with what follows it.
void bb_pager_end_block(bb_pager* pager, bool heading);
// The page's last heading lines may stay where they are.
void bb_pager_clear_keep_with_next(bb_pager* pager);
// An empty line on a page that holds something, up to the page's end.
void bb_pager_blank_line(bb_pager* pager, int line_height);

// Where a drop cap spanning height rows starts: on a new page, with the heading above, when it
// does not fit. Report it with bb_pager_added and bb_pager_note; bb_pager_reach moves the next
// free row below its spanned lines.
int16_t bb_pager_drop_cap_top(bb_pager* pager, const bb_pages* pages, int height, uint32_t offset);
void bb_pager_reach(bb_pager* pager, int y);

// Where a horizontal rule goes after top rows of spacing, on a new page when it and its spacing
// (total rows) do not fit; false when no page could be made. Report it with bb_pager_rule_added.
bool bb_pager_rule_top(bb_pager* pager, const bb_pages* pages, int16_t top, int16_t total, uint32_t offset,
                       int16_t* out);
void bb_pager_rule_added(bb_pager* pager, int16_t thickness, int16_t bottom, uint32_t offset);

// Where a table row's separator goes, if one fits on a page that holds something. Report it with
// bb_pager_row_separator_added.
bool bb_pager_row_separator_top(const bb_pager* pager, uint16_t page_width, int16_t* out);
void bb_pager_row_separator_added(bb_pager* pager);
// Whether a table row line height rows tall, with gaps rows around it, overflows a page that holds
// something.
bool bb_pager_row_overflows(const bb_pager* pager, int gaps, int height);
// Completes the page within a table row whose next line shows offset first, and starts the next;
// false when no page could be made.
bool bb_pager_break_table_row(bb_pager* pager, const bb_pages* pages, uint32_t offset);

// Where an image height rows tall goes, with margin_top and margin_bottom around it: on a new page
// when it does not fit (with the heading above when both fit there), with less top margin when
// even then it has no room. False when no page could be made. Report it with bb_pager_image_added.
bool bb_pager_image_top(bb_pager* pager, const bb_pages* pages, int16_t margin_top, int height,
                        int16_t margin_bottom, uint32_t offset, int16_t* out);
void bb_pager_image_added(bb_pager* pager, int height, int16_t margin_bottom, uint32_t offset);

// Whether BB_BOXES_MAX boxes are open.
bool bb_pager_box_full(const bb_pager* pager);
// Whether the innermost box belongs to the element at depth.
bool bb_pager_box_innermost_is(const bb_pager* pager, int depth);
// Opens a box for the element at depth, from its margin edges left and right, unless none is left
// or it is narrower than 8. Only sides with a width and a style are drawn. The border joins the
// element's padding (top, right, bottom, left), which grows to keep at least 4 between the content
// and a drawn edge or shade.
bool bb_pager_open_box(bb_pager* pager, int depth, const bb_box_side sides[4], bool shaded, int16_t left,
                       int16_t right, int16_t padding[4]);
// The height the innermost box needs of its own: its padding when it has no content and none on
// earlier pages, else 0.
int bb_pager_box_empty_height(const bb_pager* pager);
// Closes the innermost box. An empty one gets bb_pager_box_empty_height after above rows (on a new
// page, without them, when it does not fit), with below rows under it.
void bb_pager_close_box(bb_pager* pager, const bb_pages* pages, int above, int below);

#ifdef __cplusplus
}
#endif
