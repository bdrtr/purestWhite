// C ABI of rust/crates/bembeyaz-ffi/src/dither.rs. Use lib/GfxRenderer/BitmapHelpers.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// How gray (0 black to 255 white) becomes the panel's 2-bit levels (0 black to 3 white) or 1 bit.
enum {
  BB_DITHER_THRESHOLD = 0,        // 2-bit, fixed thresholds tuned to the panel
  BB_DITHER_NATIVE = 1,           // 2-bit, the top two bits (palettes of the panel's levels)
  BB_DITHER_ATKINSON = 2,         // 2-bit, Atkinson error diffusion
  BB_DITHER_FLOYD_STEINBERG = 3,  // 2-bit, Floyd-Steinberg, mirrored on odd rows
  BB_DITHER_ATKINSON_1BIT = 4,    // 1-bit, Atkinson error diffusion
};

// One image's dithering: the method, and the error values it carries from row to row (owned by the
// caller; NULL, or too few, only quantizes). Start with rows_done at 0.
typedef struct {
  int16_t* errors;
  size_t errors_len;
  uint32_t rows_done;
  uint8_t method;  // a BB_DITHER_*
  bool original_thresholds;
} bb_dither;

// Gray after the brightness, contrast and gamma adjustments (bb_dither_pixels applies them too).
uint8_t bb_dither_adjust(uint8_t gray);
// Error values a row of width pixels needs carried by a method (0: none).
size_t bb_dither_error_len(uint8_t method, int32_t width);
// Writes the levels of count gray pixels (pixels x0 on of row y) into out (out_len bytes), a row
// packed most significant bit first at the method's bits per pixel; other pixels' bits stay as they
// are. The gray values get the brightness, contrast and gamma adjustments here.
void bb_dither_pixels(bb_dither* dither, const uint8_t* gray, size_t count, size_t x0, int32_t y, uint8_t* out,
                      size_t out_len);
// Moves on to the next row.
void bb_dither_next_row(bb_dither* dither);
// Starts over: no carried error, row 0 again.
void bb_dither_reset(bb_dither* dither);

#ifdef __cplusplus
}
#endif
