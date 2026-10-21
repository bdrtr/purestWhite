// C ABI of rust/crates/bembeyaz-ffi/src/bmp.rs. Use lib/GfxRenderer/Bitmap.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// What a BMP's headers say.
typedef struct {
  int32_t width;
  int32_t height;
  // Bytes per row in the file, padded to 4.
  int32_t row_bytes;
  // Where the pixels start.
  uint32_t data_offset;
  uint32_t colors_used;
  uint16_t bits_per_pixel;
  bool top_down;
  // The file has a palette and every entry is near one of the display's gray levels (0, 85, 170,
  // 255), so the image needs no dithering; always so at 1 and 2 bits per pixel.
  bool native_palette;
  // The luminance of each palette index; entries past the palette hold their own index.
  uint8_t palette_lum[256];
} bb_bmp;

// Reads the headers and palette of an uncompressed BMP (1, 2, 4, 8, 24 or 32 bits per pixel, at
// most 2048 x 3072) into bmp and leaves the file at the pixels. read fills buf with up to len bytes
// and returns how many, 0 at the end or on an error; skip moves forward (a failure leaves the
// position); seek moves to a position and returns false when it cannot. context is passed to them
// as is. Returns 0, or BmpReaderError's number for why the file is
// refused (1 for a NULL argument); the fields read before an error are left set.
int bb_bmp_parse(bb_bmp* bmp, size_t (*read)(void* context, uint8_t* buf, size_t len),
                 void (*skip)(void* context, uint32_t count), bool (*seek)(void* context, uint32_t position),
                 void* context);
// Writes the luminance of pixels first..first + count of a row read from the file (len bytes), as
// if bytes past it were 0. False for a depth bb_bmp_parse refuses.
bool bb_bmp_row_luminance(const bb_bmp* bmp, const uint8_t* row, size_t len, size_t first, uint8_t* out, size_t count);

#ifdef __cplusplus
}
#endif
