// lib/GfxRenderer/BitmapHelpers.h and .cpp at 2a9cbb29, verbatim but for the namespace (old), the
// Memory.h include and createBmpHeader (left out): the oracle for bembeyaz-dither.
#pragma once

#include <Memory.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>

namespace old {

struct BmpHeader;

// Helper functions
uint8_t quantize(int gray, int x, int y);
uint8_t quantizeSimple(int gray);
uint8_t quantize1bit(int gray, int x, int y);
int adjustPixel(int gray);

struct GrayPlanePixel {
  bool write;
  bool black;
};

// level: 0=black, 1=dark, 2=light, 3=white. drawPixel(true) clears a bit. bembeyaz-gfx's image.rs
// (ink) has the same rule for pictures: change both.
constexpr GrayPlanePixel grayPlanePixel(uint8_t level, bool msb, bool absolute) {
  if (absolute) return {true, !(level == 3 || level == (msb ? 2 : 1))};
  return {msb ? (level == 1 || level == 2) : level == 1, false};
}

enum class BmpRowOrder { BottomUp, TopDown };

// 1-bit Atkinson dithering - better quality than noise dithering for thumbnails
// Error distribution pattern (same as 2-bit but quantizes to 2 levels):
//     X  1/8 1/8
// 1/8 1/8 1/8
//     1/8
class Atkinson1BitDitherer {
 public:
  explicit Atkinson1BitDitherer(int width) {
    if (width <= 0) return;
    const size_t candidateRowSize = static_cast<size_t>(width) + 4;
    if (candidateRowSize > SIZE_MAX / (3 * sizeof(int16_t))) return;
    rowSize = candidateRowSize;
    errorRows = makeUniqueNoThrow<int16_t[]>(rowSize * 3);
    if (!errorRows) return;
    errorRow0 = errorRows.get();
    errorRow1 = errorRow0 + rowSize;
    errorRow2 = errorRow1 + rowSize;
  }

  // Callers must check row allocation before processing pixels.
  bool isValid() const { return errorRows != nullptr; }

  // EXPLICITLY DELETE THE COPY CONSTRUCTOR
  Atkinson1BitDitherer(const Atkinson1BitDitherer& other) = delete;

  // EXPLICITLY DELETE THE COPY ASSIGNMENT OPERATOR
  Atkinson1BitDitherer& operator=(const Atkinson1BitDitherer& other) = delete;

  uint8_t processPixel(int gray, int x) {
    if (!isValid()) return adjustPixel(gray) < 128 ? 0 : 1;

    // Apply brightness/contrast/gamma adjustments
    gray = adjustPixel(gray);

    // Add accumulated error
    int adjusted = gray + errorRow0[x + 2];
    if (adjusted < 0) adjusted = 0;
    if (adjusted > 255) adjusted = 255;

    // Quantize to 2 levels (1-bit): 0 = black, 1 = white
    uint8_t quantized;
    int quantizedValue;
    if (adjusted < 128) {
      quantized = 0;
      quantizedValue = 0;
    } else {
      quantized = 1;
      quantizedValue = 255;
    }

    // Calculate error (only distribute 6/8 = 75%)
    int error = (adjusted - quantizedValue) >> 3;  // error/8

    // Distribute 1/8 to each of 6 neighbors
    errorRow0[x + 3] += error;  // Right
    errorRow0[x + 4] += error;  // Right+1
    errorRow1[x + 1] += error;  // Bottom-left
    errorRow1[x + 2] += error;  // Bottom
    errorRow1[x + 3] += error;  // Bottom-right
    errorRow2[x + 2] += error;  // Two rows down

    return quantized;
  }

  void nextRow() {
    if (!isValid()) return;
    int16_t* temp = errorRow0;
    errorRow0 = errorRow1;
    errorRow1 = errorRow2;
    errorRow2 = temp;
    memset(errorRow2, 0, rowSize * sizeof(int16_t));
  }

  void reset() {
    if (!isValid()) return;
    memset(errorRow0, 0, rowSize * sizeof(int16_t));
    memset(errorRow1, 0, rowSize * sizeof(int16_t));
    memset(errorRow2, 0, rowSize * sizeof(int16_t));
  }

