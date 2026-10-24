// C ABI of rust/crates/bembeyaz-ffi/src/drop_cap.rs: how the chapter parser sizes and places a
// drop cap beside the lines it spans. Use lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Where a drop cap goes: its top lines up with the first spanned line's capitals and its baseline
// with the last line's.
typedef struct {
  uint16_t scale;  // the letter's enlargement in 256ths, 256 to 4096
  // The letter and a space after it, at most half the room; the lines beside it move right by this.
  int32_t width;
  int32_t baseline;  // from the top of the first spanned line to the letter's baseline
  int32_t height;    // the spanned lines' height
} bb_drop_cap;

// Writes the advance (12.4 fixed point) and top of cp in the body text's font, or in the cap's
// font when cap, and returns true; false when the font has no such glyph.
typedef bool (*bb_drop_cap_metrics)(void* context, bool cap, uint32_t cp, int32_t* advance, int* top);

// Places the letters of text (len bytes of UTF-8, up to a NUL) as a drop cap lines lines tall,
// beside body text with this line height, ascender and space width, in width pixels of room.
// Without a body 'H' the capitals are 70% of the ascender; without a cap 'H' (or one of no height)
// the letter is sized as if its capitals were the body's. Without metrics, out is left as it was.
void bb_drop_cap_place(uint8_t lines, const char* text, size_t len, int line_height, int ascender, int space_width,
                       uint16_t width, bb_drop_cap_metrics metrics, void* context, bb_drop_cap* out);

#ifdef __cplusplus
}
#endif
