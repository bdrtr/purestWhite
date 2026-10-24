// bembeyaz-dither's block and line rows, drawn with bembeyaz-gfx's level rows and copied into the
// cache band as lib/Epub's JPEG and PNG framebuffer converters do, against the converters' old draw
// callbacks (OldJpegSampling.h, OldPngSampling.h): random images, scales, crops, positions, panels,
// strips, blocks and transparency must give the same screen and the same cache; and bembeyaz-png's
// row gray against the PNG converter's old convertLineToGray (OldPngGray.h).
#include <bembeyaz/dither.h>
#include <bembeyaz/gfx.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <climits>
#include <cstdint>
#include <vector>

#include "OldJpegSampling.h"
#include "OldPngGray.h"
#include "OldPngSampling.h"

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
      bb_gfx_level_row(&canvas, levels, (static_cast<size_t>(count) + 3) / 4, nullptr, 0, count, cfgX + from, outY,
                       static_cast<uint8_t>(ctx->renderer->mode), ctx->renderer->absolute);
      if (cacheRow) {
        bb_dither_put_levels(cacheRow, ctx->cache.bytesPerRow, cfgX + from - ctx->cache.originX, levels, sizeof(levels),
                             count);
      }
    }
  }
}

// The new PNG callback (PngToFramebufferConverter.cpp's pngDrawCallback), likewise.
void newPngDraw(oldPng::PngContext* ctx, const bb_png_row* row, const bb_gfx_canvas& canvas) {
  int srcY = row->y;
  int srcWidth = ctx->srcWidth;
  if (srcY < ctx->cropTop || srcY >= ctx->cropTop + ctx->visibleHeight) return;
  const int visibleSrcY = srcY - ctx->cropTop;
  int firstDstY = (visibleSrcY * ctx->dstHeight) / ctx->visibleHeight;
  int endDstY = firstDstY + 1;
  if (ctx->dstHeight > ctx->visibleHeight) {
    endDstY = ((visibleSrcY + 1) * ctx->dstHeight) / ctx->visibleHeight;
  }
  if (firstDstY <= ctx->lastDstY) firstDstY = ctx->lastDstY + 1;
  if (firstDstY >= endDstY || firstDstY >= ctx->dstHeight) return;
  if (endDstY > ctx->dstHeight) endDstY = ctx->dstHeight;
  oldPng::convertLineToGray(row->pixels, ctx->grayLineBuffer, srcWidth, row->pixel_type, row->bpp, row->palette,
                            row->has_alpha, row->transparent, ctx->alphaLineBuffer);

  const int outXBase = ctx->config->x;
  const int columns = std::min(ctx->dstWidth, ctx->screenWidth - outXBase);
  const bool transparent = ctx->alphaLineBuffer != nullptr;
  const bb_dither_line line{ctx->grayLineBuffer, ctx->alphaLineBuffer, static_cast<size_t>(srcWidth),
                            ctx->cropLeft,       ctx->visibleWidth,    ctx->dstWidth};
  const uint8_t quantize = ctx->config->useDithering ? BB_DITHER_QUANTIZE_BAYER : BB_DITHER_QUANTIZE_QUARTERS;
  // Smaller pieces than the converter's 512 columns, so that rows split.
  constexpr int PIECE = 24;
  uint8_t levels[PIECE / 4];
  uint8_t mask[PIECE / 8];
  for (int dstY = firstDstY; dstY < endDstY; dstY++) {
    ctx->lastDstY = dstY;
    int outY = ctx->config->y + dstY;
    if (outY >= ctx->screenHeight) continue;
    uint8_t* cacheRow = nullptr;
    if (ctx->caching) {
      if (!ctx->cache.advanceTo(dstY)) {
        ctx->caching = false;
      } else {
        const int bandRow = outY - (ctx->config->y + ctx->cache.bandStart);
        if (static_cast<unsigned>(bandRow) < static_cast<unsigned>(ctx->cache.bandRows)) {
          cacheRow = ctx->cache.buffer + static_cast<size_t>(bandRow) * ctx->cache.bytesPerRow;
        }
      }
    }
    for (int from = 0; from < columns; from += PIECE) {
      const int count = std::min(PIECE, columns - from);
      bb_dither_line_row(&line, from, from + count, outXBase, outY, quantize, levels, sizeof(levels),
                         transparent ? mask : nullptr, sizeof(mask));
      bb_gfx_level_row(&canvas, levels, (static_cast<size_t>(count) + 3) / 4, transparent ? mask : nullptr,
                       transparent ? (static_cast<size_t>(count) + 7) / 8 : 0, count, outXBase + from, outY,
                       static_cast<uint8_t>(ctx->renderer->mode), ctx->renderer->absolute);
      if (cacheRow) {
        bb_dither_put_levels(cacheRow, ctx->cache.bytesPerRow, outXBase + from - ctx->cache.originX, levels,
                             sizeof(levels), count);
      }
    }
  }
}

