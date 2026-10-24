// bembeyaz-dither's block rows, drawn with bembeyaz-gfx's level rows and copied into the cache
// band as lib/Epub's JPEG framebuffer converter does, against the converter's old draw callback
// (OldJpegSampling.h): random images, scales, crops, positions, panels, strips and blocks must give
// the same screen and the same cache.
#include <bembeyaz/dither.h>
#include <bembeyaz/gfx.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <climits>
#include <cstdint>
#include <vector>

#include "OldJpegSampling.h"

namespace {

uint32_t seed;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}
int range(int lo, int hi) { return lo + static_cast<int>(rnd() % static_cast<uint32_t>(hi - lo + 1)); }

// The new callback (JpegToFramebufferConverter.cpp's jpegDrawCallback), with GfxRenderer's
// drawLevelRow as the canvas it builds.
void newDraw(oldJpeg::JpegContext* ctx, const bb_jpeg_block* block, const bb_gfx_canvas& canvas) {
  const uint8_t* pixels = block->pixels;
  const int stride = block->width;
  const int validW = block->width_used;
  const int blockH = block->height;
  if (stride <= 0 || blockH <= 0 || validW <= 0) return;
  const bool useDithering = ctx->config->useDithering;
  bool caching = ctx->caching;
  const int cfgX = ctx->config->x;
  const int cfgY = ctx->config->y;
  const int blockX = block->x - ctx->cropLeft;
  const int blockY = block->y - ctx->cropTop;
  const int srcYEnd = blockY + blockH;
  const int srcXEnd = blockX + validW;
  int dstYStart = (int)((int64_t)blockY * ctx->fineScaleFPY >> 16);
  int dstYEnd = (srcYEnd >= ctx->scaledSrcHeight) ? ctx->dstHeight : (int)((int64_t)srcYEnd * ctx->fineScaleFPY >> 16);
  int dstXStart = (int)((int64_t)blockX * ctx->fineScaleFPX >> 16);
  int dstXEnd = (srcXEnd >= ctx->scaledSrcWidth) ? ctx->dstWidth : (int)((int64_t)srcXEnd * ctx->fineScaleFPX >> 16);
  int clampYMax = ctx->dstHeight;
  if (ctx->screenHeight - cfgY < clampYMax) clampYMax = ctx->screenHeight - cfgY;
  if (dstYStart < -cfgY) dstYStart = -cfgY;
  if (dstYEnd > clampYMax) dstYEnd = clampYMax;
  int clampXMax = ctx->dstWidth;
  if (ctx->screenWidth - cfgX < clampXMax) clampXMax = ctx->screenWidth - cfgX;
  if (dstXStart < -cfgX) dstXStart = -cfgX;
  if (dstXEnd > clampXMax) dstXEnd = clampXMax;
  if (dstYStart >= dstYEnd || dstXStart >= dstXEnd) return;

  int cacheOriginY = 0;
  if (caching) {
    if (!ctx->cache.advanceTo(dstYStart)) {
      caching = false;
      ctx->caching = false;
    } else {
      cacheOriginY = ctx->config->y + ctx->cache.bandStart;
    }
  }

  const bb_dither_gray_block source{
      pixels, static_cast<size_t>(stride) * blockH, static_cast<size_t>(stride), validW, blockH, blockX, blockY};
  const bb_dither_scale scale{ctx->fineScaleFPX, ctx->invScaleFPX, ctx->fineScaleFPY, ctx->invScaleFPY};
  const uint8_t quantize = useDithering ? BB_DITHER_QUANTIZE_BAYER : BB_DITHER_QUANTIZE_THIRDS;
  // Smaller pieces than the converter's 512 columns, so that rows split.
  constexpr int PIECE = 32;
  uint8_t levels[PIECE / 4];
  for (int dstY = dstYStart; dstY < dstYEnd; dstY++) {
    const int outY = cfgY + dstY;
    const int bandRow = outY - cacheOriginY;
    uint8_t* cacheRow = caching && static_cast<unsigned>(bandRow) < static_cast<unsigned>(ctx->cache.bandRows)
                            ? ctx->cache.buffer + static_cast<size_t>(bandRow) * ctx->cache.bytesPerRow
                            : nullptr;
    for (int from = dstXStart; from < dstXEnd; from += PIECE) {
      const int count = std::min(PIECE, dstXEnd - from);
      bb_dither_block_row(&source, &scale, dstY, from, from + count, cfgX, cfgY, quantize, levels, sizeof(levels));
      bb_gfx_level_row(&canvas, levels, (static_cast<size_t>(count) + 3) / 4, count, cfgX + from, outY,
                       static_cast<uint8_t>(ctx->renderer->mode), ctx->renderer->absolute);
      if (cacheRow) {
        bb_dither_put_levels(cacheRow, ctx->cache.bytesPerRow, cfgX + from - ctx->cache.originX, levels, sizeof(levels),
                             count);
      }
    }
  }
}

}  // namespace

