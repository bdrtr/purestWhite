// C ABI of rust/crates/bembeyaz-ffi/src/png.rs, the chapter-image PNG decoder.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Results, PNGdec's numbers.
enum {
  BB_PNG_SUCCESS = 0,
  // Not opened, no draw callback, or the file ended before the image did.
  BB_PNG_FILE_ENDED = 1,
  BB_PNG_DECODE_ERROR = 2,
  BB_PNG_MEM_ERROR = 3,
  // Interlaced, or more than 8 bits per sample.
  BB_PNG_UNSUPPORTED_FEATURE = 5,
  BB_PNG_INVALID_FILE = 6,
  // A row of BB_PNG_MAX_PITCH bytes or more.
  BB_PNG_TOO_BIG = 7,
  // draw returned 0.
  BB_PNG_QUIT_EARLY = 8,
};

enum {
  BB_PNG_PIXEL_GRAYSCALE = 0,
  BB_PNG_PIXEL_TRUECOLOR = 2,
  BB_PNG_PIXEL_INDEXED = 3,
  BB_PNG_PIXEL_GRAY_ALPHA = 4,
  BB_PNG_PIXEL_TRUECOLOR_ALPHA = 6,
};

// Open refuses rows of this many bytes or more (the PNG_MAX_BUFFERED_PIXELS value platformio.ini
// set for PNGdec).
enum { BB_PNG_MAX_PITCH = 16416 };

typedef struct bb_png bb_png;

typedef struct {
  int32_t width;
  int32_t height;
  uint8_t pixel_type;
  // Bits per sample.
  uint8_t bpp;
  // An alpha channel, or (once decoding has met a tRNS chunk) transparency.
  bool has_alpha;
} bb_png_info;

typedef struct {
  // pitch bytes of samples, packed most significant bit first below 8 bits.
  const uint8_t* pixels;
  // 256 RGB triples, then 256 alpha values.
  const uint8_t* palette;
  int32_t y;
  int32_t width;
  int32_t pitch;
  // The tRNS color key: a gray sample, or 0xRRGGBB, each the low byte of its 16-bit value.
  uint32_t transparent;
  uint8_t pixel_type;
  uint8_t bpp;
  bool has_alpha;
} bb_png_row;

// Reads up to len bytes at pos into buf and returns how many it read (0 past the end or on error).
typedef size_t (*bb_png_read_fn)(void* context, uint32_t pos, uint8_t* buf, size_t len);

// Reads the header of the file (size bytes) through read, and writes the result to *status.
// Returns the image even when the header fails (its info is still set), or NULL when out of
// memory. read and context must stay valid until bb_png_free.
bb_png* bb_png_open(bb_png_read_fn read, void* context, uint32_t size, int* status);
void bb_png_get_info(const bb_png* png, bb_png_info* info);
// Decodes an image that opened, handing draw each row top to bottom (rows the data holds past the
// height may follow); draw returns 0 to stop. The row lives until draw returns. Allocates about
// 37 KB plus two rows for the call. Returns a result above.
int bb_png_decode(bb_png* png, int (*draw)(void* user, const bb_png_row* row), void* user);
// Writes the gray (0 black to 255 white) of row's pixels into gray (len bytes, at most the row's
// width used) and their alpha into alpha (NULL, or len bytes), or without alpha composited onto
// white: luma 77/150/29 in 256, low-depth samples scaled to 255, a tRNS color key clear, unknown
// pixel types mid gray. row is as bb_png_decode hands it (without a palette, mid gray).
void bb_png_row_gray(const bb_png_row* row, uint8_t* gray, size_t len, uint8_t* alpha);
void bb_png_free(bb_png* png);

#ifdef __cplusplus
}
#endif