 private:
  size_t rowSize{0};
  std::unique_ptr<int16_t[]> errorRows;
  int16_t* errorRow0 = nullptr;
  int16_t* errorRow1 = nullptr;
  int16_t* errorRow2 = nullptr;
};

// Atkinson dithering - distributes only 6/8 (75%) of error for cleaner results
// Error distribution pattern:
//     X  1/8 1/8
// 1/8 1/8 1/8
//     1/8
// Less error buildup = fewer artifacts than Floyd-Steinberg
class AtkinsonDitherer {
 public:
  explicit AtkinsonDitherer(int width, bool originalThresholds = false) : originalThresholds(originalThresholds) {
    if (width <= 0) return;
    const size_t candidateRowSize = static_cast<size_t>(width) + 4;
    if (candidateRowSize > SIZE_MAX / (3 * sizeof(int16_t))) return;
    rowSize = candidateRowSize;
    errorRows = makeUniqueNoThrow<int16_t[]>(rowSize * 3);
    if (!errorRows) return;
    errorRow0 = errorRows.get();
    errorRow1 = errorRow0 + rowSize;
    errorRow2 = errorRow1 + rowSize;
  }

  // Callers must check row allocation before processing pixels.
  bool isValid() const { return errorRows != nullptr; }

  // **1. EXPLICITLY DELETE THE COPY CONSTRUCTOR**
  AtkinsonDitherer(const AtkinsonDitherer& other) = delete;

  // **2. EXPLICITLY DELETE THE COPY ASSIGNMENT OPERATOR**
  AtkinsonDitherer& operator=(const AtkinsonDitherer& other) = delete;

  uint8_t processPixel(int gray, int x) {
    // Add accumulated error
    int adjusted = gray + (isValid() ? errorRow0[x + 2] : 0);
    if (adjusted < 0) adjusted = 0;
    if (adjusted > 255) adjusted = 255;

    // Quantize to 4 levels
    uint8_t quantized;
    int quantizedValue;
    if (originalThresholds) {
      if (adjusted < 43) {
        quantized = 0;
        quantizedValue = 0;
      } else if (adjusted < 128) {
        quantized = 1;
        quantizedValue = 85;
      } else if (adjusted < 213) {
        quantized = 2;
        quantizedValue = 170;
      } else {
        quantized = 3;
        quantizedValue = 255;
      }
    } else {  // Legacy panel tuning; slightly darker midtones.
      if (adjusted < 30) {
        quantized = 0;
        quantizedValue = 15;
      } else if (adjusted < 55) {
        quantized = 1;
        quantizedValue = 35;
      } else if (adjusted < 150) {
        quantized = 2;
        quantizedValue = 90;
      } else {
        quantized = 3;
        quantizedValue = 210;
      }
    }

    if (!isValid()) return quantized;

    // Calculate error (only distribute 6/8 = 75%)
    int error = (adjusted - quantizedValue) >> 3;  // error/8

    // Distribute 1/8 to each of 6 neighbors
    errorRow0[x + 3] += error;  // Right
    errorRow0[x + 4] += error;  // Right+1
    errorRow1[x + 1] += error;  // Bottom-left
    errorRow1[x + 2] += error;  // Bottom
    errorRow1[x + 3] += error;  // Bottom-right
    errorRow2[x + 2] += error;  // Two rows down

    return quantized;
  }

  void nextRow() {
    if (!isValid()) return;
    int16_t* temp = errorRow0;
    errorRow0 = errorRow1;
    errorRow1 = errorRow2;
    errorRow2 = temp;
    memset(errorRow2, 0, rowSize * sizeof(int16_t));
  }

  void reset() {
    if (!isValid()) return;
    memset(errorRow0, 0, rowSize * sizeof(int16_t));
    memset(errorRow1, 0, rowSize * sizeof(int16_t));
    memset(errorRow2, 0, rowSize * sizeof(int16_t));
  }

 private:
  size_t rowSize{0};
  std::unique_ptr<int16_t[]> errorRows;
  int16_t* errorRow0 = nullptr;
  int16_t* errorRow1 = nullptr;
  int16_t* errorRow2 = nullptr;
  const bool originalThresholds;
};

// Floyd-Steinberg error diffusion dithering with serpentine scanning
// Serpentine scanning alternates direction each row to reduce "worm" artifacts
// Error distribution pattern (left-to-right):
//       X   7/16
// 3/16 5/16 1/16
// Error distribution pattern (right-to-left, mirrored):
// 1/16 5/16 3/16
//      7/16  X
class FloydSteinbergDitherer {
 public:
  explicit FloydSteinbergDitherer(int width, bool originalThresholds = false)
      : rowCount(0), originalThresholds(originalThresholds) {
    if (width <= 0) return;
    const size_t candidateRowSize = static_cast<size_t>(width) + 2;
    if (candidateRowSize > SIZE_MAX / (2 * sizeof(int16_t))) return;
    rowSize = candidateRowSize;
    errorRows = makeUniqueNoThrow<int16_t[]>(rowSize * 2);
    if (!errorRows) return;
    errorCurRow = errorRows.get();
    errorNextRow = errorCurRow + rowSize;
  }

