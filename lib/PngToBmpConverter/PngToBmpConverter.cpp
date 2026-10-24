#include "PngToBmpConverter.h"

#include <HalDisplay.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <bembeyaz/png_cover.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>

#include <cstddef>
#include <cstdio>
#include <cstring>

#include "BitmapHelpers.h"

// png_cover.rs checks the same layout.
static_assert(offsetof(bb_png_cover_info, height) == 4);

// ============================================================================
// IMAGE PROCESSING OPTIONS - Same as JpegToBmpConverter for consistency
// ============================================================================
constexpr bool USE_8BIT_OUTPUT = false;
constexpr bool USE_ATKINSON = true;
constexpr bool USE_FLOYD_STEINBERG = false;
constexpr bool USE_PRESCALE = true;
// ============================================================================

// BMP writing helpers (same as JpegToBmpConverter)
inline void write16(Print& out, const uint16_t value) {
  out.write(value & 0xFF);
  out.write((value >> 8) & 0xFF);
}

inline void write32(Print& out, const uint32_t value) {
  out.write(value & 0xFF);
  out.write((value >> 8) & 0xFF);
  out.write((value >> 16) & 0xFF);
  out.write((value >> 24) & 0xFF);
}

inline void write32Signed(Print& out, const int32_t value) {
  out.write(value & 0xFF);
  out.write((value >> 8) & 0xFF);
  out.write((value >> 16) & 0xFF);
  out.write((value >> 24) & 0xFF);
}

