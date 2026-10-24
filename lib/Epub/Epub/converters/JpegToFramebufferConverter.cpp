#include "JpegToFramebufferConverter.h"

#include <FsHelpers.h>
#include <GfxRenderer.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <bembeyaz/dither.h>
#include <bembeyaz/jpeg.h>

#include <algorithm>
#include <cstddef>
#include <cstdlib>
#include <memory>

#include "PixelCache.h"

// jpeg.h's layouts, which bembeyaz-ffi's jpeg.rs checks from its side.
static_assert(offsetof(bb_jpeg_info, progressive) == 8);
static_assert(offsetof(bb_jpeg_block, x) == sizeof(void*) && offsetof(bb_jpeg_block, width_used) == sizeof(void*) + 16);
// dither.h's, which bembeyaz-ffi's dither.rs checks too.
static_assert(sizeof(bb_dither_gray_block) == 3 * sizeof(size_t) + 16 &&
              offsetof(bb_dither_gray_block, width) == 3 * sizeof(size_t));
static_assert(sizeof(bb_dither_scale) == 16 && offsetof(bb_dither_scale, inv_y) == 12);

namespace {

// What the draw callback works with, passed through bb_jpeg_decode.
struct JpegContext {
  GfxRenderer* renderer{nullptr};
  const RenderConfig* config{nullptr};
  int screenWidth{0};
  int screenHeight{0};

  // Visible source dimensions after the decoder's built-in scaling
  int scaledSrcWidth{0};
  int scaledSrcHeight{0};
  int cropLeft{0};
  int cropTop{0};

  // Final output dimensions
  int dstWidth{0};
  int dstHeight{0};

  // Fine scale in 16.16 fixed-point (ESP32-C3 has no FPU).
  // X and Y axes use separate scale factors: the aspect ratio of the output (dstWidth/dstHeight)
  // may differ from the source (srcWidth/srcHeight) due to integer rounding of displayHeight.
  // Using a single (X-based) scale for both axes causes the wrong srcRow to be skipped
  // during nearest-neighbor downscaling, potentially losing critical image content.
  int32_t fineScaleFPX{1 << 16};  // X: src -> dst column mapping
  int32_t invScaleFPX{1 << 16};   // X: dst -> src column mapping
  int32_t fineScaleFPY{1 << 16};  // Y: src -> dst row mapping
  int32_t invScaleFPY{1 << 16};   // Y: dst -> src row mapping

  PixelCache cache;
  bool caching{false};

