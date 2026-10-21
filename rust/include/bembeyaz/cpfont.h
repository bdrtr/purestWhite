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

// The font file: read fills buf with up to len bytes from the current position and returns how
// many; seek moves to a position and returns false when it cannot. context is passed to them.
typedef struct {
  size_t (*read)(void* context, uint8_t* buf, size_t len);
  bool (*seek)(void* context, uint32_t position);
  void* context;
} bb_cpfont_file;

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

// Reads the 32-byte header and the 32-byte table of contents entries into header from file's
// current position, one read per entry (a short read fails), and refuses counts no sane font has.
// Returns 0, a BB_CPFONT_* reason (header keeps what was read so far) or -1 for a NULL argument.
int bb_cpfont_read_header(bb_cpfont_header* header, bb_cpfont_file* file);

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

// Why bb_cpfont_build_mini_kern fails.
enum {
  BB_CPFONT_KERN_CLASS_READ = 1,
  // reserve gave no tables; bb_cpfont_mini_kern says what was asked for.
  BB_CPFONT_KERN_RESERVE = 2,
  // A matrix row could not be sought or read; failed_row is its left class (font numbering).
  BB_CPFONT_KERN_ROW_SEEK = 3,
  BB_CPFONT_KERN_ROW_READ = 4,
};

// A page's kern matrix: only the class pairs its codepoints reach, renumbered from 1.
typedef struct {
  // left_classes * right_classes.
  uint32_t matrix_bytes;
  // Page codepoints with a left class, then with a right class.
  uint16_t left_entries;
  uint16_t right_entries;
  // Classes in the matrix; 0 when the page has no kern pairs.
  uint8_t left_classes;
  uint8_t right_classes;
  // See BB_CPFONT_KERN_ROW_SEEK.
  uint8_t failed_row;
} bb_cpfont_mini_kern;

// Entries of a style's kern block index: the first codepoint of each 64-entry block of its left
// class table, then that table's last codepoint, then the same for the right table. 0 for NULL.
size_t bb_cpfont_kern_index_len(const bb_cpfont_style* style);
// Scratch bytes bb_cpfont_build_mini_kern needs for count page codepoints. 0 for NULL.
size_t bb_cpfont_mini_kern_scratch_len(const bb_cpfont_style* style, size_t count);

// Builds the kern matrix for count page codepoints (ascending) from style's class tables and
// matrix, using scratch (bb_cpfont_mini_kern_scratch_len bytes).
//
// index is NULL or the style's block index (bb_cpfont_kern_index_len entries). When *index_ready
// is false every block is read while index, if any, is being filled in, and otherwise until the
// page's codepoints are used up; when it is true only the blocks a page codepoint can be in are
// read. Once both tables are read, *index_ready
// becomes (index != NULL); it is left unchanged when a table cannot be read.
//
// reserve is called at most once, when the page has both left and right classes, with the sizes
// in kern. It sets *left and *right to at least 3 * left_entries and 3 * right_entries bytes
// (uint16 little-endian codepoint, then uint8 class, as in the file) and *matrix to at least
// matrix_bytes bytes, none of them overlapping scratch, codepoints or index; it returns false,
// or leaves a pointer NULL, when it cannot.
//
// Returns 0 with result set (all 0 when the page has no kern pairs) or a BB_CPFONT_KERN_* reason
// with result saying more. Returns -1, without writing result, when file, style, index_ready,
// reserve or result is NULL, codepoints or scratch is NULL with a length, or scratch is too
// small.
int bb_cpfont_build_mini_kern(bb_cpfont_file* file, const bb_cpfont_style* style, const uint32_t* codepoints,
                              size_t count, uint8_t* scratch, size_t scratch_len, uint16_t* index, bool* index_ready,
                              bool (*reserve)(void* context, const bb_cpfont_mini_kern* kern, uint8_t** left,
                                              uint8_t** right, int8_t** matrix),
                              void* context, bb_cpfont_mini_kern* result);

#ifdef __cplusplus
}
#endif
