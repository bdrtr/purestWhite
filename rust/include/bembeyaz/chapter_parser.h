// C ABI of rust/crates/bembeyaz-ffi/src/chapter_parser.rs: the chapter parser
// (rust/crates/bembeyaz-chapter). It reads a chapter's XHTML, builds its paragraphs with their CSS
// and lays them out; the pages, their elements and the laid-out lines are the host's. Use
// lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <bembeyaz/chapter.h>
#include <bembeyaz/css.h>
#include <bembeyaz/layout.h>
#include <bembeyaz/pager.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_chapter bb_chapter;

// How the chapter is laid out.
typedef struct {
  int32_t font;
  float line_compression;
  bool extra_paragraph_spacing;
  uint8_t paragraph_alignment;  // CssTextAlign; None keeps the book's
  uint16_t viewport_width;
  uint16_t viewport_height;
  bool hyphenation;
  bool focus_reading;
  bool embedded_style;
  uint8_t image_rendering;  // 0 images, 1 their alt text, 2 neither
  uint8_t paragraph_indent_spaces;
  int8_t character_spacing;
  uint8_t word_spacing_percent;
  int language;  // bb_hyph_language's index, or -1
  // Frees caches for about that many bytes when a paragraph's word text cannot be stored.
  void (*reclaim)(size_t bytes);
} bb_chapter_settings;

// A laid-out line the host keeps until it is placed or dropped.
typedef struct {
  uint32_t handle;
  uint16_t words;
  // The line height with the extra ascent of taller inline words, and the room ruby takes above.
  int32_t base_height;
  int32_t ruby_shift;
} bb_chapter_line;

// The host. Strings passed to it end with a NUL, except image's path. None of the callbacks may use
// the chapter. The calls that put something on the current page come only while there is one.
typedef struct {
  void* context;
  bb_metrics metrics;
  const bb_css_rules* rules;  // the book's stylesheets, or NULL
  // Starts a new current page; false when it could not be made.
  bool (*new_page)(void* context);
  // Sets aside a next page that takes the current page's last `lines` elements, moved up by top,
  // and the page's notes from top down; false when it could not be made.
  bool (*split)(void* context, uint8_t lines, int16_t top);
  // Hands on the current page, with its first visible character and the <p> and <li> counts so
  // far. The page set aside, if any, becomes current.
  void (*emit)(void* context, uint32_t visible_offset, uint16_t paragraph, uint16_t list_item);
  // Puts a box's part on the current page; false when it could not be made.
  bool (*add_box)(void* context, const bb_box_segment* segment);
  // Makes a laid-out line of a paragraph styled block (a BlockStyle); targets holds each link
  // span's target. False when it could not be made.
  bool (*make_line)(void* context, const bb_line* line, const bb_css_block* block, const char* const* targets,
                    bb_chapter_line* out);
  // Moves a kept line's left margin to x (a table cell's column).
  void (*set_line_left)(void* context, uint32_t line, int16_t x);
  // Puts a kept line on the current page at row y, with its link areas; it is gone either way.
  bool (*push_line)(void* context, uint32_t line, int16_t y);
  void (*drop_line)(void* context, uint32_t line);
  bool (*push_rule)(void* context, int16_t width, uint8_t thickness, int16_t x, int16_t y);
  bool (*push_drop_cap)(void* context, int32_t font, uint16_t scale, bool bold, const char* text, int16_t x, int16_t y);
  // The image last sized by image.
  bool (*push_image)(void* context, int32_t width, int32_t height, int32_t x, int16_t y);
  void (*add_footnote)(void* context, const char* number, const char* href);
  int (*line_height)(void* context, int32_t font, float compression);
  // The renderer's variant of font at scale, or 0.
  int32_t (*font_variant)(void* context, int32_t font, float scale);
  // A glyph's advance (12.4 fixed point) and the top of its ink; false when the font lacks it.
  bool (*glyph)(void* context, int32_t font, uint32_t cp, bool bold, int32_t* advance, int* top);
  // The chapter's next bytes, and whether more follow; nothing with more to come is an error.
  size_t (*read)(void* context, uint8_t* buf, size_t len, bool* more);
  // A second reader at offset of the chapter, for a table's column scan, closed after it.
  bool (*table_open)(void* context, uint64_t offset);
  size_t (*table_read)(void* context, uint8_t* buf, size_t len);
  void (*table_close)(void* context);
  // A word's width in the section font, readied first on an SD font.
  int (*table_word)(void* context, const char* word, bool bold);
  // Sizes the image at path (len bytes, a path in the book) for push_image: 0 when no decoder
  // reads it, 1 when it cannot be sized, 2 with its size written.
  int (*image)(void* context, const char* path, size_t len, int16_t* width, int16_t* height);
} bb_chapter_host;

// NULL when out of memory. Free it with bb_chapter_free.
bb_chapter* bb_chapter_new(const bb_chapter_settings* settings);
void bb_chapter_free(bb_chapter* chapter);
// Replaces the settings; a pager with no page yet takes the new page height.
void bb_chapter_set_settings(bb_chapter* chapter, const bb_chapter_settings* settings);
// The chapter's folder in the book, for image paths; false when it cannot be stored.
bool bb_chapter_set_content_base(bb_chapter* chapter, const char* base, size_t len);
// An id the table of contents starts a chapter at.
void bb_chapter_add_toc_anchor(bb_chapter* chapter, const char* name, size_t len);

// The parse: begin once, step until BB_CHAPTER_DONE (or BB_CHAPTER_ERROR, also when text was lost
// for lack of memory), then finish (false when text was lost) or abort.
enum { BB_CHAPTER_MORE = 0, BB_CHAPTER_DONE = 1, BB_CHAPTER_ERROR = 2 };
bool bb_chapter_begin(bb_chapter* chapter, const bb_chapter_host* host);
int bb_chapter_step(bb_chapter* chapter, const bb_chapter_host* host);
bool bb_chapter_finish(bb_chapter* chapter, const bb_chapter_host* host);
void bb_chapter_abort(bb_chapter* chapter);

// The anchors placed so far (chapter.h's calls read them), valid until the chapter changes.
const bb_anchors* bb_chapter_anchors(const bb_chapter* chapter);

// For the host tests only, which drive the parser event by event.
// The page cursor (pager.h's bb_pager_y reads it), valid until the chapter changes.
const bb_pager* bb_chapter_pager(const bb_chapter* chapter);
// The document's events, one at a time. atts is name, value, ..., NULL.
void bb_chapter_start_element(bb_chapter* chapter, const bb_chapter_host* host, const char* name, const char** atts);
void bb_chapter_text(bb_chapter* chapter, const bb_chapter_host* host, const char* text, size_t len);
void bb_chapter_end_element(bb_chapter* chapter, const bb_chapter_host* host, const char* name);
// Starts the page cursor afresh, with no page (the test took the current page away).
void bb_chapter_restart_pages(bb_chapter* chapter);
// Sets the open table's column widths, as a scan of its markup would (a test has no file).
void bb_chapter_plan_table_columns(bb_chapter* chapter, const uint16_t* widths, size_t count);

#ifdef __cplusplus
}
#endif
