#pragma once

#include <bembeyaz/dither.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>

#include "../Memory/Memory.h"

struct BmpHeader;

// Gray (0 black to 255 white) after the brightness, contrast and gamma adjustments.
inline int adjustPixel(const int gray) { return bb_dither_adjust(static_cast<uint8_t>(gray)); }

struct GrayPlanePixel {
  bool write;
  bool black;
};

// level: 0=black, 1=dark, 2=light, 3=white. drawPixel(true) clears a bit. bembeyaz-gfx's levels.rs
// (Ink::of) has the same rule for pictures and cached image rows: change both.
constexpr GrayPlanePixel grayPlanePixel(uint8_t level, bool msb, bool absolute) {
  if (absolute) return {true, !(level == 3 || level == (msb ? 2 : 1))};
  return {msb ? (level == 1 || level == 2) : level == 1, false};
}

enum class BmpRowOrder { BottomUp, TopDown };

// Populates a 1-bit BMP header in the provided memory.
void createBmpHeader(BmpHeader* bmpHeader, int width, int height, BmpRowOrder rowOrder);

// Gray rows to BMP rows of the panel's 2-bit levels or 1 bit, through bembeyaz-dither: the method
// (a BB_DITHER_*) and, for error diffusion, the error rows it carries, which this owns.
class Ditherer {
 public:
  Ditherer(const uint8_t method, const int width, const bool originalThresholds = false) {
    state.method = method;
    state.original_thresholds = originalThresholds;
    const size_t len = bb_dither_error_len(method, width);
    needsRows =
        method == BB_DITHER_ATKINSON || method == BB_DITHER_FLOYD_STEINBERG || method == BB_DITHER_ATKINSON_1BIT;
    if (len == 0) return;
    errors = makeUniqueNoThrow<int16_t[]>(len);
    if (!errors) return;
    state.errors = errors.get();
    state.errors_len = len;
  }
  Ditherer(const Ditherer&) = delete;
  Ditherer& operator=(const Ditherer&) = delete;

  // False when the error rows could not be allocated.
  bool isValid() const { return state.errors != nullptr || !needsRows; }

  // Writes the levels of count gray pixels, pixels x0 on of row y, into the packed row out (outLen
  // bytes); other pixels' bits stay as they are.
  void pixels(const uint8_t* gray, const int count, const int x0, const int y, uint8_t* out, const size_t outLen) {
    if (count <= 0 || x0 < 0) return;
    bb_dither_pixels(&state, gray, static_cast<size_t>(count), static_cast<size_t>(x0), y, out, outLen);
  }
  void nextRow() { bb_dither_next_row(&state); }
  void reset() { bb_dither_reset(&state); }

 private:
  std::unique_ptr<int16_t[]> errors;
  bb_dither state{};
  bool needsRows = false;
};

// dither.h's layout and methods, which bembeyaz-ffi checks too.
static_assert(sizeof(bb_dither) == 2 * sizeof(size_t) + 8 && offsetof(bb_dither, rows_done) == 2 * sizeof(size_t) &&
                  offsetof(bb_dither, method) == 2 * sizeof(size_t) + 4 &&
                  offsetof(bb_dither, original_thresholds) == 2 * sizeof(size_t) + 5,
              "bb_dither layout");
static_assert(BB_DITHER_THRESHOLD == 0 && BB_DITHER_NATIVE == 1 && BB_DITHER_ATKINSON == 2 &&
                  BB_DITHER_FLOYD_STEINBERG == 3 && BB_DITHER_ATKINSON_1BIT == 4,
              "bb_dither methods");