  uint32_t lastYieldMs{0};  // throttle state for yieldDuringDecode()
};

// The image file, which the decoder reads at a position.
struct JpegFile {
  HalFile file;
  uint32_t position{0};
};

size_t jpegRead(void* context, const uint32_t pos, uint8_t* buf, const size_t len) {
  auto& f = *static_cast<JpegFile*>(context);
  if (pos != f.position) {
    if (!f.file.seek(pos)) return 0;
    f.position = pos;
  }
  const int n = f.file.read(buf, len);
  if (n <= 0) return 0;
  f.position += n;
  return static_cast<size_t>(n);
}

using JpegHandle = std::unique_ptr<bb_jpeg, decltype(&bb_jpeg_free)>;

// Opens the file and reads its header; status gets the result, or -1 when the file does not open.
// Null when the file does not open or memory runs out. file must outlive the handle.
JpegHandle openJpeg(const std::string& imagePath, JpegFile& file, int& status) {
  status = -1;
  if (!Storage.openFileForRead("JPG", imagePath, file.file)) return JpegHandle(nullptr, bb_jpeg_free);
  return JpegHandle(bb_jpeg_open(jpegRead, &file, static_cast<int32_t>(file.file.size()), &status), bb_jpeg_free);
}

// The decoder takes ~18 KB for the open image (its read buffer, Huffman tables and pixel buffer).
constexpr size_t JPEG_DECODER_APPROX_SIZE = 20 * 1024;
constexpr size_t MIN_FREE_HEAP_FOR_JPEG = JPEG_DECODER_APPROX_SIZE + 16 * 1024;

// Choose the decoder's built-in scale factor for coarse downscaling.
// Returns the scale denominator (1, 2, 4, or 8) and sets jpegScaleOption.
int chooseJpegScale(float targetScale, int& jpegScaleOption) {
  if (targetScale <= 0.125f) {
    jpegScaleOption = BB_JPEG_SCALE_EIGHTH;
    return 8;
  }
  if (targetScale <= 0.25f) {
    jpegScaleOption = BB_JPEG_SCALE_QUARTER;
    return 4;
  }
  if (targetScale <= 0.5f) {
    jpegScaleOption = BB_JPEG_SCALE_HALF;
    return 2;
  }
  jpegScaleOption = 0;
  return 1;
}

// Fixed-point 16.16 arithmetic avoids software float emulation on ESP32-C3 (no FPU).
constexpr int FP_SHIFT = 16;
constexpr int32_t FP_ONE = 1 << FP_SHIFT;

// test/image_sampling's newDraw mirrors this callback: change both.
int jpegDrawCallback(void* user, const bb_jpeg_block* block) {
  JpegContext* ctx = static_cast<JpegContext*>(user);
  if (!ctx || !ctx->config || !ctx->renderer) return 0;

  ImageToFramebufferDecoder::yieldDuringDecode(ctx->lastYieldMs);

  // 8-bit grayscale, densely packed: stride = block->width, valid columns = block->width_used
  const uint8_t* pixels = block->pixels;
  const int stride = block->width;
  const int validW = block->width_used;
  const int blockH = block->height;

  if (stride <= 0 || blockH <= 0 || validW <= 0) return 1;

  const bool useDithering = ctx->config->useDithering;
  bool caching = ctx->caching;
  const int32_t fineScaleFPX = ctx->fineScaleFPX;
  const int32_t invScaleFPX = ctx->invScaleFPX;
  const int32_t fineScaleFPY = ctx->fineScaleFPY;
  const int32_t invScaleFPY = ctx->invScaleFPY;
  GfxRenderer& renderer = *ctx->renderer;
  const int cfgX = ctx->config->x;
  const int cfgY = ctx->config->y;
  const int blockX = block->x - ctx->cropLeft;
  const int blockY = block->y - ctx->cropTop;

  // Determine destination pixel range covered by this source block
  const int srcYEnd = blockY + blockH;
  const int srcXEnd = blockX + validW;

  int dstYStart = (int)((int64_t)blockY * fineScaleFPY >> FP_SHIFT);
  int dstYEnd = (srcYEnd >= ctx->scaledSrcHeight) ? ctx->dstHeight : (int)((int64_t)srcYEnd * fineScaleFPY >> FP_SHIFT);
  int dstXStart = (int)((int64_t)blockX * fineScaleFPX >> FP_SHIFT);
  int dstXEnd = (srcXEnd >= ctx->scaledSrcWidth) ? ctx->dstWidth : (int)((int64_t)srcXEnd * fineScaleFPX >> FP_SHIFT);

  // Pre-clamp destination ranges to screen bounds (eliminates per-pixel screen checks)
  int clampYMax = ctx->dstHeight;
  if (ctx->screenHeight - cfgY < clampYMax) clampYMax = ctx->screenHeight - cfgY;
  if (dstYStart < -cfgY) dstYStart = -cfgY;
  if (dstYEnd > clampYMax) dstYEnd = clampYMax;

  int clampXMax = ctx->dstWidth;
  if (ctx->screenWidth - cfgX < clampXMax) clampXMax = ctx->screenWidth - cfgX;
  if (dstXStart < -cfgX) dstXStart = -cfgX;
  if (dstXEnd > clampXMax) dstXEnd = clampXMax;

  if (dstYStart >= dstYEnd || dstXStart >= dstXEnd) return 1;

  // The cache streams to disk one MCU-row band at a time. Flushing rows below
  // this block (raster order guarantees they are final) repositions the band;
  // cacheOriginY then maps screen rows to the band-local buffer rows. If a flush
  // write fails, stop caching for the rest of this decode (and let finalize drop
  // the partial file) rather than writing past the band buffer.
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
  const bb_dither_scale scale{fineScaleFPX, invScaleFPX, fineScaleFPY, invScaleFPY};
  const uint8_t quantize = useDithering ? BB_DITHER_QUANTIZE_BAYER : BB_DITHER_QUANTIZE_THIRDS;
  // The levels of up to this many columns of a row at a time.
  constexpr int PIECE = 512;
  uint8_t levels[PIECE / 4];
  for (int dstY = dstYStart; dstY < dstYEnd; dstY++) {
    const int outY = cfgY + dstY;
    // The row in the cache band, or none outside it.
    const int bandRow = outY - cacheOriginY;
    uint8_t* cacheRow = caching && static_cast<unsigned>(bandRow) < static_cast<unsigned>(ctx->cache.bandRows)
                            ? ctx->cache.buffer + static_cast<size_t>(bandRow) * ctx->cache.bytesPerRow
                            : nullptr;
    for (int from = dstXStart; from < dstXEnd; from += PIECE) {
      const int count = std::min(PIECE, dstXEnd - from);
      bb_dither_block_row(&source, &scale, dstY, from, from + count, cfgX, cfgY, quantize, levels, sizeof(levels));
      renderer.drawLevelRow(levels, count, cfgX + from, outY);
      if (cacheRow) {
        bb_dither_put_levels(cacheRow, ctx->cache.bytesPerRow, cfgX + from - ctx->cache.originX, levels, sizeof(levels),
                             count);
      }
    }
  }
  return 1;
}

}  // namespace