// A random panel (orientation, plane, strip) with random contents, and the canvas over a copy of it.
struct Panels {
  oldLevels::GfxRenderer r{};
  std::vector<uint8_t> a, b;
  bb_gfx_canvas cv{};
  bool portrait;

  Panels() {
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
    a.resize(static_cast<size_t>(r.rows) * r.widthBytes);
    for (auto& v : a) v = static_cast<uint8_t>(rnd());
    b = a;
    r.target = a.data();
    portrait =
        r.orientation == oldLevels::GfxRenderer::Portrait || r.orientation == oldLevels::GfxRenderer::PortraitInverted;
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
  }
  int screenWidth() const { return portrait ? r.height : r.width; }
  int screenHeight() const { return portrait ? r.width : r.height; }
};

}  // namespace

TEST(ImageSampling, PngLinesMatchTheOldConverter) {
  seed = 29;
  unsigned diffs = 0;
  for (int c = 0; c < 20000; ++c) {
    Panels p;
    const int srcW = range(1, 200), srcH = range(1, 60);
    const int cropLeft = rnd() % 4 == 0 ? range(0, (srcW - 1) / 2) : 0;
    const int cropTop = rnd() % 4 == 0 ? range(0, (srcH - 1) / 2) : 0;
    const int visibleW = srcW - 2 * cropLeft, visibleH = srcH - 2 * cropTop;
    // Fitted (never grown), or exact dimensions (either way).
    const bool exact = rnd() % 2;
    const int dstW = exact ? range(1, 2 * visibleW) : range(1, visibleW);
    const int dstH = exact ? range(1, 2 * visibleH) : range(1, visibleH);
    oldPng::RenderConfig config{};
    config.useDithering = rnd() % 2;
    config.x = range(0, p.screenWidth() - 1);
    config.y = range(0, p.screenHeight() - 1);
    const bool alpha = rnd() % 3 == 0;
    std::vector<uint8_t> grayA(srcW * (alpha ? 2 : 1)), grayB(grayA.size());
    oldPng::PngContext ctx{};
    ctx.renderer = &p.r;
    ctx.config = &config;
    ctx.screenWidth = p.screenWidth();
    ctx.screenHeight = p.screenHeight();
    ctx.srcWidth = srcW;
    ctx.cropLeft = cropLeft;
    ctx.cropTop = cropTop;
    ctx.visibleWidth = visibleW;
    ctx.visibleHeight = visibleH;
    ctx.dstWidth = dstW;
    ctx.dstHeight = dstH;
    ctx.lastDstY = -1;
    ctx.caching = !alpha && rnd() % 3 != 0;
    const int bytesPerRow = (dstW + 3) / 4;
    std::vector<uint8_t> ca(static_cast<size_t>(bytesPerRow) * dstH);
    for (auto& v : ca) v = static_cast<uint8_t>(rnd());
    auto cb = ca;
    ctx.cache = {ca.data(), bytesPerRow, dstH, config.x, 0};
    auto fresh = ctx;
    fresh.cache.buffer = cb.data();
    ctx.grayLineBuffer = grayA.data();
    ctx.alphaLineBuffer = alpha ? grayA.data() + srcW : nullptr;
    fresh.grayLineBuffer = grayB.data();
    fresh.alphaLineBuffer = alpha ? grayB.data() + srcW : nullptr;

    for (int y = 0; y < srcH; ++y) {
      std::vector<uint8_t> pixels(srcW * 2);
      for (auto& v : pixels) v = static_cast<uint8_t>(rnd());
      // Alpha: often fully clear or opaque.
      for (int x = 0; x < srcW; ++x) {
        const uint32_t k = rnd() % 4;
        if (k == 0) pixels[srcW + x] = 0;
        if (k == 1) pixels[srcW + x] = 255;
      }
      bb_png_row row{};
      row.pixels = pixels.data();
      row.y = y;
      row.width = srcW;
      oldPng::pngDrawCallback(&ctx, &row);
      newPngDraw(&fresh, &row, p.cv);
    }
    if (p.a != p.b || ca != cb) {
      ADD_FAILURE() << "case " << c << " orientation " << p.r.orientation << " mode " << p.r.mode << " src " << srcW
                    << "x" << srcH << " dst " << dstW << "x" << dstH << " at " << config.x << "," << config.y
                    << " alpha " << alpha << " dithering " << config.useDithering << (p.a != p.b ? " screen" : "")
                    << (ca != cb ? " cache" : "");
      if (++diffs == 5) return;
    }
  }
}

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

