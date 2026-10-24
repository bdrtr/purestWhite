#include "PngToFramebufferConverter.h"

#include <FsHelpers.h>
#include <GfxRenderer.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <bembeyaz/dither.h>
#include <bembeyaz/png.h>

#include <algorithm>
#include <cstddef>
#include <cstdlib>
#include <memory>

#include "PixelCache.h"

// png.h's layouts, which bembeyaz-ffi's png.rs checks from its side.
static_assert(offsetof(bb_png_info, has_alpha) == 10);
static_assert(offsetof(bb_png_row, y) == 2 * sizeof(void*) &&
              offsetof(bb_png_row, transparent) == 2 * sizeof(void*) + 12 &&
              offsetof(bb_png_row, has_alpha) == 2 * sizeof(void*) + 18);
// dither.h's, which bembeyaz-ffi's dither.rs checks too.
static_assert(offsetof(bb_dither_line, crop) == 3 * sizeof(size_t) &&
              offsetof(bb_dither_line, dst_width) == 3 * sizeof(size_t) + 8);

namespace {

// What the draw callback works with, passed through bb_png_decode.
struct PngContext {
  GfxRenderer* renderer{nullptr};
  const RenderConfig* config{nullptr};
  int screenWidth{0};
  int screenHeight{0};

  // Scaling state
  float scale{1.f};
  int srcWidth{0};
  int srcHeight{0};
  int cropLeft{0};
  int cropTop{0};
  int visibleWidth{0};
  int visibleHeight{0};
  int dstWidth{0};
  int dstHeight{0};
  int lastDstY{-1};  // Track last rendered destination Y to avoid duplicates
  PixelCache cache;
  bool caching{false};

  uint8_t* grayLineBuffer{nullptr};
  uint8_t* alphaLineBuffer{nullptr};
  uint32_t lastYieldMs{0};  // throttle state for yieldDuringDecode()
};

// The image file, which the decoder reads at a position.
struct PngFile {
  HalFile file;
  uint32_t position{0};
};

size_t pngRead(void* context, const uint32_t pos, uint8_t* buf, const size_t len) {
  auto& f = *static_cast<PngFile*>(context);
  if (pos != f.position) {
    if (!f.file.seek(pos)) return 0;
    f.position = pos;
  }
  const int n = f.file.read(buf, len);
  if (n <= 0) return 0;
  f.position += n;
  return static_cast<size_t>(n);
}

using PngHandle = std::unique_ptr<bb_png, decltype(&bb_png_free)>;

// Opens the file and reads its header; status gets the result, or -1 when the file does not open.
// Null when the file does not open or memory runs out. file must outlive the handle.
PngHandle openPng(const std::string& imagePath, PngFile& file, int& status) {
  status = -1;
  if (!Storage.openFileForRead("PNG", imagePath, file.file)) return PngHandle(nullptr, bb_png_free);
  return PngHandle(bb_png_open(pngRead, &file, static_cast<uint32_t>(file.file.size()), &status), bb_png_free);
}

// The decoder takes ~3 KB for the open image and, while decoding, ~37 KB of inflate state and
// window plus two rows (up to 16 KB at the row limit below), in separate blocks. Of these only the
// image and rows of up to 4 KB land in internal RAM, which the gate below checks; its size is kept
// so the same images decode.
constexpr size_t PNG_DECODER_APPROX_SIZE = 44 * 1024;
constexpr size_t MIN_FREE_HEAP_FOR_PNG = PNG_DECODER_APPROX_SIZE + 16 * 1024;  // decoder + 16 KB headroom

int bytesPerPixelFromType(int pixelType) {
  switch (pixelType) {
    case BB_PNG_PIXEL_TRUECOLOR:
      return 3;
    case BB_PNG_PIXEL_GRAY_ALPHA:
      return 2;
    case BB_PNG_PIXEL_TRUECOLOR_ALPHA:
      return 4;
    case BB_PNG_PIXEL_GRAYSCALE:
    case BB_PNG_PIXEL_INDEXED:
    default:
      return 1;
  }
}

int packedRowBytes(int srcWidth, int bitsPerSample) { return (srcWidth * bitsPerSample + 7) / 8; }

// The row limit PNGdec's buffer set, kept so the same images decode: two rows with their filter
// bytes, plus 32 bytes of slack, had to fit its 16416 bytes.
constexpr int PNG_ROW_BUDGET = 16416;

int requiredPngInternalBufferBytes(int srcWidth, int pixelType, int bitsPerSample) {
  int pitch = srcWidth * bytesPerPixelFromType(pixelType);
  if ((pixelType == BB_PNG_PIXEL_GRAYSCALE || pixelType == BB_PNG_PIXEL_INDEXED) && bitsPerSample < 8) {
    pitch = packedRowBytes(srcWidth, bitsPerSample);
  }
  return ((pitch + 1) * 2) + 32;
}

bool isSupportedBitDepth(int pixelType, int bitsPerSample) {
  if (bitsPerSample == 8) return true;
  if (bitsPerSample != 1 && bitsPerSample != 2 && bitsPerSample != 4) return false;
  return pixelType == BB_PNG_PIXEL_GRAYSCALE || pixelType == BB_PNG_PIXEL_INDEXED;
}

// test/image_sampling's newPngDraw mirrors this callback's drawing: change both.
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
  bb_png_row_gray(row, ctx->grayLineBuffer, static_cast<size_t>(srcWidth), ctx->alphaLineBuffer);

