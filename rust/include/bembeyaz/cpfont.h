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

// A glyph record, laid out as EpdGlyph.
typedef struct {
  uint8_t width;
  uint8_t height;
  // 12.4 fixed point.
  uint16_t advance_x;
  int16_t left;
  int16_t top;
  uint16_t data_length;
  // From the style's bitmaps in the file; into the arena after bb_cpfont_read_bitmaps.
  uint32_t data_offset;
} bb_cpfont_glyph;

// Why bb_cpfont_read_glyphs or bb_cpfont_read_glyph fails; read_glyphs' failed is the glyph index.
enum {
  BB_CPFONT_GLYPH_SEEK = 1,
  BB_CPFONT_GLYPH_READ = 2,
};

// Reads the records of glyphs index(context, 0) .. index(context, count - 1), in that order
// (ascending, so runs of consecutive glyphs read without seeking; the first always seeks), and
// passes each to on_glyph with its position. A glyph whose data_length is under the bytes its
// width * height pixels need (1 or 2 bits each) gets width, height and data_length 0 and keeps its
// other metrics. Returns 0 with *seeks set, a BB_CPFONT_GLYPH_* reason with *failed set (the
// records before it were passed on), or -1 for a NULL file, style, index or on_glyph. seeks and
// failed may be NULL.
int bb_cpfont_read_glyphs(bb_cpfont_file* file, const bb_cpfont_style* style, bool two_bit, size_t count,
                          uint32_t (*index)(void* context, size_t i),
                          void (*on_glyph)(void* context, size_t i, const bb_cpfont_glyph* glyph), void* context,
                          uint32_t* seeks, uint32_t* failed);

// Reads the record of glyph index into out, as bb_cpfont_read_glyphs does. Returns 0, a
// BB_CPFONT_GLYPH_* reason, or -1 for a NULL file, style or out.
int bb_cpfont_read_glyph(bb_cpfont_file* file, const bb_cpfont_style* style, bool two_bit, uint32_t index,
                         bb_cpfont_glyph* out);

// Why bb_cpfont_read_bitmaps fails.
enum {
  BB_CPFONT_BITMAP_SEEK = 1,
  BB_CPFONT_BITMAP_READ = 2,
};

// Reads the bitmaps of glyphs[order[0]] .. glyphs[order[count - 1]] into arena one after another
// (order ascending by data_offset, so adjacent bitmaps read without seeking) and points each
// glyph's data_offset at its place in arena; a glyph without a bitmap gets the place the next one
// would take. Returns 0 with *seeks set, a BB_CPFONT_BITMAP_* reason, or -1 for a NULL file or
// style, a NULL array whose length is not 0, an order entry past glyph_count or bitmaps that do
// not fit arena. seeks may be NULL.
int bb_cpfont_read_bitmaps(bb_cpfont_file* file, const bb_cpfont_style* style, bb_cpfont_glyph* glyphs,
                           size_t glyph_count, const uint32_t* order, size_t count, uint8_t* arena, size_t arena_len,
                           uint32_t* seeks);

// A style's interval table: bembeyaz-font's records (read them through bb_cpfont_find_glyph) in
// memory the caller owns.
typedef struct {
  // NULL when not loaded.
  uint8_t* entries;
  uint32_t count;
  bool compact;
  // entries belong to an earlier style's table.
  bool shared;
} bb_cpfont_interval_table;

// Why bb_cpfont_load_intervals fails.
enum {
  // The table's start could not be reached.
  BB_CPFONT_INTERVALS_SEEK = 1,
  // Record result.record could not be read while checking.
  BB_CPFONT_INTERVALS_READ = 2,
  // Record result.record, result.first..result.last, breaks the layout as result.fault says.
  BB_CPFONT_INTERVALS_INVALID = 3,
  // The table's start could not be reached again.
  BB_CPFONT_INTERVALS_SEEK_BACK = 4,
  // reserve gave no memory for result.bytes bytes of form result.compact.
  BB_CPFONT_INTERVALS_RESERVE = 5,
  // Record result.record could not be read into the compact table. *table holds the reserved
  // memory, for the caller to free.
  BB_CPFONT_INTERVALS_COMPACT_READ = 6,
  // The records could not be read into the full table; *table holds the memory as above.
  BB_CPFONT_INTERVALS_FULL_READ = 7,
};

typedef struct {
  uint32_t record;
  uint32_t first;
  uint32_t last;
  // The table's size, allocated or shared.
  uint32_t bytes;
  bb_cpfont_interval_fault fault;
  // The earlier style whose table was shared (its position in earlier), or 255.
  uint8_t owner;
  bool compact;
} bb_cpfont_interval_load;

