// C ABI of rust/crates/bembeyaz-ffi/src/compressed.rs. Use lib/EpdFont/FontDecompressor.h instead.
#pragma once

#include <bembeyaz/cpfont.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The most glyphs a page keeps, and the most groups it decompresses.
#define BB_COMPRESSED_MAX_PAGE_GLYPHS 512
#define BB_COMPRESSED_MAX_PAGE_GROUPS 128

// A DEFLATE-compressed block of glyph bitmaps, laid out as EpdFontGroup.
typedef struct {
  uint32_t compressed_offset;
  uint32_t compressed_size;
  uint32_t uncompressed_size;
  uint16_t glyph_count;
  uint32_t first_glyph;
} bb_compressed_group;

// A page glyph and where its bitmap is: buffer_offset in the page buffer (UINT32_MAX until
// extracted), aligned_offset in its decompressed group.
typedef struct {
  uint32_t glyph;
  uint32_t buffer_offset;
  uint32_t aligned_offset;
} bb_compressed_entry;

// A compressed font's tables: glyphs and intervals are EpdGlyph and EpdUnicodeInterval (the same
// layouts as bb_cpfont_glyph and bb_cpfont_interval). glyph_to_group is NULL for fonts whose
// groups hold consecutive glyphs, or holds glyph_count entries; ligatures is ligature_count
// 8-byte records laid out as EpdLigaturePair. NULL arrays read as empty.
typedef struct {
  const bb_cpfont_glyph* glyphs;
  size_t glyph_count;
  const bb_cpfont_interval* intervals;
  size_t interval_count;
  const bb_compressed_group* groups;
  size_t group_count;
  const uint16_t* glyph_to_group;
  const uint8_t* ligatures;
  size_t ligature_count;
} bb_compressed_font;

// The group that holds glyph, or -1.
int32_t bb_compressed_group_of(const bb_compressed_font* font, uint32_t glyph);
// Packs glyph from its decompressed group (rows padded to whole bytes, 2 bits a pixel) into packed
// without the padding; bytes past either buffer are left out.
void bb_compressed_unpack(const bb_compressed_font* font, uint16_t group, uint32_t glyph, const uint8_t* decompressed,
                          size_t decompressed_len, uint8_t* packed, size_t packed_len);

// Gathers the glyphs of text the font has into out, unique, in the order they first appear, up to
// out_len (at most BB_COMPRESSED_MAX_PAGE_GLYPHS), then the outputs of ligatures whose two parts
// are both among them while there is room. Returns how many; *capped (when not NULL) says whether
// the text had more than fit.
size_t bb_compressed_gather(const bb_compressed_font* font, const char* text, uint32_t* out, size_t out_len,
                            bool* capped);

// Plans a page of count glyphs: their groups (in the order first needed, up to
// BB_COMPRESSED_MAX_PAGE_GROUPS, into groups, which holds that many), and *total_bytes their
// packed bitmaps take.
// Returns how many groups; *capped says whether more were needed. total_bytes and capped may be
// NULL.
size_t bb_compressed_plan(const bb_compressed_font* font, const uint32_t* glyphs, size_t count, uint16_t* groups,
                          uint32_t* total_bytes, bool* capped);

// Lays out count entries for the page glyphs: sorted by glyph, none extracted, each with its
// offset in its decompressed group when groups (the plan's) holds that group.
void bb_compressed_index(const bb_compressed_font* font, const uint32_t* glyphs, size_t count, const uint16_t* groups,
                         size_t group_count, bb_compressed_entry* entries);

// Packs the page glyphs of group from its decompressed bitmaps into buffer from write on, and
// records where each went. Returns where the next one goes.
uint32_t bb_compressed_extract(const bb_compressed_font* font, uint16_t group, const uint8_t* decompressed,
                               size_t decompressed_len, bb_compressed_entry* entries, size_t count, uint8_t* buffer,
                               size_t buffer_len, uint32_t write);

// Where glyph's bitmap is in the page buffer, or UINT32_MAX when the count entries (sorted) lack
// it or it was not extracted.
uint32_t bb_compressed_find_entry(const bb_compressed_entry* entries, size_t count, uint32_t glyph);

#ifdef __cplusplus
}
#endif