namespace {
void yieldDuringDecode(uint8_t& rowsSinceYield) {
  if (++rowsSinceYield < 8) return;
  rowsSinceYield = 0;
  vTaskDelay(1);
}

void writeBmpHeader8bit(Print& bmpOut, const int width, const int height) {
  const int bytesPerRow = (width + 3) / 4 * 4;
  const int imageSize = bytesPerRow * height;
  const uint32_t paletteSize = 256 * 4;
  const uint32_t fileSize = 14 + 40 + paletteSize + imageSize;

  bmpOut.write('B');
  bmpOut.write('M');
  write32(bmpOut, fileSize);
  write32(bmpOut, 0);
  write32(bmpOut, 14 + 40 + paletteSize);

  write32(bmpOut, 40);
  write32Signed(bmpOut, width);
  write32Signed(bmpOut, -height);
  write16(bmpOut, 1);
  write16(bmpOut, 8);
  write32(bmpOut, 0);
  write32(bmpOut, imageSize);
  write32(bmpOut, 2835);
  write32(bmpOut, 2835);
  write32(bmpOut, 256);
  write32(bmpOut, 256);

  for (int i = 0; i < 256; i++) {
    bmpOut.write(static_cast<uint8_t>(i));
    bmpOut.write(static_cast<uint8_t>(i));
    bmpOut.write(static_cast<uint8_t>(i));
    bmpOut.write(static_cast<uint8_t>(0));
  }
}

void writeBmpHeader1bit(Print& bmpOut, const int width, const int height) {
  const int bytesPerRow = (width + 31) / 32 * 4;
  const int imageSize = bytesPerRow * height;
  const uint32_t fileSize = 62 + imageSize;

  bmpOut.write('B');
  bmpOut.write('M');
  write32(bmpOut, fileSize);
  write32(bmpOut, 0);
  write32(bmpOut, 62);

  write32(bmpOut, 40);
  write32Signed(bmpOut, width);
  write32Signed(bmpOut, -height);
  write16(bmpOut, 1);
  write16(bmpOut, 1);
  write32(bmpOut, 0);
  write32(bmpOut, imageSize);
  write32(bmpOut, 2835);
  write32(bmpOut, 2835);
  write32(bmpOut, 2);
  write32(bmpOut, 2);

  uint8_t palette[8] = {0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0x00};
  for (const uint8_t i : palette) {
    bmpOut.write(i);
  }
}

void writeBmpHeader2bit(Print& bmpOut, const int width, const int height) {
  const int bytesPerRow = (width * 2 + 31) / 32 * 4;
  const int imageSize = bytesPerRow * height;
  const uint32_t fileSize = 70 + imageSize;

  bmpOut.write('B');
  bmpOut.write('M');
  write32(bmpOut, fileSize);
  write32(bmpOut, 0);
  write32(bmpOut, 70);

  write32(bmpOut, 40);
  write32Signed(bmpOut, width);
  write32Signed(bmpOut, -height);
  write16(bmpOut, 1);
  write16(bmpOut, 2);
  write32(bmpOut, 0);
  write32(bmpOut, imageSize);
  write32(bmpOut, 2835);
  write32(bmpOut, 2835);
  write32(bmpOut, 4);
  write32(bmpOut, 4);

  uint8_t palette[16] = {0x00, 0x00, 0x00, 0x00, 0x55, 0x55, 0x55, 0x00,
                         0xAA, 0xAA, 0xAA, 0x00, 0xFF, 0xFF, 0xFF, 0x00};
  for (const uint8_t i : palette) {
    bmpOut.write(i);
  }
}

// The caller's file, which the reader reads at a position.
struct PngCoverFile {
  HalFile* file;
  uint32_t position;
};

size_t pngCoverRead(void* context, const uint32_t pos, uint8_t* buf, const size_t len) {
  auto& f = *static_cast<PngCoverFile*>(context);
  if (pos != f.position) {
    if (!f.file->seekSet(pos)) return 0;
    f.position = pos;
  }
  const int n = f.file->read(buf, len);
  if (n <= 0) return 0;
  f.position += n;
  return static_cast<size_t>(n);
}

// What the row callback writes with, passed through bb_png_cover_decode.
struct BmpRowCtx {
  Print* bmpOut;
  bool oneBit;
  bool needsScaling;
  uint32_t width;
  int outWidth;
  int outHeight;
  int bytesPerRow;
  uint8_t* rowBuffer;
  Ditherer* ditherer;
  uint32_t* rowAccum;
  uint32_t* rowCount;
  uint32_t scaleX_fp;
  uint32_t scaleY_fp;
  int currentOutY;
  uint32_t nextOutY_srcStart;
  // Source rows so far.
  uint32_t y;
  uint8_t rowsSinceYield;
};

// Row callback: one source row of gray pixels, top to bottom.
int bmpRowCallback(void* user, const uint8_t* grayRow) {
  BmpRowCtx& c = *static_cast<BmpRowCtx*>(user);
  const uint32_t y = c.y++;
  uint8_t* rowBuffer = c.rowBuffer;
  const int bytesPerRow = c.bytesPerRow;
  const int outWidth = c.outWidth;

  if (!c.needsScaling) {
    // Direct output (no scaling)
    memset(rowBuffer, 0, bytesPerRow);

    if (USE_8BIT_OUTPUT && !c.oneBit) {
      for (int x = 0; x < outWidth; x++) {
        rowBuffer[x] = adjustPixel(grayRow[x]);
      }
    } else {
      c.ditherer->pixels(grayRow, outWidth, 0, static_cast<int>(y), rowBuffer, bytesPerRow);
      c.ditherer->nextRow();
    }
    c.bmpOut->write(rowBuffer, bytesPerRow);
    yieldDuringDecode(c.rowsSinceYield);
    return 1;
  }

  // Area-averaging scaling (same as JpegToBmpConverter)
  bb_dither_area_add(grayRow, c.width, c.scaleX_fp, c.rowAccum, c.rowCount, static_cast<size_t>(outWidth));

  // Check if we've crossed into the next output row(s)
  const uint32_t srcY_fp = static_cast<uint32_t>(y + 1) << 16;

  // Output all rows whose boundaries we've crossed (handles both up and downscaling)
  // For upscaling, one source row may produce multiple output rows
  while (srcY_fp >= c.nextOutY_srcStart && c.currentOutY < c.outHeight) {
    memset(rowBuffer, 0, bytesPerRow);

    if (USE_8BIT_OUTPUT && !c.oneBit) {
      for (int x = 0; x < outWidth; x++) {
        const uint8_t gray = (c.rowCount[x] > 0) ? (c.rowAccum[x] / c.rowCount[x]) : 0;
        rowBuffer[x] = adjustPixel(gray);
      }
    } else {
      c.ditherer->areaPixels(c.rowAccum, c.rowCount, outWidth, c.currentOutY, rowBuffer, bytesPerRow);
      c.ditherer->nextRow();
    }

    c.bmpOut->write(rowBuffer, bytesPerRow);
    c.currentOutY++;
    yieldDuringDecode(c.rowsSinceYield);

    c.nextOutY_srcStart = static_cast<uint32_t>(c.currentOutY + 1) * c.scaleY_fp;

    // For upscaling: don't reset accumulators if next output row uses same source data
    // Only reset when we'll move to a new source row
    if (srcY_fp >= c.nextOutY_srcStart) {
      // More output rows to emit from same source - keep accumulator data
      continue;
    }
    // Moving to next source row - reset accumulators
    memset(c.rowAccum, 0, outWidth * sizeof(uint32_t));
    memset(c.rowCount, 0, static_cast<size_t>(outWidth) * sizeof(uint32_t));
  }
  return 1;
}
}  // namespace