// Loads style's interval table from file into *table. Every record is checked first, and compared
// with the earlier styles' tables of the same length (earlier: earlier_count pointers, NULL for an
// absent style; at most BB_CPFONT_MAX_STYLES are looked at); one that matched every record and has
// the same form is shared. Otherwise reserve(context, bytes) is asked for the table, compact when
// every value and the glyph count fit 16 bits; it returns that much memory (not NULL even for 0
// bytes) or NULL when it cannot. The records are then read again into it: one read per record for
// a compact table, one read for all of them otherwise. Returns 0 with *table, result->owner (255
// when not shared), result->bytes and result->compact set, or a BB_CPFONT_INTERVALS_* reason with
// *result filled as the reason says and *table untouched unless the reason says otherwise.
// Returns -1, without writing *table or *result, for a NULL file, style, table, reserve or result,
// or a NULL earlier with a count.
int bb_cpfont_load_intervals(bb_cpfont_file* file, const bb_cpfont_style* style,
                             const bb_cpfont_interval_table* const* earlier, size_t earlier_count,
                             bb_cpfont_interval_table* table, uint8_t* (*reserve)(void* context, size_t bytes),
                             void* context, bb_cpfont_interval_load* result);

// The glyph index of codepoint in table, or -1 when it is not covered or table is NULL or not
// loaded.
int32_t bb_cpfont_find_glyph(const bb_cpfont_interval_table* table, uint32_t codepoint);

// A run of codepoints and the position of its first glyph, laid out as EpdUnicodeInterval.
typedef struct {
  uint32_t first;
  uint32_t last;
  uint32_t offset;
} bb_cpfont_interval;

// A codepoint and its glyph index in the font.
typedef struct {
  uint32_t codepoint;
  int32_t index;
} bb_cpfont_mapping;

// A style's ligature table: count 8-byte records laid out as EpdLigaturePair.
typedef struct {
  const uint8_t* pairs;
  uint32_t count;
} bb_cpfont_ligatures;

// The page functions below read a NULL array as empty.

// Gathers a page's codepoints into out, unsorted: those of the texts text(context, 0) ..
// text(context, text_count - 1) (NULL ones are skipped) in the order they first appear, up to
// budget, then U+FFFD and the outputs of the ligatures (ligature_tables tables, in order) whose two
// parts are both on the page, while out_len has room (only the first BB_CPFONT_MAX_STYLES ligature
// tables are used). Returns how many it wrote; 0 for a NULL text callback.
size_t bb_cpfont_gather_page(const char* (*text)(const void* context, uint32_t i), const void* context,
                             uint32_t text_count, size_t budget, const bb_cpfont_ligatures* ligatures,
                             size_t ligature_tables, uint32_t* out, size_t out_len);

// When the resident mini intervals (ascending) hold every codepoint of request that table covers
// (a NULL or unloaded table covers none), how many request codepoints table lacks; otherwise -1.
int32_t bb_cpfont_mini_covers(const uint32_t* request, size_t count, const bb_cpfont_interval* mini, size_t mini_count,
                              const bb_cpfont_interval_table* table);

// Merges the codepoints of the mini intervals with the ascending request into out, dropping
// duplicates. Returns how many there are, or 0 when mini is empty or they do not fit out_len.
size_t bb_cpfont_merge_mini(const bb_cpfont_interval* mini, size_t mini_count, const uint32_t* request, size_t count,
                            uint32_t* out, size_t out_len);

// Maps each of the count codepoints table covers to its glyph index, in order, into out (count
// entries). Returns how many it covers; 0 for a NULL or unloaded table.
size_t bb_cpfont_find_glyphs(const bb_cpfont_interval_table* table, const uint32_t* codepoints, size_t count,
                             bb_cpfont_mapping* out);

// Lays the count ascending mappings out as intervals of consecutive codepoints into out (out_len
// entries), each pointing at its first mapping. Returns how many it wrote.
size_t bb_cpfont_build_mini_intervals(const bb_cpfont_mapping* mappings, size_t count, bb_cpfont_interval* out,
                                      size_t out_len);

// Writes the codepoints of the mini intervals into out, as many as fit out_len. Returns how many.
size_t bb_cpfont_mini_codepoints(const bb_cpfont_interval* mini, size_t mini_count, uint32_t* out, size_t out_len);

#ifdef __cplusplus
}
#endif