  // Callers must check row allocation before processing pixels.
  bool isValid() const { return errorRows != nullptr; }

  // **1. EXPLICITLY DELETE THE COPY CONSTRUCTOR**
  FloydSteinbergDitherer(const FloydSteinbergDitherer& other) = delete;

  // **2. EXPLICITLY DELETE THE COPY ASSIGNMENT OPERATOR**
  FloydSteinbergDitherer& operator=(const FloydSteinbergDitherer& other) = delete;

  // Process a single pixel and return quantized 2-bit value
  // x is the logical x position (0 to width-1), direction handled internally
  uint8_t processPixel(int gray, int x) {
    // Add accumulated error to this pixel
    int adjusted = gray + (isValid() ? errorCurRow[x + 1] : 0);

    // Clamp to valid range
    if (adjusted < 0) adjusted = 0;
    if (adjusted > 255) adjusted = 255;

    // Quantize to 4 levels (0, 85, 170, 255)
    uint8_t quantized;
    int quantizedValue;
    if (originalThresholds) {
      if (adjusted < 43) {
        quantized = 0;
        quantizedValue = 0;
      } else if (adjusted < 128) {
        quantized = 1;
        quantizedValue = 85;
      } else if (adjusted < 213) {
        quantized = 2;
        quantizedValue = 170;
      } else {
        quantized = 3;
        quantizedValue = 255;
      }
    } else {  // Legacy panel tuning; slightly darker midtones.
      if (adjusted < 30) {
        quantized = 0;
        quantizedValue = 15;
      } else if (adjusted < 55) {
        quantized = 1;
        quantizedValue = 35;
      } else if (adjusted < 150) {
        quantized = 2;
        quantizedValue = 90;
      } else {
        quantized = 3;
        quantizedValue = 210;
      }
    }

    if (!isValid()) return quantized;

    // Calculate error
    int error = adjusted - quantizedValue;

    // Distribute error to neighbors (serpentine: direction-aware)
    if (!isReverseRow()) {
      // Left to right: standard distribution
      // Right: 7/16
      errorCurRow[x + 2] += (error * 7) >> 4;
      // Bottom-left: 3/16
      errorNextRow[x] += (error * 3) >> 4;
      // Bottom: 5/16
      errorNextRow[x + 1] += (error * 5) >> 4;
      // Bottom-right: 1/16
      errorNextRow[x + 2] += (error) >> 4;
    } else {
      // Right to left: mirrored distribution
      // Left: 7/16
      errorCurRow[x] += (error * 7) >> 4;
      // Bottom-right: 3/16
      errorNextRow[x + 2] += (error * 3) >> 4;
      // Bottom: 5/16
      errorNextRow[x + 1] += (error * 5) >> 4;
      // Bottom-left: 1/16
      errorNextRow[x] += (error) >> 4;
    }

    return quantized;
  }

  // Call at the end of each row to swap buffers
  void nextRow() {
    if (!isValid()) return;
    // Swap buffers
    int16_t* temp = errorCurRow;
    errorCurRow = errorNextRow;
    errorNextRow = temp;
    // Clear the next row buffer
    memset(errorNextRow, 0, rowSize * sizeof(int16_t));
    rowCount++;
  }

  // Check if current row should be processed in reverse
  bool isReverseRow() const { return (rowCount & 1) != 0; }

  // Reset for a new image or MCU block
  void reset() {
    if (!isValid()) return;
    memset(errorCurRow, 0, rowSize * sizeof(int16_t));
    memset(errorNextRow, 0, rowSize * sizeof(int16_t));
    rowCount = 0;
  }

