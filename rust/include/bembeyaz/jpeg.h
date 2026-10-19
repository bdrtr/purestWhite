// C ABI of rust/crates/bembeyaz-ffi/src/jpeg.rs, the JPEG decoder for chapter images and covers.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Results, JPEGDEC's numbers.
enum {
  BB_JPEG_SUCCESS = 0,
  // No image, read or draw callback, an unknown option, or a decode after a failed open or a
  // second time.
  BB_JPEG_INVALID_PARAMETER = 1,
  BB_JPEG_DECODE_ERROR = 2,
  BB_JPEG_UNSUPPORTED_FEATURE = 3,
  BB_JPEG_INVALID_FILE = 4,
  // bb_jpeg_open could not allocate the image.
  BB_JPEG_ERROR_MEMORY = 5,
};

// Decode options: 1/2, 1/4 and 1/8 size. A progressive image decodes at 1/8 when asked for full
// size; at 1/2 or 1/4 it gives that size from the same 1/8 data (blocky, or for subsampled color
// mostly unfilled): ask for 1/8.
enum {
  BB_JPEG_SCALE_HALF = 2,
  BB_JPEG_SCALE_QUARTER = 4,
  BB_JPEG_SCALE_EIGHTH = 8,
};

typedef struct bb_jpeg bb_jpeg;

typedef struct {
  int32_t width;
  int32_t height;
  // Only its first scan is decoded (see the decode options).
  bool progressive;
} bb_jpeg_info;

// A block of decoded MCUs, left to right and top to bottom: height rows of width 8-bit gray
// pixels, of which the first width_used of each row are inside the image.
typedef struct {
  const uint8_t* pixels;
  int32_t x;
  int32_t y;
  int32_t width;
  int32_t height;
  int32_t width_used;
} bb_jpeg_block;

// Reads up to len bytes at pos into buf and returns how many it read (0 past the end or on error).
typedef size_t (*bb_jpeg_read_fn)(void* context, uint32_t pos, uint8_t* buf, size_t len);

// Reads the header of the file (size bytes) through read, and writes the result (BB_JPEG_*) to
// *status. Returns the image even when the header fails, or NULL when read is NULL or memory runs
// out. read and context must stay valid until bb_jpeg_free.
bb_jpeg* bb_jpeg_open(bb_jpeg_read_fn read, void* context, int32_t size, int* status);
void bb_jpeg_get_info(const bb_jpeg* jpeg, bb_jpeg_info* info);
// Decodes an image that opened, once, at options (0 or a BB_JPEG_SCALE_*), handing draw each block;
// draw returns 0 to stop, and the decode still succeeds. The block lives until draw returns.
// Returns the result (BB_JPEG_*).
int bb_jpeg_decode(bb_jpeg* jpeg, int options, int (*draw)(void* user, const bb_jpeg_block* block), void* user);
void bb_jpeg_free(bb_jpeg* jpeg);

#ifdef __cplusplus
}
#endif
