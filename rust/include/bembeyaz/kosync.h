// C ABI of rust/crates/bembeyaz-ffi/src/kosync.rs. Use lib/KOReaderSync/ChapterXPathResolver.h and
// ProgressMapper.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_kosync bb_kosync;

// What a resolver looks for in a chapter's XHTML.
enum {
  // The target-th (1-based) <p> or <li>.
  BB_KOSYNC_PARAGRAPH = 0,
  // The target-th (zero-based) visible codepoint.
  BB_KOSYNC_OFFSET = 1,
  // The same, also accepting the text piece that ends at it.
  BB_KOSYNC_OFFSET_INCLUSIVE = 2,
  // Nothing: counts the visible codepoints (target unused).
  BB_KOSYNC_COUNT = 3,
};

// NULL for an unknown kind or when out of memory. spine_index is the chapter's zero-based place in
// the spine; limit bounds the XML parser (BB_XML_DEFAULT_LIMIT). Free it with bb_kosync_free.
bb_kosync* bb_kosync_new(int kind, uint64_t target, uint32_t spine_index, size_t limit);
void bb_kosync_free(bb_kosync* resolver);
// Reads the next len bytes of the chapter, or with last ends it. Input after a failure or once the
// target is found is ignored. BB_XML_OK, or the BB_XML_* error the chapter failed with here.
int bb_kosync_parse(bb_kosync* resolver, const char* data, size_t len, bool last);
// The chapter has failed (or resolver is NULL).
bool bb_kosync_failed(const bb_kosync* resolver);
// The path found (len bytes, not NUL-terminated), or NULL. Valid until the resolver changes.
const char* bb_kosync_xpath(const bb_kosync* resolver, size_t* len);
// For BB_KOSYNC_COUNT: the visible codepoints up to the last one that is not whitespace, so a 100%
// target lands inside real text.
uint64_t bb_kosync_visible_codepoints(const bb_kosync* resolver);

// The other way: a KOReader path in a chapter's raw XHTML.

// One step of a path below the chapter's <body>: a NUL-terminated name and a 1-based sibling
// index (0: any).
typedef struct {
  char tag[12];
  int32_t index;
} bb_kosync_step;

// What a KOReader path says.
typedef struct {
  // The 1-based DocFragment[N], or -1.
  int32_t doc_fragment;
  // The last /p[N], or -1.
  int32_t paragraph;
  // The digits after the first '.' following the last text() (else the last '.'), or 0.
  int32_t char_offset;
  // The N of the last text()[N], at least 1.
  int32_t text_node;
  // The steps between DocFragment[N]/body/ and the text position, at most 16; 0 when the path has none
  // or is malformed.
  int32_t step_count;
  bb_kosync_step steps[16];
  // .../body/text()...: text directly in <body> (and no steps).
  bool body_text;
  // A path to the start of the chapter.
  bool chapter_start;
} bb_kosync_path;

// Reads a path of len bytes into out.
void bb_kosync_parse_xpath(const char* xpath, size_t len, bb_kosync_path* out);

typedef struct bb_kosync_locator bb_kosync_locator;

// What a locator looks for in a chapter.
enum {
  // How many <p> start before byte forward_target (xpath unused, may be NULL).
  BB_KOSYNC_LOCATE_FORWARD = 0,
  // char_offset codepoints into text node text_node of the paragraph-th <p> anywhere.
  BB_KOSYNC_LOCATE_PARAGRAPH = 1,
  // The same in the element the steps name, each step a direct child of the one before, the first of
  // <body>.
  BB_KOSYNC_LOCATE_STEPS = 2,
  // The same with the first step at any depth.
  BB_KOSYNC_LOCATE_STEPS_RELAXED = 3,
  // The same in text directly in <body>.
  BB_KOSYNC_LOCATE_BODY_TEXT = 4,
};

// NULL for an unknown mode, a NULL xpath outside BB_KOSYNC_LOCATE_FORWARD, or when out of memory.
// Free it with bb_kosync_locator_free.
bb_kosync_locator* bb_kosync_locator_new(int mode, const bb_kosync_path* xpath, uint64_t forward_target);
void bb_kosync_locator_free(bb_kosync_locator* locator);
// Reads the next len bytes of the chapter's XHTML.
void bb_kosync_locator_feed(bb_kosync_locator* locator, const uint8_t* data, size_t len);

typedef struct {
  // The position was found; target_visible is its visible codepoint count from the chapter start.
  bool found;
  // For BB_KOSYNC_LOCATE_FORWARD: the <p> count at the target byte, or at the end.
  int32_t paragraph_count;
  // <p> and <li> up to and including the matched element (steps modes).
  int32_t paragraph_at_match;
  int32_t list_item_at_match;
  uint64_t total_visible;
  uint64_t target_visible;
  // The id of the matched element or an <a> in it (anchor_len bytes; steps modes), or NULL. Valid until the
  // locator changes.
  const char* anchor;
  size_t anchor_len;
} bb_kosync_location;

void bb_kosync_locator_get_location(const bb_kosync_locator* locator, bb_kosync_location* out);

#ifdef __cplusplus
}
#endif