  // Columns step through the source line Bresenham-style (dst column c reads cropLeft + c * visibleWidth /
  // dstWidth); those past the screen's right edge are left out.
  const int outXBase = ctx->config->x;
  const int columns = std::min(ctx->dstWidth, ctx->screenWidth - outXBase);
  const bool transparent = ctx->alphaLineBuffer != nullptr;
  const bb_dither_line line{ctx->grayLineBuffer, ctx->alphaLineBuffer, static_cast<size_t>(srcWidth),
                            ctx->cropLeft,       ctx->visibleWidth,    ctx->dstWidth};
  const uint8_t quantize = ctx->config->useDithering ? BB_DITHER_QUANTIZE_BAYER : BB_DITHER_QUANTIZE_QUARTERS;
  // The levels (and, with alpha, which pixels show) of up to this many columns of a row at a time.
  constexpr int PIECE = 512;
  uint8_t levels[PIECE / 4]{};
  uint8_t mask[PIECE / 8]{};

  for (int dstY = firstDstY; dstY < endDstY; dstY++) {
    ctx->lastDstY = dstY;
    int outY = ctx->config->y + dstY;
    if (outY >= ctx->screenHeight) continue;

    // The cache streams to disk one row at a time. Flushing rows below this one
    // (the decoder delivers scanlines top to bottom) repositions the single-row band.
    // A flush failure stops caching for the rest of the decode so we never write
    // past the band buffer; finalize() then drops the partial file.
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
      ctx->renderer->drawLevelRow(levels, count, outXBase + from, outY, transparent ? mask : nullptr);
      // Images with alpha are never cached, so the mask has no part here.
      if (cacheRow) {
        bb_dither_put_levels(cacheRow, ctx->cache.bytesPerRow, outXBase + from - ctx->cache.originX, levels,
                             sizeof(levels), count);
      }
    }
  }

  return 1;
}

}  // namespace

bool PngToFramebufferConverter::getDimensionsStatic(const std::string& imagePath, ImageDimensions& out) {
  size_t freeHeap = ESP.getFreeHeap();
  if (freeHeap < MIN_FREE_HEAP_FOR_PNG) {
    LOG_ERR("PNG", "Not enough heap for PNG decoder (%u free, need %u)", freeHeap, MIN_FREE_HEAP_FOR_PNG);
    return false;
  }

  PngFile file;
  int rc = BB_PNG_SUCCESS;
  const PngHandle png = openPng(imagePath, file, rc);
  if (!png || rc != BB_PNG_SUCCESS) {
    LOG_ERR("PNG", "Failed to open PNG for dimensions: %d", rc);
    return false;
  }

  bb_png_info info;
  bb_png_get_info(png.get(), &info);
  return validateAndStoreDimensions(info.width, info.height, out, "PNG");
}