bool PngToBmpConverter::pngFileToBmpStreamInternal(HalFile& pngFile, Print& bmpOut, int targetWidth, int targetHeight,
                                                   bool oneBit, bool crop, bool originalThresholds) {
  LOG_DBG("PNG", "Converting PNG to %s BMP (target: %dx%d)", oneBit ? "1-bit" : "2-bit", targetWidth, targetHeight);

  pngFile.seekSet(0);
  PngCoverFile file{&pngFile, 0};
  int status = BB_PNG_COVER_NOT_OPEN;
  const std::unique_ptr<bb_png_cover, decltype(&bb_png_cover_free)> png(
      bb_png_cover_open(pngCoverRead, &file, static_cast<uint32_t>(pngFile.size()), &status), bb_png_cover_free);
  if (status != BB_PNG_COVER_OK) {
    LOG_ERR("PNG", "PNG open failed (err=%d)", status);
    return false;
  }
  bb_png_cover_info info{};
  bb_png_cover_get_info(png.get(), &info);
  const uint32_t width = info.width;
  const uint32_t height = info.height;
  LOG_DBG("PNG", "Image: %ux%u", width, height);

  // Calculate output dimensions (same logic as JpegToBmpConverter)
  int outWidth = width;
  int outHeight = height;
  uint32_t scaleX_fp = 65536;
  uint32_t scaleY_fp = 65536;
  bool needsScaling = false;

  if (targetWidth > 0 && targetHeight > 0 &&
      (static_cast<int>(width) != targetWidth || static_cast<int>(height) != targetHeight)) {
    const float scaleToFitWidth = static_cast<float>(targetWidth) / width;
    const float scaleToFitHeight = static_cast<float>(targetHeight) / height;
    float scale = 1.0;
    if (crop) {
      scale = (scaleToFitWidth > scaleToFitHeight) ? scaleToFitWidth : scaleToFitHeight;
    } else {
      scale = (scaleToFitWidth < scaleToFitHeight) ? scaleToFitWidth : scaleToFitHeight;
    }

    outWidth = static_cast<int>(width * scale);
    outHeight = static_cast<int>(height * scale);
    if (outWidth < 1) outWidth = 1;
    if (outHeight < 1) outHeight = 1;

    scaleX_fp = (width << 16) / outWidth;
    scaleY_fp = (height << 16) / outHeight;
    needsScaling = true;

    LOG_DBG("PNG", "Scaling %ux%u -> %dx%d (target %dx%d)", width, height, outWidth, outHeight, targetWidth,
            targetHeight);
  }

  // Write BMP header
  int bytesPerRow;
  if (USE_8BIT_OUTPUT && !oneBit) {
    writeBmpHeader8bit(bmpOut, outWidth, outHeight);
    bytesPerRow = (outWidth + 3) / 4 * 4;
  } else if (oneBit) {
    writeBmpHeader1bit(bmpOut, outWidth, outHeight);
    bytesPerRow = (outWidth + 31) / 32 * 4;
  } else {
    writeBmpHeader2bit(bmpOut, outWidth, outHeight);
    bytesPerRow = (outWidth * 2 + 31) / 32 * 4;
  }

  auto rowBuffer = makeUniqueNoThrow<uint8_t[]>(bytesPerRow);
  if (!rowBuffer) {
    LOG_ERR("PNG", "OOM: row buffer (%d bytes)", bytesPerRow);
    return false;
  }

  // The ditherer (same as JpegToBmpConverter)
  uint8_t method = BB_DITHER_THRESHOLD;
  if (oneBit) {
    method = BB_DITHER_ATKINSON_1BIT;
  } else if (USE_ATKINSON) {
    method = BB_DITHER_ATKINSON;
  } else if (USE_FLOYD_STEINBERG) {
    method = BB_DITHER_FLOYD_STEINBERG;
  }
  auto ditherer = makeUniqueNoThrow<Ditherer>(method, outWidth, originalThresholds);
  if (!ditherer || !ditherer->isValid()) {
    LOG_ERR("PNG", "OOM: ditherer or row buffers");
    return false;
  }

  // Scaling accumulators
  std::unique_ptr<uint32_t[]> rowAccum;
  std::unique_ptr<uint32_t[]> rowCount;
  uint32_t nextOutY_srcStart = 0;

  if (needsScaling) {
    rowAccum = makeUniqueNoThrow<uint32_t[]>(outWidth);
    rowCount = makeUniqueNoThrow<uint32_t[]>(outWidth);
    if (!rowAccum || !rowCount) {
      LOG_ERR("PNG", "OOM: scaling accumulators");
      return false;
    }
    nextOutY_srcStart = scaleY_fp;
  }

  BmpRowCtx ctx{.bmpOut = &bmpOut,
                .oneBit = oneBit,
                .needsScaling = needsScaling,
                .width = width,
                .outWidth = outWidth,
                .outHeight = outHeight,
                .bytesPerRow = bytesPerRow,
                .rowBuffer = rowBuffer.get(),
                .ditherer = ditherer.get(),
                .rowAccum = rowAccum.get(),
                .rowCount = rowCount.get(),
                .scaleX_fp = scaleX_fp,
                .scaleY_fp = scaleY_fp,
                .currentOutY = 0,
                .nextOutY_srcStart = nextOutY_srcStart,
                .y = 0,
                .rowsSinceYield = 0};
  status = bb_png_cover_decode(png.get(), bmpRowCallback, &ctx);
  if (status == BB_PNG_COVER_MEMORY) {
    LOG_ERR("PNG", "OOM: decode buffers for %u-pixel rows", width);
    return false;
  }
  if (status != BB_PNG_COVER_OK) {
    LOG_ERR("PNG", "Failed to decode scanline %u (err=%d)", ctx.y, status);
    return false;
  }

  LOG_DBG("PNG", "Successfully converted PNG to BMP");
  return true;
}

bool PngToBmpConverter::pngFileToBmpStream(HalFile& pngFile, Print& bmpOut, bool crop, bool originalThresholds) {
  // Use runtime display dimensions (swapped for portrait cover sizing)
  const int targetWidth = display.getDisplayHeight();
  const int targetHeight = display.getDisplayWidth();
  return pngFileToBmpStreamInternal(pngFile, bmpOut, targetWidth, targetHeight, false, crop, originalThresholds);
}

bool PngToBmpConverter::pngFileToBmpStreamWithSize(HalFile& pngFile, Print& bmpOut, int targetMaxWidth,
                                                   int targetMaxHeight) {
  return pngFileToBmpStreamInternal(pngFile, bmpOut, targetMaxWidth, targetMaxHeight, false);
}

bool PngToBmpConverter::pngFileTo1BitBmpStreamWithSize(HalFile& pngFile, Print& bmpOut, int targetMaxWidth,
                                                       int targetMaxHeight) {
  return pngFileToBmpStreamInternal(pngFile, bmpOut, targetMaxWidth, targetMaxHeight, true, true);
}
