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

// The chapter parser's inline style stack. One open element's inline style: each value applies only
// with its has_ flag; enums are CssStyle.h's (decoration a set of CssTextDecoration bits).
typedef struct {
  int32_t depth;
  bool has_bold;
  bool bold;
  bool has_italic;
  bool italic;
  bool has_decoration;
  uint8_t decoration;
  bool has_direction;
  uint8_t direction;
  // The direction is the paragraph's too (a block, or html and body).
  bool sets_paragraph_direction;
  bool has_text_align;
  uint8_t text_align;
  bool has_sup;
  bool sup;
  bool has_sub;
  bool sub;
  bool has_small_caps;
  bool small_caps;
  bool has_white_space;
  bool preserve_whitespace;
  // A size: a multiple of the parent's, or of the body's when font_scale_rem.
  bool has_font_scale;
  bool font_scale_rem;
  float font_scale;
} bb_css_inline_entry;

// The element an entry is for.
enum {
  // A block: pushed only when it sets a text property; sized_ancestor: an open entry has a size,
  // so the block's resolved scale is pushed too, as a rem size.
  BB_CSS_ENTRY_BLOCK = 0,
  // u, s and the like: their line (decoration) unless the style sets another.
  BB_CSS_ENTRY_DECORATION = 1,
  // A footnote link: underlined.
  BB_CSS_ENTRY_LINK = 2,
  BB_CSS_ENTRY_BOLD = 3,
  BB_CSS_ENTRY_ITALIC = 4,
  BB_CSS_ENTRY_SUP = 5,
  BB_CSS_ENTRY_SUB = 6,
  // A span or other inline element: pushed only when it sets a text property; sets_paragraph for
  // html and body; in_table: its text-align counts; scale: the tag's own size (small, big), 0 for
  // none.
  BB_CSS_ENTRY_INLINE = 7,
};

// The element's kind and what its declarations alone do not say: decoration for DECORATION;
// sized_ancestor and scale (the block's resolved scale) for BLOCK; sets_paragraph, in_table and
// scale (the tag's own size) for INLINE.
typedef struct {
  uint8_t kind;
  uint8_t decoration;
  bool sized_ancestor;
  bool sets_paragraph;
  bool in_table;
  float scale;
} bb_css_entry_kind;

// The entry an element at depth with style css pushes into out; false when it pushes none.
bool bb_css_make_inline_entry(const bb_css_entry_kind* kind, const bb_css_style* css, int32_t depth,
                              bb_css_inline_entry* out);

// The style words are set in, and the paragraph's direction.
typedef struct {
  bool bold;
  bool italic;
  uint8_t decoration;
  bool small_caps;
  bool preserve_whitespace;
  bool direction_defined;
  uint8_t direction;
  bool text_align_defined;
  uint8_t text_align;
  bool sup;
  bool sub;
  bool paragraph_direction_defined;
  bool paragraph_rtl;
} bb_css_effective;

// Folds the count open entries, in order, over the block's style css and the paragraph direction
// it starts from: a line decoration, once on, stays on; sup and sub replace each other.
void bb_css_fold_inline(const bb_css_style* css, bool block_direction_defined, bool block_rtl,
                        const bb_css_inline_entry* stack, size_t count, bb_css_effective* out);

// The size a word is set in: each of the count sized open entries multiplies the size so far
// (block_scale until that is positive) or, when rem, replaces it; 0 when no entry is sized.
float bb_css_word_scale(const bb_css_inline_entry* stack, size_t count, float block_scale);
// A block's size: the count open entries over its parent block's size, then css's font size (a
// multiple of that, or rem), else a heading's default (heading 1 to 6, 0 for none); within 0.5 to 3.
float bb_css_block_scale(const bb_css_style* css, uint8_t heading, float parent, const bb_css_inline_entry* stack,
                         size_t count);
// The size of a span that holds a whole block: css's font size over base (the block's, or the
// enclosing such spans'), a multiple of it or rem, base when it sets none; within 0.5 to 3.
float bb_css_span_scale(const bb_css_style* css, float base);
// The browser's default size of the len-byte inline element name (small, big) as a multiple of
// its parent's; 0 for other names.
float bb_css_tag_scale(const char* name, size_t len);

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
