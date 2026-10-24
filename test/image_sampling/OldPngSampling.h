// The old PNG framebuffer converter's draw callback (lib/Epub/Epub/converters/
// PngToFramebufferConverter.cpp at 38e2e5ef) with its alphaThreshold4x4, verbatim but for the namespace,
// over just enough of its context: the oracle for bembeyaz-dither's line rows. convertLineToGray stands
// in for the converter's own (unchanged): row pixels are the gray line, then the alpha line.
#pragma once

#include <bembeyaz/png.h>

#include <cstdint>
#include <cstring>

#include "OldJpegSampling.h"

namespace oldPng {

using oldJpeg::applyBayerDither4Level;
using oldJpeg::DirectCacheWriter;
using oldJpeg::GfxRenderer;
using oldJpeg::ImageToFramebufferDecoder;
using oldJpeg::PixelCache;
using oldLevels::DirectPixelWriter;

struct RenderConfig {
  int x, y;
  bool useDithering;
};

struct PngContext {
  GfxRenderer* renderer;
  const RenderConfig* config;
  int screenWidth, screenHeight;
  int srcWidth, cropLeft, cropTop, visibleWidth, visibleHeight, dstWidth, dstHeight;
  int lastDstY;
  PixelCache cache;
  bool caching;
  uint8_t* grayLineBuffer;
  uint8_t* alphaLineBuffer;
  uint32_t lastYieldMs;
};

inline void convertLineToGray(const uint8_t* pPixels, uint8_t* grayLine, int width, int, int, const uint8_t*, int,
                              uint32_t, uint8_t* alphaLine) {
  memcpy(grayLine, pPixels, width);
  if (alphaLine) memcpy(alphaLine, pPixels + width, width);
}

uint8_t alphaThreshold4x4(const int x, const int y) {
  static constexpr uint8_t BAYER_4X4[16] = {0, 128, 32, 160, 192, 64, 224, 96, 48, 176, 16, 144, 240, 112, 208, 80};
  return BAYER_4X4[((y & 0x03) << 2) | (x & 0x03)];
}

int pngDrawCallback(void* user, const bb_png_row* row) {
  PngContext* ctx = static_cast<PngContext*>(user);
  if (!ctx || !ctx->config || !ctx->renderer || !ctx->grayLineBuffer) return 0;

  ImageToFramebufferDecoder::yieldDuringDecode(ctx->lastYieldMs);

  int srcY = row->y;
  int srcWidth = ctx->srcWidth;
  if (srcY < ctx->cropTop || srcY >= ctx->cropTop + ctx->visibleHeight) return 1;
  const int visibleSrcY = srcY - ctx->cropTop;

  // Map source rows with the exact output-height ratio. During downscaling,
  // multiple source rows can select the same output row; during upscaling, one
  // source row must be repeated across every output row in its range. Emitting
  // only the first row of an upscale leaves zero-filled (black) gaps in the
  // streamed pixel cache.
  int firstDstY = (visibleSrcY * ctx->dstHeight) / ctx->visibleHeight;
  int endDstY = firstDstY + 1;
  if (ctx->dstHeight > ctx->visibleHeight) {
    endDstY = ((visibleSrcY + 1) * ctx->dstHeight) / ctx->visibleHeight;
  }

  if (firstDstY <= ctx->lastDstY) firstDstY = ctx->lastDstY + 1;
  if (firstDstY >= endDstY || firstDstY >= ctx->dstHeight) return 1;
  if (endDstY > ctx->dstHeight) endDstY = ctx->dstHeight;

  // The decoder parses tRNS while decoding, so each row carries the transparency as it stands.
  convertLineToGray(row->pixels, ctx->grayLineBuffer, srcWidth, row->pixel_type, row->bpp, row->palette, row->has_alpha,
                    row->transparent, ctx->alphaLineBuffer);

  // Render scaled rows using Bresenham-style integer stepping (no floating-point division)
  int dstWidth = ctx->dstWidth;
  int outXBase = ctx->config->x;
  int screenWidth = ctx->screenWidth;
  bool useDithering = ctx->config->useDithering;

  // Pre-compute orientation and render-mode state once per callback.
  DirectPixelWriter pw;
  pw.init(*ctx->renderer);

  for (int dstY = firstDstY; dstY < endDstY; dstY++) {
    ctx->lastDstY = dstY;
    int outY = ctx->config->y + dstY;
    if (outY >= ctx->screenHeight) continue;

    pw.beginRow(outY);

    // The cache streams to disk one row at a time. Flushing rows below this one
    // (the decoder delivers scanlines top to bottom) repositions the single-row band.
    // A flush failure stops caching for the rest of the decode so we never write
    // past the band buffer; finalize() then drops the partial file.
    bool caching = ctx->caching;
    DirectCacheWriter cw;
    if (caching) {
      if (!ctx->cache.advanceTo(dstY)) {
        caching = false;
        ctx->caching = false;
      } else {
        cw.init(ctx->cache.buffer, ctx->cache.bytesPerRow, ctx->cache.bandRows, ctx->cache.originX);
        cw.beginRow(outY, ctx->config->y + ctx->cache.bandStart);
      }
    }

    int srcX = ctx->cropLeft;
    int error = 0;

    for (int dstX = 0; dstX < dstWidth; dstX++) {
      int outX = outXBase + dstX;
      if (outX < screenWidth) {
        const uint8_t alpha = ctx->alphaLineBuffer ? ctx->alphaLineBuffer[srcX] : 255;
        if (alpha >= 8 && alpha > alphaThreshold4x4(outX, outY)) {
          uint8_t gray = ctx->grayLineBuffer[srcX];

          uint8_t ditheredGray;
          if (useDithering) {
            ditheredGray = applyBayerDither4Level(gray, outX, outY);
          } else {
            ditheredGray = gray >> 6;
          }
          pw.writePixel(outX, ditheredGray, ctx->alphaLineBuffer != nullptr);
          if (caching) cw.writePixel(outX, ditheredGray);
        }
      }

      // Bresenham-style stepping: advance srcX based on ratio visibleWidth/dstWidth
      error += ctx->visibleWidth;
      while (error >= dstWidth) {
        error -= dstWidth;
        srcX++;
      }
    }
  }

  return 1;
}

}  // namespace oldPng
