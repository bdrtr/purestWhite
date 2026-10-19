// C ABI of rust/crates/bembeyaz-ffi/src/opf.rs. Use lib/Epub/Epub/parsers/ContentOpfParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct bb_opf bb_opf;

enum {
  // Keep the manifest to resolve the spine through spine_item.
  BB_OPF_SPINE = 1,
  // Stop at the end of the metadata, or at a manifest, spine or guide.
  BB_OPF_METADATA_ONLY = 2,
};

// Called while parsing, with strings that are not NUL-terminated and live until the call returns.
// None may call back into the reader.
typedef struct {
  // The next spine item's href.
  void (*spine_item)(void* context, const char* href, size_t len);
  // A manifest item of type text/css.
  void (*stylesheet)(void* context, const char* href, size_t len);
  // len bytes of text as a finite float, the way strtof reads all of it; false when they are not.
  bool (*parse_float)(void* context, const char* text, size_t len, float* out);
} bb_opf_callbacks;

// What bb_opf_field returns.
enum {
  BB_OPF_TITLE = 0,
  BB_OPF_AUTHOR = 1,
  BB_OPF_LANGUAGE = 2,
  BB_OPF_ISBN = 3,
  BB_OPF_ASIN = 4,
  BB_OPF_SERIES = 5,
  BB_OPF_TOC_NCX = 6,
  BB_OPF_TOC_NAV = 7,
  BB_OPF_COVER = 8,
  BB_OPF_GUIDE_COVER = 9,
  BB_OPF_TEXT_REFERENCE = 10,
};

// What ContentOpfParser lets the manifest keep: ids, hrefs and their index. A package that large
// would hold about 20,000 items.
#define BB_OPF_MANIFEST_LIMIT ((size_t)2 * 1024 * 1024)

// A reader for a package in the directory base (with its trailing '/'); flags are BB_OPF_*. limit
// bounds the XML parser (as bb_xml_new's), manifest_limit the manifest kept with BB_OPF_SPINE. NULL
// when out of memory. Free it with bb_opf_free.
bb_opf* bb_opf_new(const char* base, size_t base_len, uint32_t flags, size_t limit, size_t manifest_limit);
void bb_opf_free(bb_opf* opf);

// Reads the next len bytes; last when nothing follows. Returns 0 or a BB_XML_* error code (xml.h):
// BB_XML_ABORTED when a metadata-only read has all the metadata, BB_XML_NO_MEMORY also when the
// manifest passes its limit (or callbacks is NULL). After an error, every later call returns the
// same code; after last, BB_XML_FINISHED.
int bb_opf_parse(bb_opf* opf, const char* data, size_t len, bool last, const bb_opf_callbacks* callbacks,
                 void* context);

// Where the token that failed starts in the input, in bytes.
uint64_t bb_opf_error_offset(const bb_opf* opf);

// A field as read so far: len bytes, not NUL-terminated, valid until the next bb_opf_parse or
// bb_opf_free.
const char* bb_opf_field(const bb_opf* opf, int field, size_t* len);
// The series index, when the series has one.
bool bb_opf_series_index(const bb_opf* opf, float* out);

#ifdef __cplusplus
}
#endif
