// C ABI of rust/crates/bembeyaz-ffi/src/dither.rs. lib/GfxRenderer/BitmapHelpers.h wraps the
// ditherer; lib/Epub's framebuffer converters use the sampling directly.
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

// Covers scaled to the output width before dithering, positions in 16.16 fixed point.
// Adds a source row (row_len bytes) to the area sums and counts of an output row width pixels wide:
// output column x covers source columns x * scale_x >> 16 up to (x + 1) * scale_x >> 16 (products
// wrapping at 32 bits), or at least the first of them, kept inside the row.
void bb_dither_area_add(const uint8_t* row, size_t row_len, uint32_t scale_x, uint32_t* sums, uint32_t* counts,
                        size_t width);
// Dithers the area averages (sums over counts, 0 where nothing was counted) of a row width pixels
// wide as row y into out (out_len bytes), as bb_dither_pixels does from x0 0.
void bb_dither_area_pixels(bb_dither* dither, const uint32_t* sums, const uint32_t* counts, size_t width, int32_t y,
                           uint8_t* out, size_t out_len);
// Writes row (row_len bytes) linearly resampled to out_len pixels into out: pixel x samples x * step,
// the last pixel the row's last. step is ((row_len - 1) << 16) / (out_len - 1), or reads past the row
// come out 0.
void bb_dither_linear_row(const uint8_t* row, size_t row_len, uint32_t step, uint8_t* out, size_t out_len);
// Writes rows a and b (len bytes each) mixed fy (16.16, below 1) of the way to b into out.
void bb_dither_blend_rows(const uint8_t* a, const uint8_t* b, uint32_t fy, uint8_t* out, size_t len);

// Images as they decode: blocks or lines of gray resampled to the output size, each pixel turned into
// a 2-bit level (0 black to 3 white), packed four to a byte, most significant first.
enum {
  BB_DITHER_QUANTIZE_BAYER = 0,     // ordered dithering, 4x4 Bayer matrix at the screen position
  BB_DITHER_QUANTIZE_THIRDS = 1,    // gray / 85
  BB_DITHER_QUANTIZE_QUARTERS = 2,  // the top two bits
};

// Gray pixels at (x, y) of the source: height rows stride bytes apart (len bytes in all), the first
// width of each valid.
typedef struct {
  const uint8_t* pixels;
  size_t len;
  size_t stride;
  int32_t width;
  int32_t height;
  int32_t x;
  int32_t y;
} bb_dither_gray_block;

// Output size over source size on each axis (x, y) and source over output (inv_x, inv_y), 16.16
// fixed point.
typedef struct {
  int32_t x;
  int32_t inv_x;
  int32_t y;
  int32_t inv_y;
} bb_dither_scale;

// Writes the levels of output row dst_y's columns start to end (exclusive) into out (out_len bytes),
// packed from its first bit; the last byte's spare bits are 0. The output's top left is at screen
// (ox, oy), which places the dither pattern. Growing on both axes interpolates bilinearly, 1:1 copies
// (the row as it is, up to its stride), and anything else takes the nearest pixel; other source
// positions are kept in the block.
void bb_dither_block_row(const bb_dither_gray_block* block, const bb_dither_scale* scale, int32_t dst_y, int32_t start,
                         int32_t end, int32_t ox, int32_t oy, uint8_t quantize, uint8_t* out, size_t out_len);
// A source line of gray values and, for images drawn over the screen, alpha (or NULL), len bytes
// each, stepped through for an output dst_width columns wide: column c takes pixel
// crop + c * visible / dst_width, rounded down.
typedef struct {
  const uint8_t* gray;
  const uint8_t* alpha;
  size_t len;
  int32_t crop;
  int32_t visible;
  int32_t dst_width;
} bb_dither_line;

// Writes the levels of output row out_y's columns start to end (exclusive) from line into out as
// bb_dither_block_row does; the output's left edge is at screen column ox. With a mask (mask_len
// bytes), each column also gets a bit there, most significant first: set where its pixel is opaque
// enough to draw against a 4x4 ordered threshold at its screen position, all set without alpha.
void bb_dither_line_row(const bb_dither_line* line, int32_t start, int32_t end, int32_t ox, int32_t out_y,
                        uint8_t quantize, uint8_t* out, size_t out_len, uint8_t* mask, size_t mask_len);
// Copies count packed levels from src (src_len bytes) into the packed row dst (dst_len bytes) from its
// column at, leaving out the columns before its start or past its last byte.
void bb_dither_put_levels(uint8_t* dst, size_t dst_len, int32_t at, const uint8_t* src, size_t src_len, size_t count);

#ifdef __cplusplus
}
#endif
