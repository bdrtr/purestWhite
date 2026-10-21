// C ABI of rust/crates/bembeyaz-ffi/src/kosync.rs. Use lib/KOReaderSync/ChapterXPathResolver.h
// instead.
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
// target is found is ignored. BB_XML_OK, or the BB_XML_* error the chapter failed with here: with
// last, BB_XML_ABORTED when the target was found only at the end.
int bb_kosync_parse(bb_kosync* resolver, const char* data, size_t len, bool last);
// The chapter has failed (or resolver is NULL).
bool bb_kosync_failed(const bb_kosync* resolver);
// The path found (len bytes, not NUL-terminated), or NULL. Valid until the resolver changes.
const char* bb_kosync_xpath(const bb_kosync* resolver, size_t* len);
// For BB_KOSYNC_COUNT: the visible codepoints up to the last one that is not whitespace, so a 100%
// target lands inside real text.
uint64_t bb_kosync_visible_codepoints(const bb_kosync* resolver);

#ifdef __cplusplus
}
#endif
