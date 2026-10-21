// C ABI of rust/crates/bembeyaz-ffi/src/cpfont.rs. Use lib/EpdFont/SdCardFont.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The .cpfont version bembeyaz-font's cpfont::VERSION reads.
#define BB_CPFONT_FORMAT_VERSION 4
#define BB_CPFONT_MAX_STYLES 4

// Why bb_cpfont_read_header refuses a .cpfont file.
enum {
  BB_CPFONT_HEADER_ENDED = 1,
  BB_CPFONT_NOT_CPFONT = 2,
  // The version is in bb_cpfont_header.version.
  BB_CPFONT_WRONG_VERSION = 3,
  // The count is in bb_cpfont_header.style_count.
  BB_CPFONT_BAD_STYLE_COUNT = 4,
  // The entry's index is in bb_cpfont_header.failed_entry.
  BB_CPFONT_TOC_ENDED = 5,
  // The id is in bb_cpfont_header.failed_entry.
  BB_CPFONT_BAD_STYLE_ID = 6,
  // The style is bb_cpfont_header.failed_entry; its counts are in its entry.
  BB_CPFONT_BAD_COUNTS = 7,
};

// One style's table of contents entry and where its sections start in the file. Offsets wrap at
// 32 bits, as the file's offsets do.
typedef struct {
  uint32_t interval_count;
  uint32_t glyph_count;
  uint32_t intervals_offset;
  uint32_t glyphs_offset;
  uint32_t kern_left_offset;
  uint32_t kern_right_offset;
  uint32_t kern_matrix_offset;
  uint32_t ligatures_offset;
  uint32_t bitmap_offset;
  int16_t ascender;
  int16_t descender;
  uint16_t kern_left_entries;
  uint16_t kern_right_entries;
  uint8_t advance_y;
  uint8_t kern_left_classes;
  uint8_t kern_right_classes;
  uint8_t ligature_pairs;
  bool present;
} bb_cpfont_style;

// A .cpfont file's header and table of contents.
typedef struct {
  // By style id; a later entry with the same id replaces an earlier one.
  bb_cpfont_style styles[BB_CPFONT_MAX_STYLES];
  // FNV-1a over the header and the table of contents.
  uint32_t content_hash;
  uint16_t version;
  bool two_bit;
  uint8_t style_count;
  uint8_t failed_entry;
} bb_cpfont_header;

// Reads the 32-byte header and the 32-byte table of contents entries into header, one entry per
// read call (read fills buf with up to len bytes and returns how many; fewer than len fails), and
// refuses counts no sane font has. Returns 0, a BB_CPFONT_* reason (header keeps what was read so
// far) or -1 for a NULL argument.
int bb_cpfont_read_header(bb_cpfont_header* header, size_t (*read)(void* context, uint8_t* buf, size_t len),
                          void* context);

// Checks a style's interval table record by record. Set up by bb_cpfont_intervals_init.
typedef struct {
  uint32_t opaque[4];
} bb_cpfont_intervals;

// How an interval breaks the table's layout.
typedef struct {
  // first > last; the other fields are false and 0.
  bool reversed;
  bool overlaps;
  bool span_too_big;
  bool offset_mismatch;
  bool offset_overruns;
  // Codepoints in the interval (wrapping).
  uint32_t span;
} bb_cpfont_interval_fault;

// Starts checking the intervals of a style with glyph_count glyphs.
void bb_cpfont_intervals_init(bb_cpfont_intervals* check, uint32_t glyph_count);
// Checks the next interval: sorted, not overlapping the last, starting where the last one's glyphs
// ended, inside the glyphs. False, with fault set when not NULL, when it does not.
bool bb_cpfont_intervals_check(bb_cpfont_intervals* check, uint32_t first, uint32_t last, uint32_t offset,
                               bb_cpfont_interval_fault* fault);
// Every interval checked so far fits the compact 16-bit table.
bool bb_cpfont_intervals_bmp16(const bb_cpfont_intervals* check);

#ifdef __cplusplus
}
#endif
