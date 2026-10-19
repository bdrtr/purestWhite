// C ABI of rust/crates/bembeyaz-ffi/src/layout.rs. Use lib/Epub/Epub/ParsedText.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_paragraph bb_paragraph;

// The renderer's glyph metrics. Styles are EpdFontFamily::Style bits; text is NUL-terminated.
// None may call back into the paragraph.
typedef struct {
  void* context;
  int (*text_advance)(void* context, int font, const char* text, uint8_t style, int8_t spacing);
  int (*kerning)(void* context, int font, uint32_t left, uint32_t right, uint8_t style, int8_t spacing);
  int (*space_advance)(void* context, int font, uint32_t left, uint32_t right, uint8_t style);
  int (*space_width)(void* context, int font, uint8_t style);
  int (*ascender)(void* context, int font);
  bool (*is_sd_font)(void* context, int font);
  // Loads the advances of every codepoint of the segments (each lens[i] bytes of NUL-separated
  // words) for an SD-card font.
  void (*prewarm)(void* context, int font, const char* const* segments, const size_t* lens, size_t count,
                  bool include_space, bool include_hyphen, uint8_t style_mask, bool small_caps);
} bb_metrics;

// What layout reads of the BlockStyle; it may set is_rtl. align is CssTextAlign.
typedef struct {
  uint8_t align;
  bool text_align_defined;
  bool direction_defined;
  bool is_rtl;
  bool text_indent_defined;
  int16_t text_indent;
} bb_block_style;

typedef struct {
  int32_t font;
  // The font of each size slot (slot 0: the paragraph's), slot_count of them in use.
  int32_t slot_fonts[4];
  uint8_t slot_count;
  uint16_t width;
  bool include_last_line;
  int8_t character_spacing;
  uint8_t word_spacing;
  size_t max_lines;
  // bb_hyph_language's index, or -1.
  int language;
  // Link ids above this are dropped.
  size_t link_count;
} bb_layout_params;

typedef struct {
  uint8_t link;  // 1-based link id
  int16_t x;
  int16_t width;
  int16_t top_lift;
} bb_link_span;

// A laid-out line in visual order, valid while the callback runs. Word i is the NUL-terminated
// string at text + word_starts[i]; its ruby (logical order) is at ruby_text + ruby_starts[i].
// ranges holds a start and an end per word. focus and suffix_x are NULL when no word has a bold
// prefix, word_slots when every word uses the block font.
typedef struct {
  const char* text;
  const uint32_t* word_starts;
  size_t count;
  const int16_t* xpos;
  const uint8_t* styles;
  const uint8_t* focus;
  const uint16_t* suffix_x;
  const char* ruby_text;
  const uint32_t* ruby_starts;
  const bb_link_span* links;
  size_t link_count;
  const uint32_t* ranges;
  uint16_t paragraph_start;
  const uint8_t* word_slots;
  int32_t word_fonts[3];  // the fonts of slots 1-3
  uint32_t visible_offset;
  int32_t lines_after;  // -1 when the rest of the paragraph is not laid out yet
  bool is_rtl;
} bb_line;

// NULL when out of memory. Free it with bb_paragraph_free. reclaim (or NULL) frees caches for about
// that many bytes when a 2 KB chunk of word text cannot be allocated; the allocation is then tried
// once more.
bb_paragraph* bb_paragraph_new(bool hyphenation, bool focus_reading, uint8_t indent_spaces, void (*reclaim)(size_t));
void bb_paragraph_free(bb_paragraph* paragraph);

void bb_paragraph_add_word(bb_paragraph* paragraph, const char* word, size_t len, uint8_t style, bool underline,
                           bool attach_to_previous, uint32_t offset, uint8_t link, uint8_t slot, bool no_space_before);
void bb_paragraph_set_ruby(bb_paragraph* paragraph, size_t index, const char* ruby, size_t len);
void bb_paragraph_set_ruby_group(bb_paragraph* paragraph, size_t start, size_t count, const char* ruby, size_t len);
size_t bb_paragraph_len(const bb_paragraph* paragraph);
// A word or a line was dropped for lack of memory (or the paragraph is NULL): the paragraph is
// incomplete.
bool bb_paragraph_dropped_words(const bb_paragraph* paragraph);
uint8_t bb_paragraph_word_style(const bb_paragraph* paragraph, size_t index);
// len bytes, valid until the paragraph next changes.
const char* bb_paragraph_ruby(const bb_paragraph* paragraph, size_t index, size_t* len);
void bb_paragraph_suppress_first_line_indent(bb_paragraph* paragraph);
void bb_paragraph_reset_first_line_indent(bb_paragraph* paragraph);

// Lays out the paragraph, handing each line to line (which returns false when it could not keep
// it), and drops the words used. line must not call bb_paragraph_* on this paragraph.
void bb_paragraph_layout(bb_paragraph* paragraph, const bb_metrics* metrics, bb_block_style* style,
                         const bb_layout_params* params, bool (*line)(void* context, const bb_line* line),
                         void* context);

#ifdef __cplusplus
}
#endif
