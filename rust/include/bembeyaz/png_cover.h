// C ABI of rust/crates/bembeyaz-ffi/src/png_cover.rs, the PNG reader for covers.
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Results.
enum {
  BB_PNG_COVER_OK = 0,
  // Decoded without a successful open, or a second time, or with a null cover or draw.
  BB_PNG_COVER_NOT_OPEN = 1,
  // The file ended inside the header.
  BB_PNG_COVER_HEADER_ENDED = 2,
  BB_PNG_COVER_NOT_PNG = 3,
  // The first chunk is not IHDR.
  BB_PNG_COVER_NO_HEADER = 4,
  // Compression or filter method other than 0, interlaced, an unknown color type, or a bit depth
  // PNG does not allow for the color type.
  BB_PNG_COVER_UNSUPPORTED = 5,
  // No pixels, wider than 2048 or taller than 3072, or rows of more than 16384 bytes.
  BB_PNG_COVER_BAD_SIZE = 6,
  // No IDAT chunk before IEND or the end of the file.
  BB_PNG_COVER_NO_IMAGE_DATA = 7,
  BB_PNG_COVER_MEMORY = 8,
  // The image data ended or is corrupt (a wrong Adler-32 sum included, when the read of the last
  // row reaches it); the row being read is not drawn.
  BB_PNG_COVER_CORRUPT = 9,
  // A row with a filter type other than 0 to 4.
  BB_PNG_COVER_UNKNOWN_FILTER = 10,
  // draw returned 0.
  BB_PNG_COVER_QUIT_EARLY = 11,
};

typedef struct bb_png_cover bb_png_cover;

typedef struct {
  uint32_t width;
  uint32_t height;
} bb_png_cover_info;

// Reads up to len bytes at pos into buf and returns how many it read (0 past the end or on error).
typedef size_t (*bb_png_cover_read_fn)(void* context, uint32_t pos, uint8_t* buf, size_t len);

// Reads the header of the file (size bytes) through read and walks to its image data, as
// PngToBmpConverter's own decoder did, and writes the result to *status. Returns the cover even
// when that fails (width and height are set once IHDR is read), or NULL when out of memory or read
// is NULL. read and context must stay valid until bb_png_cover_free.
bb_png_cover* bb_png_cover_open(bb_png_cover_read_fn read, void* context, uint32_t size, int* status);
void bb_png_cover_get_info(const bb_png_cover* cover, bb_png_cover_info* info);
// Decodes a cover that opened, once, handing draw each row top to bottom as width gray bytes;
// draw returns 0 to stop. The row lives until draw returns. Allocates about 37 KB, three rows and a
// gray row for the call. Returns a result above.
int bb_png_cover_decode(bb_png_cover* cover, int (*draw)(void* user, const uint8_t* gray), void* user);
void bb_png_cover_free(bb_png_cover* cover);

#ifdef __cplusplus
}
#endif