TEST(ImageSampling, JpegBlocksMatchTheOldConverter) {
  seed = 23;
  unsigned diffs = 0;
  for (int c = 0; c < 20000; ++c) {
    oldLevels::GfxRenderer r{};
    r.widthBytes = range(2, 30);
    r.width = r.widthBytes * 8 - (rnd() % 3 == 0 ? range(0, 7) : 0);
    r.height = range(8, 200);
    r.orientation = static_cast<oldLevels::GfxRenderer::Orientation>(rnd() % 4);
    r.mode = static_cast<oldLevels::GfxRenderer::RenderMode>(rnd() % 3);
    r.absolute = rnd() % 2;
    r.originY = 0;
    r.rows = r.height;
    if (rnd() % 3 == 0) {
      r.originY = range(0, r.height - 1);
      r.rows = range(1, r.height - r.originY);
    }
    std::vector<uint8_t> a(static_cast<size_t>(r.rows) * r.widthBytes);
    for (auto& v : a) v = static_cast<uint8_t>(rnd());
    auto b = a;
    const bool portrait =
        r.orientation == oldLevels::GfxRenderer::Portrait || r.orientation == oldLevels::GfxRenderer::PortraitInverted;

    // The decoder's scaled image, cropped, and the output size.
    const int fullW = range(1, 160), fullH = range(1, 120);
    const int cropLeft = rnd() % 4 == 0 ? range(0, (fullW - 1) / 4) : 0;
    const int cropTop = rnd() % 4 == 0 ? range(0, (fullH - 1) / 4) : 0;
    const int srcW = fullW - 2 * cropLeft, srcH = fullH - 2 * cropTop;
    int dstW = srcW, dstH = srcH;
    switch (rnd() % 4) {
      case 1:  // grown
        dstW = std::max(1, srcW * range(100, 400) / 100);
        dstH = std::max(1, srcH * range(100, 400) / 100);
        break;
      case 2:  // shrunk
        dstW = std::max(1, srcW * range(40, 100) / 100);
        dstH = std::max(1, srcH * range(40, 100) / 100);
        break;
      case 3:  // either
        dstW = range(1, 2 * srcW);
        dstH = range(1, 2 * srcH);
        break;
      default:
        break;
    }
    oldJpeg::RenderConfig config{};
    config.useDithering = rnd() % 2;
    oldJpeg::JpegContext ctx{};
    ctx.renderer = &r;
    ctx.config = &config;
    ctx.screenWidth = portrait ? r.height : r.width;
    ctx.screenHeight = portrait ? r.width : r.height;
    config.x = range(0, ctx.screenWidth - 1);
    config.y = range(0, ctx.screenHeight - 1);
    ctx.scaledSrcWidth = srcW;
    ctx.scaledSrcHeight = srcH;
    ctx.cropLeft = cropLeft;
    ctx.cropTop = cropTop;
    ctx.dstWidth = dstW;
    ctx.dstHeight = dstH;
    ctx.fineScaleFPX = (int32_t)((int64_t)dstW * 65536 / srcW);
    ctx.invScaleFPX = (int32_t)((int64_t)srcW * 65536 / dstW);
    ctx.fineScaleFPY = (int32_t)((int64_t)dstH * 65536 / srcH);
    ctx.invScaleFPY = (int32_t)((int64_t)srcH * 65536 / dstH);
    ctx.caching = rnd() % 3 != 0;
    const int bytesPerRow = (dstW + 3) / 4;
    std::vector<uint8_t> ca(static_cast<size_t>(bytesPerRow) * dstH);
    for (auto& v : ca) v = static_cast<uint8_t>(rnd());
    auto cb = ca;
    ctx.cache = {nullptr, bytesPerRow, dstH, config.x, 0};
    auto fresh = ctx;

    bb_gfx_canvas cv{};
    cv.buf = b.data();
    cv.len = b.size();
    cv.stride = static_cast<size_t>(r.widthBytes);
    cv.origin_y = r.originY;
    cv.rows = r.rows;
    cv.panel_width = r.width;
    cv.panel_height = r.height;
    cv.clip_left = INT32_MIN;
    cv.clip_top = INT32_MIN;
    cv.clip_right = INT32_MAX;
    cv.clip_bottom = INT32_MAX;
    cv.orientation = static_cast<uint8_t>(r.orientation);

    // MCU blocks left to right and top to bottom, as bembeyaz-jpeg hands them out.
    const int mcuW = rnd() % 2 ? 8 : 16, mcuH = rnd() % 2 ? 8 : 16;
    const int pitch = mcuW * range(1, 8);
    for (int y = 0; y < fullH; y += mcuH) {
      for (int x = 0; x < fullW; x += pitch) {
        const int height = std::min(mcuH, fullH - y);
        std::vector<uint8_t> pixels(static_cast<size_t>(pitch) * height);
        for (auto& v : pixels) v = static_cast<uint8_t>(rnd());
        const bb_jpeg_block block{pixels.data(), x, y, pitch, height, std::min(pitch, fullW - x)};
        r.target = a.data();
        ctx.cache.buffer = ca.data();
        oldJpeg::jpegDrawCallback(&ctx, &block);
        fresh.cache.buffer = cb.data();
        newDraw(&fresh, &block, cv);
      }
    }
    if (a != b || ca != cb) {
      ADD_FAILURE() << "case " << c << " orientation " << r.orientation << " mode " << r.mode << " src " << srcW << "x"
                    << srcH << " dst " << dstW << "x" << dstH << " at " << config.x << "," << config.y << " dithering "
                    << config.useDithering << (a != b ? " screen" : "") << (ca != cb ? " cache" : "");
      if (++diffs == 5) return;
    }
  }
}
