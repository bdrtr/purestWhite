#pragma once

// A page's body in the section cache file (rust/crates/bembeyaz-section, page module): reading it
// into the objects a builder makes, and writing it element by element. lib/Epub's Page and
// TextBlock keep the objects and their drawing.

#include <bembeyaz/section.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BB_PAGE_NUMBER_LEN 32
#define BB_PAGE_HREF_LEN 256
#define BB_PAGE_MAX_FOOTNOTES 16
#define BB_PAGE_MAX_LINKS 32
#define BB_PAGE_MAX_DROP_CAP 12
#define BB_PAGE_WORD_FONTS 3

// A text block's counts.
typedef struct {
  uint16_t word_count;
  bool focus;
  uint16_t text_bytes;
  // The paragraph's first word, or UINT16_MAX.
  uint16_t paragraph_start;
} bb_page_text;

// A line's layout style (BlockStyle's stored fields).
typedef struct {
  uint8_t alignment;
  bool text_align_defined;
  int16_t margin_top;
  int16_t margin_bottom;
  int16_t margin_left;
  int16_t margin_right;
  int16_t padding_top;
  int16_t padding_bottom;
  int16_t padding_left;
  int16_t padding_right;
  int16_t text_indent;
  bool text_indent_defined;
  bool is_rtl;
  bool direction_defined;
  int8_t character_spacing;
  int32_t font_id;
} bb_page_block_style;

typedef struct {
  uint8_t width;
  uint8_t style;
} bb_page_side;

// A footnote: its number and href, NUL-terminated.
typedef struct {
  char number[BB_PAGE_NUMBER_LEN];
  char href[BB_PAGE_HREF_LEN];
} bb_page_footnote;

// A link: its href, NUL-terminated, and its box.
typedef struct {
  char href[BB_PAGE_HREF_LEN];
  int16_t x;
  int16_t y;
  int16_t width;
  int16_t height;
} bb_page_link;

// The bytes of a text block's arena: one array per field, each word_count long, in order: source
// ranges (two uint32), text offsets (uint16), x (int16), [focus suffix x (uint16)], styles (uint8),
// [focus boundaries (uint8)]; then the text, each word NUL-terminated.
size_t bb_page_arena_size(const bb_page_text* text);

// Makes what bb_page_read finds. A callback giving room gives exactly the bytes asked for, or NULL
// when it cannot; one taking an element returns false when it cannot keep it.
typedef struct {
  void* context;
  void (*begin)(void* context, uint16_t count);
  // A line at (x, y): sets *arena to arena_size bytes of room (NULL when arena_size is 0).
  bool (*line)(void* context, int16_t x, int16_t y, const bb_page_text* text, size_t arena_size, uint8_t** arena);
  // Only for a ruby that is not empty, at most once a word, in word order.
  char* (*ruby)(void* context, uint16_t word, size_t len);
  // The current line's font ids (BB_PAGE_WORD_FONTS int32) and a slot per word.
  uint8_t* (*word_fonts)(void* context, size_t size);
  // The current line's style: the line is complete.
  bool (*line_end)(void* context, const bb_page_block_style* style);
  // The next image's cache path (source false) or source href, when not empty.
  char* (*image_text)(void* context, bool source, size_t len);
  bool (*image)(void* context, int16_t x, int16_t y, int16_t width, int16_t height);
  bool (*rule)(void* context, int16_t x, int16_t y, uint16_t width, uint8_t thickness);
  bool (*drop_cap)(void* context, int16_t x, int16_t y, int32_t font_id, uint16_t scale256, uint8_t style,
                   const char* text, size_t len);
  bool (*border_box)(void* context, int16_t x, int16_t y, uint16_t width, uint16_t height, const bb_page_side sides[4],
                     bool shaded);
  // Room for the page's count footnotes and links, which are read into it (only when count > 0).
  bb_page_footnote* (*footnotes)(void* context, size_t count);
  bb_page_link* (*links)(void* context, size_t count);
} bb_page_builder;

// Returned by the reads: BB_PAGE_READ, or why the page is unreadable (see bb_page_error).
enum { BB_PAGE_READ = 0 };

// Reads the page at `at` into builder (which may have been given part of it on failure); on success
// *end is where the page ends.
int bb_page_read(const bb_section_file* file, uint32_t at, const bb_page_builder* builder, uint32_t* end);
// Reads a text block alone (its line at 0, 0).
int bb_page_read_text_block(const bb_section_file* file, uint32_t at, const bb_page_builder* builder, uint32_t* end);
// What a read's code means, for the log.
const char* bb_page_error(int code);

// A text block to write.
typedef struct {
  bb_page_text text;
  // bb_page_arena_size bytes.
  const uint8_t* arena;
  // Each word's ruby (*len bytes; 0 for none).
  const void* ruby_context;
  const char* (*ruby)(const void* context, uint16_t word, size_t* len);
  bb_page_block_style style;
  // The font ids and a slot per word, or NULL when the words use the line's font.
  const uint8_t* word_fonts;
} bb_page_line;

// The writes write at *at and move it past what they wrote; false when a write failed.
bool bb_page_write_count(const bb_section_sink* sink, uint32_t* at, uint16_t count);
bool bb_page_write_line(const bb_section_sink* sink, uint32_t* at, int16_t x, int16_t y, const bb_page_line* line);
bool bb_page_write_text_block(const bb_section_sink* sink, uint32_t* at, const bb_page_line* line);
bool bb_page_write_image(const bb_section_sink* sink, uint32_t* at, int16_t x, int16_t y, const char* path,
                         size_t path_len, const char* source, size_t source_len, int16_t width, int16_t height);
bool bb_page_write_rule(const bb_section_sink* sink, uint32_t* at, int16_t x, int16_t y, uint16_t width,
                        uint8_t thickness);
// At most BB_PAGE_MAX_DROP_CAP bytes of text.
bool bb_page_write_drop_cap(const bb_section_sink* sink, uint32_t* at, int16_t x, int16_t y, int32_t font_id,
                            uint16_t scale256, uint8_t style, const char* text, size_t len);
bool bb_page_write_border_box(const bb_section_sink* sink, uint32_t* at, int16_t x, int16_t y, uint16_t width,
                              uint16_t height, const bb_page_side sides[4], bool shaded);
// The first BB_PAGE_MAX_FOOTNOTES footnotes and BB_PAGE_MAX_LINKS links.
bool bb_page_write_notes(const bb_section_sink* sink, uint32_t* at, const bb_page_footnote* footnotes,
                         size_t footnote_count, const bb_page_link* links, size_t link_count);

#ifdef __cplusplus
}
#endif
