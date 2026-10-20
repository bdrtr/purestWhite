// C ABI of rust/crates/bembeyaz-ffi/src/toc.rs. Use lib/Epub/Epub/parsers/TocNcxParser.h and
// TocNavParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_toc bb_toc;

enum {
  BB_TOC_NCX = 0,  // EPUB 2 NCX
  BB_TOC_NAV = 1,  // EPUB 3 nav document
};

// One entry: a non-empty label, its path resolved against the document's directory and its anchor
// (empty without '#'), and its depth. The bytes are valid during the callback only.
typedef struct {
  const char* label;
  size_t label_len;
  const char* href;
  size_t href_len;
  const char* anchor;
  size_t anchor_len;
  uint8_t level;
} bb_toc_entry;

// A reader for a document in the directory base (with its trailing '/', or empty); NULL when out
// of memory or for an unknown kind. limit bounds the XML parser (BB_XML_DEFAULT_LIMIT).
bb_toc* bb_toc_new(int kind, const char* base, size_t base_len, size_t limit);
void bb_toc_free(bb_toc* toc);
// Reads the next piece; last when nothing follows. A BB_XML_* code: BB_XML_OK, or why the
// document stopped (BB_XML_NO_MEMORY also when an entry could not be held); later calls fail too.
// entry may be NULL; it must not call back into this reader.
int bb_toc_parse(bb_toc* toc, const char* data, size_t len, bool last,
                 void (*entry)(void* context, const bb_toc_entry* entry), void* context);
uint64_t bb_toc_error_offset(const bb_toc* toc);

#ifdef __cplusplus
}
#endif
