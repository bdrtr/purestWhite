// The cover converters' row scaling (lib/JpegToBmpConverter/JpegToBmpConverter.cpp at 50cdf52f; the
// area loops are the same in PngToBmpConverter.cpp), the loops verbatim inside small functions over
// just enough of their context: the oracle for bembeyaz-dither's scale module.
#pragma once

#include <algorithm>
#include <cstdint>
#include <memory>

#include "lib/GfxRenderer/BitmapHelpers.h"

namespace oldCover {

constexpr uint32_t FP_ONE = 1UL << 16;

struct BmpConvertCtx {
  int srcWidth;
  int outWidth;
  int currentOutY;
  int bytesPerRow;
  uint32_t scaleX_fp;
  uint32_t smoothScaleX_fp;
  std::unique_ptr<uint32_t[]> rowAccum;
  std::unique_ptr<uint32_t[]> rowCount;
  uint8_t* smoothOutRow;
  std::unique_ptr<uint8_t[]> bmpRow;
  Ditherer* ditherer;
};

static uint32_t interpolatedSourceFp(const int outIndex, const int outSize, const int srcSize, const uint32_t step) {
  if (srcSize <= 1 || outSize <= 1) return 0;
  if (outIndex >= outSize - 1) return static_cast<uint32_t>(srcSize - 1) << 16;
  return static_cast<uint32_t>(outIndex) * step;
}

static void scaleRowLinear(BmpConvertCtx* ctx, const uint8_t* srcRow, uint8_t* dstRow) {
  for (int outX = 0; outX < ctx->outWidth; outX++) {
    const uint32_t srcX_fp = interpolatedSourceFp(outX, ctx->outWidth, ctx->srcWidth, ctx->smoothScaleX_fp);
    const int x0 = srcX_fp >> 16;
    const int x1 = (x0 + 1 < ctx->srcWidth) ? (x0 + 1) : x0;
    const uint32_t fx = srcX_fp & (FP_ONE - 1);
    dstRow[outX] = static_cast<uint8_t>((srcRow[x0] * (FP_ONE - fx) + srcRow[x1] * fx) >> 16);
  }
}

// writeBlendedRow's loop.
static void blendRows(BmpConvertCtx* ctx, const uint8_t* row0, const uint8_t* row1, const uint32_t fy) {
  const uint32_t invFy = FP_ONE - fy;
  for (int outX = 0; outX < ctx->outWidth; outX++) {
    ctx->smoothOutRow[outX] = static_cast<uint8_t>((row0[outX] * invFy + row1[outX] * fy) >> 16);
  }
}

// flushScaledRow's dithering.
static void flushPixels(BmpConvertCtx* ctx) {
  // The averaged gray, 64 pixels at a time.
  uint8_t gray[64];
  for (int x0 = 0; x0 < ctx->outWidth; x0 += 64) {
    const int count = std::min(64, ctx->outWidth - x0);
    for (int i = 0; i < count; i++) {
      const int x = x0 + i;
      gray[i] = (ctx->rowCount[x] > 0) ? (ctx->rowAccum[x] / ctx->rowCount[x]) : 0;
    }
    ctx->ditherer->pixels(gray, count, x0, ctx->currentOutY, ctx->bmpRow.get(), ctx->bytesPerRow);
  }
  ctx->ditherer->nextRow();
}

// bmpDrawCallback's area averaging.
static void areaAdd(BmpConvertCtx* ctx, const uint8_t* srcRow) {
  // Fixed-point area averaging on X axis
  for (int outX = 0; outX < ctx->outWidth; outX++) {
    const int srcXStart = (static_cast<uint32_t>(outX) * ctx->scaleX_fp) >> 16;
    const int srcXEnd = (static_cast<uint32_t>(outX + 1) * ctx->scaleX_fp) >> 16;
    int sum = 0;
    int count = 0;
    for (int srcX = srcXStart; srcX < srcXEnd && srcX < ctx->srcWidth; srcX++) {
      sum += srcRow[srcX];
      count++;
    }
    if (count == 0 && srcXStart < ctx->srcWidth) {
      sum = srcRow[srcXStart];
      count = 1;
    }
    ctx->rowAccum[outX] += sum;
    ctx->rowCount[outX] += count;
  }
}

}  // namespace oldCover
