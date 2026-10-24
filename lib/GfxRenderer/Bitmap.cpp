#include "Bitmap.h"

#include <Logging.h>

#include <algorithm>
#include <cstddef>
#include <cstdlib>
#include <cstring>

// bembeyaz-ffi/src/bmp.rs checks the same layout; Error in bembeyaz-bmp uses these numbers.
static_assert(sizeof(bb_bmp) == 280 && offsetof(bb_bmp, height) == 4 && offsetof(bb_bmp, row_bytes) == 8 &&
                  offsetof(bb_bmp, data_offset) == 12 && offsetof(bb_bmp, colors_used) == 16 &&
                  offsetof(bb_bmp, bits_per_pixel) == 20 && offsetof(bb_bmp, top_down) == 22 &&
                  offsetof(bb_bmp, native_palette) == 23 && offsetof(bb_bmp, palette_lum) == 24,
              "bb_bmp layout");
namespace {
constexpr bool parseResultsMatch() {
  constexpr BmpReaderError order[] = {BmpReaderError::FileInvalid,
                                      BmpReaderError::SeekStartFailed,
                                      BmpReaderError::NotBMP,
                                      BmpReaderError::DIBTooSmall,
                                      BmpReaderError::BadPlanes,
                                      BmpReaderError::UnsupportedBpp,
                                      BmpReaderError::UnsupportedCompression,
                                      BmpReaderError::BadDimensions,
                                      BmpReaderError::ImageTooLarge,
                                      BmpReaderError::PaletteTooLarge,
                                      BmpReaderError::SeekPixelDataFailed};
  for (int i = 0; i < 11; i++) {
    if (static_cast<int>(order[i]) != i + 1) return false;
  }
  return true;
}
}  // namespace
static_assert(parseResultsMatch(), "bb_bmp_parse results");

// ============================================================================
// IMAGE PROCESSING OPTIONS
// ============================================================================
// Dithering is applied when converting high-color BMPs to the display's native
// 2-bit (4-level) grayscale. Images whose palette entries all map to native
// gray levels (0, 85, 170, 255 ±21) are mapped directly without dithering.
// For cover images, dithering is done in JpegToBmpConverter.cpp instead.
constexpr bool USE_ATKINSON = true;  // Use Atkinson dithering instead of Floyd-Steinberg
// ============================================================================

Bitmap::~Bitmap() = default;

const char* Bitmap::errorToString(BmpReaderError err) {
  switch (err) {
    case BmpReaderError::Ok:
      return "Ok";
    case BmpReaderError::FileInvalid:
      return "FileInvalid";
    case BmpReaderError::SeekStartFailed:
      return "SeekStartFailed";
    case BmpReaderError::NotBMP:
      return "NotBMP (missing 'BM')";
    case BmpReaderError::DIBTooSmall:
      return "DIBTooSmall (<40 bytes)";
    case BmpReaderError::BadPlanes:
      return "BadPlanes (!= 1)";
    case BmpReaderError::UnsupportedBpp:
      return "UnsupportedBpp (expected 1, 2, 4, 8, 24, or 32)";
    case BmpReaderError::UnsupportedCompression:
      return "UnsupportedCompression (expected BI_RGB or BI_BITFIELDS for 32bpp)";
    case BmpReaderError::BadDimensions:
      return "BadDimensions";
    case BmpReaderError::ImageTooLarge:
      return "ImageTooLarge (max 2048x3072)";
    case BmpReaderError::PaletteTooLarge:
      return "PaletteTooLarge";

    case BmpReaderError::SeekPixelDataFailed:
      return "SeekPixelDataFailed";
    case BmpReaderError::BufferTooSmall:
      return "BufferTooSmall";

    case BmpReaderError::OomRowBuffer:
      return "OomRowBuffer";
    case BmpReaderError::OomDitherer:
      return "OomDitherer";
    case BmpReaderError::ShortReadRow:
      return "ShortReadRow";
  }
  return "Unknown";
}

BmpReaderError Bitmap::parseHeaders() {
  if (!file) return BmpReaderError::FileInvalid;
  const auto read = [](void* f, uint8_t* buf, const size_t len) -> size_t {
    const int count = static_cast<HalFile*>(f)->read(buf, len);
    return count > 0 ? static_cast<size_t>(count) : 0;
  };
  const auto skip = [](void* f, const uint32_t count) { static_cast<HalFile*>(f)->seekCur(count); };
  const auto seek = [](void* f, const uint32_t position) { return static_cast<HalFile*>(f)->seek(position); };
  const auto result = static_cast<BmpReaderError>(bb_bmp_parse(&bmp, read, skip, seek, &file));
  if (result != BmpReaderError::Ok) return result;

  // Decide pixel processing strategy:
  //  - Native palette → direct mapping, no processing needed
  //  - High-color + dithering enabled → error-diffusion dithering (Atkinson or Floyd-Steinberg)
  //  - High-color + dithering disabled → simple quantization (no error diffusion)
  const bool highColor = !bmp.native_palette;
  uint8_t method = BB_DITHER_THRESHOLD;
  if (!highColor) {
    method = BB_DITHER_NATIVE;
  } else if (dithering) {
    method = USE_ATKINSON ? BB_DITHER_ATKINSON : BB_DITHER_FLOYD_STEINBERG;
  }
  ditherer.emplace(method, bmp.width, originalThresholds);
  if (!ditherer->isValid()) {
    ditherer.reset();
    LOG_ERR("BMP", "OOM: ditherer or row buffers");
    return BmpReaderError::OomDitherer;
  }

  return BmpReaderError::Ok;
}

// packed 2bpp output, 0 = black, 1 = dark gray, 2 = light gray, 3 = white
BmpReaderError Bitmap::readNextRow(uint8_t* data, uint8_t* rowBuffer) const {
  // Note: rowBuffer should be pre-allocated by the caller to size 'rowBytes'
  if (file.read(rowBuffer, bmp.row_bytes) != bmp.row_bytes) return BmpReaderError::ShortReadRow;

  prevRowY += 1;

  constexpr int CHUNK = 64;
  uint8_t lum[CHUNK];
  const int width = bmp.width;
  // Packed 2-bit levels; the unused bits of a last partial byte stay 0.
  const size_t outLen = width > 0 ? (static_cast<size_t>(width) + 3) / 4 : 0;
  if (width > 0 && width % 4 != 0) data[outLen - 1] = 0;
  int first = 0;
  // At least one call, so an unparsed bitmap still returns UnsupportedBpp.
  do {
    const int count = std::max(0, std::min(CHUNK, width - first));
    if (!bb_bmp_row_luminance(&bmp, rowBuffer, bmp.row_bytes, first, lum, count)) {
      return BmpReaderError::UnsupportedBpp;
    }
    if (ditherer) ditherer->pixels(lum, count, first, prevRowY, data, outLen);
    first += CHUNK;
  } while (first < width);
  if (ditherer) ditherer->nextRow();

  return BmpReaderError::Ok;
}

BmpReaderError Bitmap::rewindToData() const {
  if (!file.seek(bmp.data_offset)) {
    return BmpReaderError::SeekPixelDataFailed;
  }

  // Reset dithering when rewinding
  if (ditherer) ditherer->reset();

  return BmpReaderError::Ok;
}