 private:
  int rowCount;
  size_t rowSize{0};
  std::unique_ptr<int16_t[]> errorRows;
  int16_t* errorCurRow = nullptr;
  int16_t* errorNextRow = nullptr;
  const bool originalThresholds;
};

// Brightness/Contrast adjustments:
constexpr bool USE_BRIGHTNESS = false;       // true: apply brightness/gamma adjustments
constexpr int BRIGHTNESS_BOOST = 10;         // Brightness offset (0-50)
constexpr bool GAMMA_CORRECTION = false;     // Gamma curve (brightens midtones)
constexpr float CONTRAST_FACTOR = 1.15f;     // Contrast multiplier (1.0 = no change, >1 = more contrast)
constexpr bool USE_NOISE_DITHERING = false;  // Hash-based noise dithering

// Integer approximation of gamma correction (brightens midtones)
// Uses a simple curve: out = 255 * sqrt(in/255) ≈ sqrt(in * 255)
static inline int applyGamma(int gray) {
  if (!GAMMA_CORRECTION) return gray;
  // Fast integer square root approximation for gamma ~0.5 (brightening)
  // This brightens dark/mid tones while preserving highlights
  const int product = gray * 255;
  // Newton-Raphson integer sqrt (2 iterations for good accuracy)
  int x = gray;
  if (x > 0) {
    x = (x + product / x) >> 1;
    x = (x + product / x) >> 1;
  }
  return x > 255 ? 255 : x;
}

// Apply contrast adjustment around midpoint (128)
// factor > 1.0 increases contrast, < 1.0 decreases
static inline int applyContrast(int gray) {
  // Integer-based contrast: (gray - 128) * factor + 128
  // Using fixed-point: factor 1.15 ≈ 115/100
  constexpr int factorNum = static_cast<int>(CONTRAST_FACTOR * 100);
  int adjusted = ((gray - 128) * factorNum) / 100 + 128;
  if (adjusted < 0) adjusted = 0;
  if (adjusted > 255) adjusted = 255;
  return adjusted;
}
// Combined brightness/contrast/gamma adjustment
int adjustPixel(int gray) {
  if (!USE_BRIGHTNESS) return gray;

  // Order: contrast first, then brightness, then gamma
  gray = applyContrast(gray);
  gray += BRIGHTNESS_BOOST;
  if (gray > 255) gray = 255;
  if (gray < 0) gray = 0;
  gray = applyGamma(gray);

  return gray;
}
// Simple quantization without dithering - divide into 4 levels
// The thresholds are fine-tuned to the X4 display
uint8_t quantizeSimple(int gray) {
  if (gray < 45) {
    return 0;
  } else if (gray < 70) {
    return 1;
  } else if (gray < 140) {
    return 2;
  } else {
    return 3;
  }
}

// Hash-based noise dithering - survives downsampling without moiré artifacts
// Uses integer hash to generate pseudo-random threshold per pixel
static inline uint8_t quantizeNoise(int gray, int x, int y) {
  uint32_t hash = static_cast<uint32_t>(x) * 374761393u + static_cast<uint32_t>(y) * 668265263u;
  hash = (hash ^ (hash >> 13)) * 1274126177u;
  const int threshold = static_cast<int>(hash >> 24);

  const int scaled = gray * 3;
  if (scaled < 255) {
    return (scaled + threshold >= 255) ? 1 : 0;
  } else if (scaled < 510) {
    return ((scaled - 255) + threshold >= 255) ? 2 : 1;
  } else {
    return ((scaled - 510) + threshold >= 255) ? 3 : 2;
  }
}

// Main quantization function - selects between methods based on config
uint8_t quantize(int gray, int x, int y) {
  if (USE_NOISE_DITHERING) {
    return quantizeNoise(gray, x, y);
  } else {
    return quantizeSimple(gray);
  }
}

// 1-bit noise dithering for fast home screen rendering
// Uses hash-based noise for consistent dithering that works well at small sizes
uint8_t quantize1bit(int gray, int x, int y) {
  gray = adjustPixel(gray);

  // Generate noise threshold using integer hash (no regular pattern to alias)
  uint32_t hash = static_cast<uint32_t>(x) * 374761393u + static_cast<uint32_t>(y) * 668265263u;
  hash = (hash ^ (hash >> 13)) * 1274126177u;
  const int threshold = static_cast<int>(hash >> 24);  // 0-255

  // Simple threshold with noise: gray >= (128 + noise offset) -> white
  // The noise adds variation around the 128 midpoint
  const int adjustedThreshold = 128 + ((threshold - 128) / 2);  // Range: 64-192
  return (gray >= adjustedThreshold) ? 1 : 0;
}

}  // namespace old