TEST(ImageSampling, PngRowGrayMatchesTheOldConversion) {
  seed = 31;
  unsigned diffs = 0;
  static constexpr uint8_t TYPES[] = {BB_PNG_PIXEL_GRAYSCALE,
                                      BB_PNG_PIXEL_TRUECOLOR,
                                      BB_PNG_PIXEL_INDEXED,
                                      BB_PNG_PIXEL_GRAY_ALPHA,
                                      BB_PNG_PIXEL_TRUECOLOR_ALPHA,
                                      1,
                                      5};
  for (int c = 0; c < 20000; ++c) {
    const uint8_t type = TYPES[rnd() % 7];
    const bool packed = type == BB_PNG_PIXEL_GRAYSCALE || type == BB_PNG_PIXEL_INDEXED;
    static constexpr uint8_t DEPTHS[] = {1, 2, 4, 8};
    const uint8_t bpp = packed ? DEPTHS[rnd() % 4] : 8;
    const int channels = type == BB_PNG_PIXEL_TRUECOLOR         ? 3
                         : type == BB_PNG_PIXEL_GRAY_ALPHA      ? 2
                         : type == BB_PNG_PIXEL_TRUECOLOR_ALPHA ? 4
                                                                : 1;
    const int width = range(1, 300);
    const int pitch = (width * bpp * channels + 7) / 8;
    std::vector<uint8_t> pixels(pitch), palette(1024);
    for (auto& v : pixels) v = static_cast<uint8_t>(rnd());
    for (auto& v : palette) v = static_cast<uint8_t>(rnd());
    // A color key that some pixels match.
    uint32_t key = rnd();
    if (rnd() % 2) {
      if (type == BB_PNG_PIXEL_TRUECOLOR) {
        const int x = range(0, width - 1);
        key = (uint32_t{pixels[x * 3]} << 16) | (uint32_t{pixels[x * 3 + 1]} << 8) | pixels[x * 3 + 2];
      } else {
        key = rnd() % (1u << bpp);
      }
    }
    bb_png_row row{};
    row.pixels = pixels.data();
    row.palette = palette.data();
    row.width = width;
    row.pitch = pitch;
    row.transparent = key;
    row.pixel_type = type;
    row.bpp = bpp;
    row.has_alpha = rnd() % 2;
    const bool keepAlpha = rnd() % 2;
    std::vector<uint8_t> grayA(width, 0x5A), grayB(width, 0x5A), alphaA(width, 0xA5), alphaB(width, 0xA5);
    oldPngGray::convertLineToGray(pixels.data(), grayA.data(), width, type, bpp, palette.data(), row.has_alpha, key,
                                  keepAlpha ? alphaA.data() : nullptr);
    bb_png_row_gray(&row, grayB.data(), grayB.size(), keepAlpha ? alphaB.data() : nullptr);
    if (grayA != grayB || alphaA != alphaB) {
      ADD_FAILURE() << "case " << c << " type " << int{type} << " bpp " << int{bpp} << " width " << width
                    << " has_alpha " << row.has_alpha << " keep alpha " << keepAlpha << (grayA != grayB ? " gray" : "")
                    << (alphaA != alphaB ? " alpha" : "");
      if (++diffs == 5) return;
    }
  }
}
