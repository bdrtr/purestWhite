// C ABI of rust/crates/bembeyaz-ffi/src/css.rs. Use lib/Epub/Epub/css/CssParser.h and
// lib/Epub/Epub/blocks/BlockStyle.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A CssStyle (128 bytes) and a CssAncestor (28 bytes) from CssStyle.h and CssParser.h, whose
// layout CssParser.cpp checks.
typedef struct bb_css_style bb_css_style;
typedef struct bb_css_ancestor bb_css_ancestor;

// The rule store. Rust owns the heap blocks it points to: set it up once with bb_css_rules_init,
// release them with bb_css_rules_free before the storage goes away, and never copy it.
typedef struct {
  void* opaque[12];
} bb_css_rules;

// One stylesheet being read (about 2.2 KB). bb_css_tokenizer_init starts it.
typedef struct {
  uint32_t opaque[549];
} bb_css_tokenizer;

enum {
  BB_CSS_COMPLETE = 0,
  BB_CSS_PARTIAL = 1,
};

// Why new rules stopped being stored.
enum {
  BB_CSS_GROWING = 0,
  BB_CSS_STOPPED_AT_LIMIT = 1,
  BB_CSS_STOPPED_OUT_OF_MEMORY = 2,
};

void bb_css_rules_init(bb_css_rules* rules);
// Frees the rules; the store stays usable, empty.
void bb_css_rules_free(bb_css_rules* rules);
size_t bb_css_rules_len(const bb_css_rules* rules);
bool bb_css_rules_has_first_letter(const bb_css_rules* rules);
int bb_css_rules_stopped(const bb_css_rules* rules);

void bb_css_tokenizer_init(bb_css_tokenizer* tokenizer);
void bb_css_feed(bb_css_rules* rules, bb_css_tokenizer* tokenizer, const char* data, size_t len);
// Ends the stylesheet: BB_CSS_COMPLETE or BB_CSS_PARTIAL.
int bb_css_finish(bb_css_rules* rules, bb_css_tokenizer* tokenizer);
// A selector or declaration was longer than the tokenizer's buffer.
bool bb_css_tokenizer_truncated(const bb_css_tokenizer* tokenizer);

typedef struct {
  const char* tag;
  size_t tag_len;
  const char* classes;
  size_t classes_len;
  const char* id;
  size_t id_len;
  const bb_css_ancestor* ancestors;  // outermost first
  size_t ancestor_count;
  bool first_letter;
} bb_css_query;

void bb_css_resolve(const bb_css_rules* rules, const bb_css_query* query, bb_css_style* out);
void bb_css_parse_inline(const char* style, size_t len, bb_css_style* out);
void bb_css_make_ancestor(const char* tag, size_t tag_len, const char* classes, size_t classes_len, const char* id,
                          size_t id_len, bb_css_ancestor* out);

// The rule cache file. The source reads all of len bytes or fails, reports the bytes left, and
// skips len bytes or fails. A NULL callback fails the call (BB_CSS_CACHE_INVALID, BB_CSS_LOAD_INVALID).
typedef struct {
  void* context;
  bool (*read)(void* context, uint8_t* buf, size_t len);
  size_t (*remaining)(void* context);
  bool (*skip)(void* context, size_t len);
} bb_css_source;

// Matches cache::VERSION in rust/crates/bembeyaz-css/src/cache.rs.
#define BB_CSS_CACHE_VERSION 18

enum {
  BB_CSS_CACHE_COMPLETE = 0,
  BB_CSS_CACHE_PARTIAL = 1,
  BB_CSS_CACHE_INVALID = 2,
};

enum {
  BB_CSS_LOAD_COMPLETE = 0,
  BB_CSS_LOAD_LOW_MEMORY = 1,
  BB_CSS_LOAD_INVALID = 2,
  // Another version wrote the file, or it has no version byte: remove it.
  BB_CSS_LOAD_STALE = 3,
};

// What a cache holds, read from its start: BB_CSS_CACHE_*.
int bb_css_cache_inspect(const bb_css_source* source);
// Replaces the rules with the cache's: BB_CSS_LOAD_*. The rules are empty unless it completes.
int bb_css_cache_load(bb_css_rules* rules, const bb_css_source* source);
// Writes the rules in source order through write; false when write is NULL, a write fails or
// there is no memory to sort them.
bool bb_css_cache_save(const bb_css_rules* rules, bool complete,
                       bool (*write)(void* context, const uint8_t* data, size_t len), void* context);

// The size to show an image natural_width x natural_height pixels large: from css's width and
// height when it sets them (ems of em; a percentage width of container_width, a percentage height
// of page_height), keeping the image's proportions for a side it leaves out, else the image's own
// size; shrunk to fit container_width and page_height, never under a pixel when css sizes it.
// Without a positive natural size, css is ignored. container_width is at least 1.
void bb_css_image_size(const bb_css_style* css, int16_t natural_width, int16_t natural_height, float em,
                       int container_width, uint16_t page_height, int* width, int* height);

// A block's layout style: lib/Epub's BlockStyle, which checks that its layout matches Rust's.
typedef struct bb_css_block bb_css_block;

enum {
  BB_CSS_HORIZONTAL = 1,  // side insets add up; indent and alignment are inherited
  BB_CSS_VERTICAL = 2,    // top and bottom margins collapse, padding adds up, a page break carries
};

// Writes css (NULL: the default style) as a block into out: its lengths in pixels over em (the
// font's line height) and viewport (0 when unknown, so percentages are 0 and a percentage indent
// is left to the paragraph), side insets capped at 2em, and paragraph_alignment (a CssTextAlign;
// None keeps the book's).
void bb_css_block_from_style(const bb_css_style* css, float em, uint8_t paragraph_alignment, uint16_t viewport,
                             bb_css_block* out);
// Puts child inside parent (not the same block) along axis; child's fields on the other axis
// stay, and direction and font size it leaves open come from the parent.
void bb_css_block_combine(const bb_css_block* parent, bb_css_block* child, uint8_t axis);
// Collapses source's bottom into block's (not the same block): the larger margin, the padding
// added.
void bb_css_block_add_bottom(bb_css_block* block, const bb_css_block* source);

#ifdef __cplusplus
}
#endif
