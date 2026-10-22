#pragma once

// The section cache file's header and tables (rust/crates/bembeyaz-section, file module): the
// layout check, the page, anchor, paragraph, list-item and visible-offset lookups, and the
// commit that writes the tables. lib/Epub's Section keeps file handling and the build.

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BB_SECTION_HEADER_SIZE 44
// The format version of finished files, and of a suspended build's partial file.
#define BB_SECTION_VERSION 60
#define BB_SECTION_PARTIAL_VERSION 0xDE

// The section file: read_at fills len bytes from offset, false when it cannot fill them all.
typedef struct {
  void* context;
  uint32_t size;
  bool (*read_at)(void* context, uint32_t offset, uint8_t* buf, size_t len);
} bb_section_file;

// The file being built: write_at writes len bytes at offset, false when it cannot write them all.
typedef struct {
  void* context;
  bool (*write_at)(void* context, uint32_t offset, const uint8_t* bytes, size_t len);
} bb_section_sink;

// The layout settings a section is built with (ReaderRenderSpec's).
typedef struct {
  int32_t font_id;
  float line_compression;
  bool extra_paragraph_spacing;
  uint8_t paragraph_indent_spaces;
  uint8_t paragraph_alignment;
  uint16_t viewport_width;
  uint16_t viewport_height;
  bool hyphenation_enabled;
  bool embedded_style;
  uint8_t image_rendering;
  bool focus_reading_enabled;
  int8_t character_spacing;
  uint8_t word_spacing_percent;
} bb_section_spec;

// A built page: its position in the file, the paragraph and list-item indices it starts in, and
// the visible offset it starts at.
typedef struct {
  uint32_t position;
  uint16_t paragraph;
  uint16_t list_item;
  uint32_t visible;
} bb_section_lut_entry;

// The header a build starts with: the incomplete version, spec, and zero for the rest.
void bb_section_header(const bb_section_spec* spec, uint8_t out[BB_SECTION_HEADER_SIZE]);

enum {
  BB_SECTION_LOADED = 0,
  BB_SECTION_UNKNOWN_VERSION = 1,
  BB_SECTION_OTHER_SPEC = 2,
  // A partial file whose watermark cannot be where it should be.
  BB_SECTION_BAD_PARTIAL = 3,
};

typedef struct {
  uint16_t page_count;
  // A suspended build's file, and how far it had parsed.
  bool partial;
  uint32_t consumed;
  uint32_t total;
  // The version found, for BB_SECTION_UNKNOWN_VERSION.
  uint8_t version;
} bb_section_loaded;

// Checks the file is a finished or partial section built with spec. Returns a BB_SECTION_ code
// and fills *out.
int bb_section_check(const bb_section_file* file, const bb_section_spec* spec, bb_section_loaded* out);

// The lookups below return false when the file does not hold the answer.

// A finished section's page count (a partial's is only how far its build got).
bool bb_section_finished_page_count(const bb_section_file* file, uint16_t* out);
// Where page is, and the visible offset it starts at (0 when the visible LUT lacks it). False
// when its LUT entry cannot be read or points into the header.
bool bb_section_page(const bb_section_file* file, uint16_t page, uint32_t* position, uint32_t* visible);
// The visible offset page starts at.
bool bb_section_visible_offset(const bb_section_file* file, uint16_t page, uint32_t* out);
// The last page starting at or before offset, or with prefer_first the first page starting there.
// A partial file does not know about offsets past its last page's start.
bool bb_section_page_for_visible_offset(const bb_section_file* file, uint32_t offset, bool prefer_first, uint16_t* out);
// The same over the count pages of a build in progress.
bool bb_section_page_for_visible_offset_in(const bb_section_lut_entry* entries, size_t count, uint32_t offset,
                                           bool prefer_first, uint16_t* out);
// The first page whose paragraph (list-item) index reaches target, else the last page.
bool bb_section_page_for_paragraph(const bb_section_file* file, uint16_t target, uint16_t* out);
bool bb_section_page_for_list_item(const bb_section_file* file, uint16_t target, uint16_t* out);
// The paragraph index page starts in.
bool bb_section_paragraph_for_page(const bb_section_file* file, uint16_t page, uint16_t* out);
// The page of the anchor called name (len bytes).
bool bb_section_find_anchor(const bb_section_file* file, const char* name, size_t len, uint16_t* out);
// Hands every anchor whose name and page fit buf (len bytes: the name, then 2 for the page) to
// visit, in order. A name too long for buf is skipped: size it to the longest name sought.
void bb_section_anchors(const bb_section_file* file, uint8_t* buf, size_t len, void* context,
                        void (*visit)(void* context, const char* name, size_t len, uint16_t page));

enum {
  BB_SECTION_COMMITTED = 0,
  // A page the build could not write (position 0).
  BB_SECTION_MISSING_PAGE = 1,
  BB_SECTION_WRITE_FAILED = 2,
};

// How far a suspended build had parsed its HTML.
typedef struct {
  uint32_t consumed;
  uint32_t total;
} bb_section_watermark;

// Finishes a build's file: the tables from at (the end of its pages) with the count entries and
// the anchor_count anchors anchor gives (name and its length, and *page; for a partial file only
// those on its pages), then the header's page count and offsets, and last the version, finished
// or (with a watermark) partial, the commit point. Returns a BB_SECTION_ code.
int bb_section_commit(const bb_section_sink* sink, uint32_t at, const bb_section_lut_entry* entries, size_t count,
                      size_t anchor_count, const char* (*anchor)(void* context, size_t i, size_t* len, uint16_t* page),
                      void* anchor_context, const bb_section_watermark* watermark);

#ifdef __cplusplus
}
#endif