bool PngToFramebufferConverter::decodeToFramebuffer(const std::string& imagePath, GfxRenderer& renderer,
                                                    const RenderConfig& config) {
  LOG_DBG("PNG", "Decoding PNG: %s", imagePath.c_str());

  size_t freeHeap = ESP.getFreeHeap();
  if (freeHeap < MIN_FREE_HEAP_FOR_PNG) {
    LOG_ERR("PNG", "Not enough heap for PNG decoder (%u free, need %u)", freeHeap, MIN_FREE_HEAP_FOR_PNG);
    return false;
  }

  PngContext ctx;
  ctx.renderer = &renderer;
  ctx.config = &config;
  ctx.screenWidth = renderer.getScreenWidth();
  ctx.screenHeight = renderer.getScreenHeight();

  PngFile file;
  int rc = BB_PNG_SUCCESS;
  const PngHandle png = openPng(imagePath, file, rc);
  if (!png || rc != BB_PNG_SUCCESS) {
    LOG_ERR("PNG", "Failed to open PNG: %d", rc);
    return false;
  }
  bb_png_info info;
  bb_png_get_info(png.get(), &info);

  ImageDimensions sourceDimensions;
  if (!validateAndStoreDimensions(info.width, info.height, sourceDimensions, "PNG")) return false;

  // Calculate output dimensions
  ctx.srcWidth = sourceDimensions.width;
  ctx.srcHeight = sourceDimensions.height;
  ctx.cropLeft = static_cast<int>(ctx.srcWidth * std::clamp(config.sourceCropX, 0.0f, 0.99f) / 2.0f);
  ctx.cropTop = static_cast<int>(ctx.srcHeight * std::clamp(config.sourceCropY, 0.0f, 0.99f) / 2.0f);
  ctx.visibleWidth = ctx.srcWidth - 2 * ctx.cropLeft;
  ctx.visibleHeight = ctx.srcHeight - 2 * ctx.cropTop;
  if (ctx.visibleWidth <= 0 || ctx.visibleHeight <= 0) {
    LOG_ERR("PNG", "PNG crop leaves no visible pixels");
    return false;
  }

  if (config.useExactDimensions && config.maxWidth > 0 && config.maxHeight > 0) {
    // Use exact dimensions as specified (avoids rounding mismatches with pre-calculated sizes)
    ctx.dstWidth = config.maxWidth;
    ctx.dstHeight = config.maxHeight;
    ctx.scale = (float)ctx.dstWidth / ctx.visibleWidth;
  } else {
    // Calculate scale factor to fit within maxWidth/maxHeight
    float scaleX = (float)config.maxWidth / ctx.visibleWidth;
    float scaleY = (float)config.maxHeight / ctx.visibleHeight;
    ctx.scale = (scaleX < scaleY) ? scaleX : scaleY;
    if (ctx.scale > 1.0f) ctx.scale = 1.0f;  // Don't upscale

    ctx.dstWidth = (int)(ctx.visibleWidth * ctx.scale);
    ctx.dstHeight = (int)(ctx.visibleHeight * ctx.scale);
  }
  ctx.lastDstY = -1;  // Reset row tracking

  const int pixelType = info.pixel_type;
  const int bitsPerSample = info.bpp;
  LOG_DBG("PNG", "PNG %dx%d (visible %dx%d) -> %dx%d (scale %.2f), type: %d, bpp: %d", ctx.srcWidth, ctx.srcHeight,
          ctx.visibleWidth, ctx.visibleHeight, ctx.dstWidth, ctx.dstHeight, ctx.scale, pixelType, bitsPerSample);

  const int requiredInternal = requiredPngInternalBufferBytes(ctx.srcWidth, pixelType, bitsPerSample);
  if (requiredInternal > PNG_ROW_BUDGET) {
    LOG_ERR("PNG", "PNG too wide: two rows need %d bytes for width=%d type=%d bpp=%d, limit %d", requiredInternal,
            ctx.srcWidth, pixelType, bitsPerSample, PNG_ROW_BUDGET);
    return false;
  }

  if (!isSupportedBitDepth(pixelType, bitsPerSample)) {
    warnUnsupportedFeature(
        "bit depth (" + std::to_string(bitsPerSample) + "bpp) for pixel type " + std::to_string(pixelType), imagePath);
    return false;
  }

  // The converter expands each source row to 8-bit grayscale before dithering,
  // so this scratch buffer is sized by source pixels even when the decoder hands
  // over a packed 1/2/4-bit row.
  constexpr size_t MAX_GRAY_LINE_BUFFER_BYTES = PNG_ROW_BUDGET / 2;
  const size_t grayBufSize = static_cast<size_t>(ctx.srcWidth);
  if (grayBufSize > MAX_GRAY_LINE_BUFFER_BYTES) {
    LOG_ERR("PNG", "Expanded gray row too wide: need %u bytes for width=%d, max=%u", static_cast<unsigned>(grayBufSize),
            ctx.srcWidth, static_cast<unsigned>(MAX_GRAY_LINE_BUFFER_BYTES));
    return false;
  }

  // tRNS chunks are parsed while decoding, so info.has_alpha is not reliable
  // yet for color-key and indexed transparency. Reserve the alpha line whenever
  // the caller requests preservation; opaque pixels simply store alpha 255.
  const bool retainAlpha = config.preserveAlpha;
  const size_t lineBufferBytes = grayBufSize * (retainAlpha ? 2u : 1u);
  auto lineBuffers = makeUniqueNoThrow<uint8_t[]>(lineBufferBytes);
  if (!lineBuffers) {
    LOG_ERR("PNG", "Failed to allocate PNG line buffers (%u bytes)", static_cast<unsigned>(lineBufferBytes));
    return false;
  }
  ctx.grayLineBuffer = lineBuffers.get();
  ctx.alphaLineBuffer = retainAlpha ? ctx.grayLineBuffer + grayBufSize : nullptr;

  // Stream the pixel cache to disk. The decoder delivers source scanlines top to
  // bottom and we emit at most one (downscaled) output row per callback, so the
  // band only needs a single row. Streaming keeps the working set tiny, so
  // unlike the old full-image buffer it neither competes with the decoder
  // nor forces larger images to skip caching - which previously meant a full
  // re-decode on every one of an image page's ~14 render passes.
  ctx.caching = !config.preserveAlpha && !config.cachePath.empty();
  if (ctx.caching) {
    if (!ctx.cache.begin(config.cachePath, ctx.dstWidth, ctx.dstHeight, config.x, config.y, 1)) {
      LOG_ERR("PNG", "Failed to start cache stream, continuing without caching");
      ctx.caching = false;
    }
  }

  unsigned long decodeStart = millis();
  ctx.lastYieldMs = decodeStart;
  rc = bb_png_decode(png.get(), pngDrawCallback, &ctx);
  unsigned long decodeTime = millis() - decodeStart;

  ctx.grayLineBuffer = nullptr;
  ctx.alphaLineBuffer = nullptr;

  if (rc != BB_PNG_SUCCESS) {
    LOG_ERR("PNG", "Decode failed: %d", rc);
    if (ctx.caching) ctx.cache.abort();
    return false;
  }

  LOG_DBG("PNG", "PNG decoding complete - render time: %lu ms", decodeTime);

  // Finalize the streamed cache (caching may have been cleared on a flush error).
  if (ctx.caching) {
    ctx.cache.finalize();
  }

  return true;
}

bool PngToFramebufferConverter::supportsFormat(const std::string& extension) {
  return FsHelpers::hasPngExtension(extension);
}