bool JpegToFramebufferConverter::getDimensionsStatic(const std::string& imagePath, ImageDimensions& out) {
  size_t freeHeap = ESP.getFreeHeap();
  if (freeHeap < MIN_FREE_HEAP_FOR_JPEG) {
    LOG_ERR("JPG", "Not enough heap for JPEG decoder (%u free, need %u)", freeHeap, MIN_FREE_HEAP_FOR_JPEG);
    return false;
  }

  JpegFile file;
  int status = 0;
  const JpegHandle jpeg = openJpeg(imagePath, file, status);
  if (status != BB_JPEG_SUCCESS) {
    LOG_ERR("JPG", "Failed to open JPEG for dimensions (err=%d): %s", status, imagePath.c_str());
    return false;
  }
  bb_jpeg_info info{};
  bb_jpeg_get_info(jpeg.get(), &info);

  const int width = info.width;
  const int height = info.height;
  if (!validateAndStoreDimensions(width, height, out, "JPEG")) return false;
  LOG_DBG("JPG", "Image dimensions: %dx%d", width, height);

  return true;
}

bool JpegToFramebufferConverter::decodeToFramebuffer(const std::string& imagePath, GfxRenderer& renderer,
                                                     const RenderConfig& config) {
  bool cacheWritten;
  return decodeToFramebuffer(imagePath, renderer, config, cacheWritten);
}

bool JpegToFramebufferConverter::decodeToFramebuffer(const std::string& imagePath, GfxRenderer& renderer,
                                                     const RenderConfig& config, bool& cacheWritten) {
  cacheWritten = false;
  LOG_DBG("JPG", "Decoding JPEG: %s", imagePath.c_str());

  size_t freeHeap = ESP.getFreeHeap();
  if (freeHeap < MIN_FREE_HEAP_FOR_JPEG) {
    LOG_ERR("JPG", "Not enough heap for JPEG decoder (%u free, need %u)", freeHeap, MIN_FREE_HEAP_FOR_JPEG);
    return false;
  }

  JpegContext ctx;
  ctx.renderer = &renderer;
  ctx.config = &config;
  ctx.screenWidth = renderer.getScreenWidth();
  ctx.screenHeight = renderer.getScreenHeight();

  JpegFile file;
  int status = 0;
  const JpegHandle jpeg = openJpeg(imagePath, file, status);
  if (status != BB_JPEG_SUCCESS) {
    LOG_ERR("JPG", "Failed to open JPEG (err=%d): %s", status, imagePath.c_str());
    return false;
  }
  bb_jpeg_info info{};
  bb_jpeg_get_info(jpeg.get(), &info);

  ImageDimensions sourceDimensions;
  if (!validateAndStoreDimensions(info.width, info.height, sourceDimensions, "JPEG")) return false;
  const int srcWidth = sourceDimensions.width;
  const int srcHeight = sourceDimensions.height;
  const float cropX = std::clamp(config.sourceCropX, 0.0f, 0.99f);
  const float cropY = std::clamp(config.sourceCropY, 0.0f, 0.99f);
  const int visibleWidth = srcWidth - 2 * static_cast<int>(srcWidth * cropX / 2);
  const int visibleHeight = srcHeight - 2 * static_cast<int>(srcHeight * cropY / 2);

  const bool isProgressive = info.progressive;
  if (isProgressive) {
    LOG_INF("JPG", "Progressive JPEG detected - decoding DC coefficients only (lower quality)");
  }

  // Calculate overall target scale
  float targetScale;
  int destWidth, destHeight;

  if (config.useExactDimensions && config.maxWidth > 0 && config.maxHeight > 0) {
    destWidth = config.maxWidth;
    destHeight = config.maxHeight;
    targetScale = (float)destWidth / visibleWidth;
  } else {
    float scaleX =
        (config.maxWidth > 0 && visibleWidth > config.maxWidth) ? (float)config.maxWidth / visibleWidth : 1.0f;
    float scaleY =
        (config.maxHeight > 0 && visibleHeight > config.maxHeight) ? (float)config.maxHeight / visibleHeight : 1.0f;
    targetScale = (scaleX < scaleY) ? scaleX : scaleY;
    if (targetScale > 1.0f) targetScale = 1.0f;

    destWidth = (int)(visibleWidth * targetScale);
    destHeight = (int)(visibleHeight * targetScale);
  }

  // Choose the decoder's built-in scaling for coarse downscaling.
  // Progressive JPEGs hold only 1/8-size data (the DC values of the first
  // scan), so ask for 1/8: full size gives 1/8 anyway, and 1/2 or 1/4 leave
  // most of a subsampled image's blocks unfilled.
  int jpegScaleOption;
  int jpegScaleDenom;
  if (isProgressive) {
    jpegScaleOption = BB_JPEG_SCALE_EIGHTH;
    jpegScaleDenom = 8;
  } else {
    jpegScaleDenom = chooseJpegScale(targetScale, jpegScaleOption);
  }

  if (destWidth <= 0 || destHeight <= 0) {
    LOG_ERR("JPG", "Degenerate output dimensions %dx%d for %s, skipping render", destWidth, destHeight,
            imagePath.c_str());
    return false;
  }

  ctx.scaledSrcWidth = (srcWidth + jpegScaleDenom - 1) / jpegScaleDenom;
  ctx.scaledSrcHeight = (srcHeight + jpegScaleDenom - 1) / jpegScaleDenom;
  ctx.cropLeft = static_cast<int>(ctx.scaledSrcWidth * cropX / 2);
  ctx.cropTop = static_cast<int>(ctx.scaledSrcHeight * cropY / 2);
  ctx.scaledSrcWidth -= 2 * ctx.cropLeft;
  ctx.scaledSrcHeight -= 2 * ctx.cropTop;
  ctx.dstWidth = destWidth;
  ctx.dstHeight = destHeight;
  ctx.fineScaleFPX = (int32_t)((int64_t)destWidth * FP_ONE / ctx.scaledSrcWidth);
  ctx.invScaleFPX = (int32_t)((int64_t)ctx.scaledSrcWidth * FP_ONE / destWidth);
  ctx.fineScaleFPY = (int32_t)((int64_t)destHeight * FP_ONE / ctx.scaledSrcHeight);
  ctx.invScaleFPY = (int32_t)((int64_t)ctx.scaledSrcHeight * FP_ONE / destHeight);

  LOG_DBG("JPG", "JPEG %dx%d -> %dx%d (scale %.2f, jpegScale 1/%d, fineScale %.2f)%s", srcWidth, srcHeight, destWidth,
          destHeight, targetScale, jpegScaleDenom, (float)destWidth / ctx.scaledSrcWidth,
          isProgressive ? " [progressive]" : "");

  // Start streaming the pixel cache to disk. The band only needs to hold the
  // tallest single decode block: an MCU cell is at most 16 scaled-source
  // rows tall, which our fine scale maps to this many output rows.
  ctx.caching = !config.cachePath.empty();
  if (ctx.caching) {
    const int maxBlockDstRows = (int)(((int64_t)16 * ctx.fineScaleFPY) >> FP_SHIFT) + 2;
    if (!ctx.cache.begin(config.cachePath, destWidth, destHeight, config.x, config.y, maxBlockDstRows)) {
      LOG_ERR("JPG", "Failed to start cache stream, continuing without caching");
      ctx.caching = false;
    }
  }

  unsigned long decodeStart = millis();
  ctx.lastYieldMs = decodeStart;
  status = bb_jpeg_decode(jpeg.get(), jpegScaleOption, jpegDrawCallback, &ctx);
  unsigned long decodeTime = millis() - decodeStart;

  if (status != BB_JPEG_SUCCESS) {
    LOG_ERR("JPG", "Decode failed (err=%d)", status);
    if (ctx.caching) ctx.cache.abort();
    return false;
  }

  LOG_DBG("JPG", "JPEG decoding complete - render time: %lu ms", decodeTime);

  // Finalize the streamed cache file. Note: a flush failure mid-decode clears
  // ctx.caching (the partial file is dropped), so re-read the flag here.
  if (ctx.caching) {
    cacheWritten = ctx.cache.finalize();
  }

  return true;
}

bool JpegToFramebufferConverter::supportsFormat(const std::string& extension) {
  return FsHelpers::hasJpgExtension(extension);
}
