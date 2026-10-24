#include "GfxRenderer.h"

#include <BidiUtils.h>
#include <BoardConfig.h>
#include <BuildScratch.h>
#include <FontDecompressor.h>
#include <HalGPIO.h>
#include <Logging.h>
#include <MemoryManager.h>
#include <SdCardFont.h>
#include <TtfEpdFont.h>
#include <Utf8.h>
#include <bembeyaz/gfx.h>

#include <algorithm>

#include "../Memory/Memory.h"
#include "FontCacheManager.h"
#include "GlyphBitmap.h"

namespace {
// Synthesized small caps: lowercase letters draw as capitals at roughly x-height.
constexpr int SMALL_CAPS_SCALE_NUM = 3;
constexpr int SMALL_CAPS_SCALE_DEN = 4;
constexpr int32_t scaleSmallCapAdvance(const int32_t advanceFP) {
  return (advanceFP * SMALL_CAPS_SCALE_NUM + SMALL_CAPS_SCALE_DEN / 2) / SMALL_CAPS_SCALE_DEN;
}
}  // namespace

namespace {
constexpr int trackingBetween(const uint32_t leftCp, const uint32_t rightCp, const int8_t tracking) {
  const auto isSpace = [](const uint32_t cp) { return cp == ' ' || cp == 0xA0 || cp == 0x3000; };
  return leftCp == 0 || isSpace(leftCp) || isSpace(rightCp) ? 0 : tracking;
}

/**
 * Resolves the requested style to the best available style in the given SD card font.
 * Falls back gracefully when the font lacks the requested variant.
 */
uint8_t resolveSdCardStyle(const SdCardFont& font, const EpdFontFamily::Style style) {
  return font.resolveStyle(static_cast<uint8_t>(style));
}

uint16_t getSdCardSpaceAdvance(SdCardFont& font, const EpdFontFamily::Style style) {
  const uint8_t resolvedStyle = resolveSdCardStyle(font, style);
  const uint16_t advance = font.getAdvance(' ', resolvedStyle);
  if (advance != 0) return advance;

  // Zero means uncached (full table, style not prewarmed, or failed
  // preparation): read the glyph, as the per-codepoint slow path does.
  const EpdFont* epdFont = font.getEpdFont(resolvedStyle);
  const EpdGlyph* glyph = epdFont ? epdFont->getGlyph(' ') : nullptr;
  return glyph ? glyph->advanceX : 0;
}
}  // namespace

namespace {
const char* resolveVisualText(const char* text, std::string& visualBuffer, BidiUtils::BidiBaseDir baseDir);

// Appends the shaped visual form of every RTL token in `text` to `shapedOut`.
// getTextAdvanceX() measures the bidi-reordered, Arabic-shaped codepoint stream,
// so the SD advance table must be warmed with the presentation forms as well as
// the logical codepoints — otherwise every RTL word measurement misses the fast
// path and falls through to onGlyphMiss(), which opens the .cpfont and reads
// glyph metadata + bitmap into the 8-slot overflow ring, once per glyph.
// Tokens without RTL lead bytes (0xD6-0xDB) are skipped with a byte scan, so
// pure-LTR text pays almost nothing.
void appendShapedRtlTokens(const char* text, std::string& shapedOut) {
  const auto isBreak = [](const char c) { return c == ' ' || c == '\n' || c == '\r' || c == '\t'; };
  std::string token;
  std::string visual;
  const char* p = text;
  while (*p) {
    while (*p && isBreak(*p)) ++p;
    const char* start = p;
    bool hasRtlBytes = false;
    while (*p && !isBreak(*p)) {
      const auto b = static_cast<unsigned char>(*p);
      hasRtlBytes = hasRtlBytes || (b >= 0xD6 && b <= 0xDB);
      ++p;
    }
    if (!hasRtlBytes) continue;
    token.assign(start, p - start);
    if (BidiUtils::applyBidiVisual(token.c_str(), visual, static_cast<int>(BidiUtils::BidiBaseDir::AUTO))) {
      shapedOut += visual;
    }
  }
}
}  // namespace

const uint8_t* GfxRenderer::getGlyphBitmap(const EpdFontData* fontData, const EpdGlyph* glyph) const {
  // Vector (TTF) fonts: the glyph bitmap lives in the font's own cache, keyed by
  // the EpdGlyph the miss handler returned. Checked first so it never reaches the
  // SdCardFont overflow cast below. nullptr = zero-width glyph (e.g. space).
  if (fontData->vectorBitmapHandler != nullptr) {
    return fontData->vectorBitmapHandler(fontData->glyphMissCtx, glyph);
  }
  if (fontData->groups != nullptr) {
    auto* fd = fontCacheManager_ ? fontCacheManager_->getDecompressor() : nullptr;
    if (!fd) {
      LOG_ERR("GFX", "Compressed font but no FontDecompressor set");
      return nullptr;
    }
    uint32_t glyphIndex = static_cast<uint32_t>(glyph - fontData->glyph);
    // For page-buffer hits the pointer is stable for the page lifetime.
    // For hot-group hits it is valid only until the next getBitmap() call — callers
    // must consume it (draw the glyph) before requesting another bitmap.
    return fd->getBitmap(fontData, glyph, glyphIndex);
  }
  // For SD card fonts, check if the glyph was loaded on demand into the overflow
  // buffer.  getOverflowBitmap() returns:
  //   - bitmap pointer for overflow glyphs with bitmap data
  //   - nullptr for overflow glyphs without bitmap data (e.g. space: width=0, height=0)
  //   - nullptr for non-overflow glyphs (normal prewarmed path)
  // We distinguish overflow-with-no-bitmap from non-overflow by checking isOverflowGlyph().
  if (fontData->glyphMissCtx) {
    auto* sdFont = SdCardFont::fromMissCtx(fontData->glyphMissCtx);
    if (sdFont->isOverflowGlyph(glyph)) {
      return sdFont->getOverflowBitmap(glyph);  // may be nullptr for zero-width glyphs
    }
  }
  return &fontData->bitmap[glyph->dataOffset];
}

void GfxRenderer::ensureSdCardFontReady(int fontId, const char* utf8Text, uint8_t styleMask) const {
  auto it = sdCardFonts_.find(fontId);
  if (it != sdCardFonts_.end()) {
    std::string shaped;
    appendShapedRtlTokens(utf8Text, shaped);
    int missed = it->second->buildAdvanceTable(utf8Text, styleMask, shaped.empty() ? nullptr : shaped.c_str());
    if (missed > 0) {
      LOG_DBG("GFX", "ensureSdCardFontReady: %d glyph(s) not found", missed);
    }
    return;
  }
  // TTF (vector) fonts need nothing here: getGlyph faults glyphs in on demand
  // (glyphMissHandler), and the page's set is batch-warmed by the render scan's
  // prewarmCache(). Pre-faulting a whole chapter's text during layout would just
  // thrash the page-bounded cache, so it is intentionally omitted.
}

void GfxRenderer::ensureSdCardFontReady(const int fontId, const char* const* segments, const size_t* segmentLens,
                                        const size_t segmentCount, const bool includeSpace, const bool includeHyphen,
                                        const uint8_t styleMask, const bool includeSmallCaps) const {
  auto it = sdCardFonts_.find(fontId);
  if (it != sdCardFonts_.end()) {
    // Augment the persistent advance-only table for layout measurement.
    // The table survives across paragraphs/sections (capped per font), so
    // repeated indexing of the same SD font amortizes glyph-metric SD reads.
    std::string shaped;
    for (size_t seg = 0; seg < segmentCount; seg++) {
      const char* p = segments[seg];
      const char* const end = p + segmentLens[seg];
      while (p < end) {
        appendShapedRtlTokens(p, shaped);
        p += strlen(p) + 1;
      }
    }
    int missed =
        it->second->buildAdvanceTablePacked(segments, segmentLens, segmentCount, includeSpace, includeHyphen, styleMask,
                                            shaped.empty() ? nullptr : shaped.c_str(), includeSmallCaps);
    if (missed > 0) {
      LOG_DBG("GFX", "ensureSdCardFontReady: %d glyph(s) not found", missed);
    }
    return;
  }
  // TTF (vector) fonts: nothing to do — glyphs fault in on demand at getGlyph
  // and the page is batch-warmed by the render scan (see the other overload).
}

void GfxRenderer::begin() {
  frameBuffer = display.getFrameBuffer();
  if (!frameBuffer) {
    LOG_ERR("GFX", "!! No framebuffer");
    assert(false);
  }
  panelWidth = display.getDisplayWidth();
  panelHeight = display.getDisplayHeight();
  panelWidthBytes = display.getDisplayWidthBytes();
  frameBufferSize = display.getBufferSize();
  bwBufferChunks.assign((frameBufferSize + BW_BUFFER_CHUNK_SIZE - 1) / BW_BUFFER_CHUNK_SIZE, nullptr);

  // Evictable-cache sinks for the SDK memory manager: layout's OOM choke
  // points (the paragraph word store, TextBlock) call ensureFree() to flush these and retry
  // before failing a section build. Both caches rebuild transparently from SD
  // or flash on the next glyph access. The SD-font persistent advance table is
  // deliberately NOT a sink: evicting it mid-paragraph would make later
  // measurements in the same layout pass return 0-width advances.
  auto& memoryManager = freeink::MemoryManager::instance();
  memoryManager.registerSink({"gfx.renderGlyphCache", 50, [this](size_t) -> size_t {
                                if (!fontCacheManager_ || fontCacheManager_->isScanning()) return 0;
                                const size_t before = freeink::MemoryManager::instance().freeBytes();
                                fontCacheManager_->clearCache();
                                const size_t after = freeink::MemoryManager::instance().freeBytes();
                                return after > before ? after - before : 0;
                              }});
  memoryManager.registerSink({"gfx.sdFontMini", 60, [this](size_t) -> size_t {
                                const size_t before = freeink::MemoryManager::instance().freeBytes();
                                for (auto& entry : sdCardFonts_) {
                                  if (entry.second) entry.second->clearCache();
                                }
                                const size_t after = freeink::MemoryManager::instance().freeBytes();
                                return after > before ? after - before : 0;
                              }});
#if CROSSPOINT_VECTOR_FONTS
  // TTF glyph arenas are safe to shed mid-layout, unlike the SD-font advance
  // table: metrics re-fault through FreeType with identical values, so layout
  // cannot silently corrupt — the cost is re-rasterizing on the next draw.
  memoryManager.registerSink({"gfx.ttfGlyphArenas", 70, [this](size_t) -> size_t {
                                const size_t before = freeink::MemoryManager::instance().freeBytes();
                                for (auto& entry : ttfFonts_) {
                                  if (entry.second) entry.second->releaseResidentCaches();
                                }
                                const size_t after = freeink::MemoryManager::instance().freeBytes();
                                return after > before ? after - before : 0;
                              }});
#endif
}

void GfxRenderer::releaseFrameBufferForBuild() {
  // Lend the framebuffer's bytes IN PLACE: the allocation is never freed, so
  // it cannot move and repeated loans cannot fragment the heap (the previous
  // free+realloc model measurably decayed the max contiguous block over a
  // session). The bytes are deposited in the build-scratch registry so
  // memory-hungry build phases (e.g. InflateStream's decoder state + window)
  // can claim them instead of allocating.
  uint32_t size = 0;
  uint8_t* scratch = display.lendFrameBufferStorage(&size);
  frameBuffer = nullptr;
  frameBufferLoans++;
  if (scratch) {
    buildscratch::lend(scratch, size);
  }
}

bool GfxRenderer::restoreFrameBufferAfterBuild() {
  buildscratch::reclaim();
  display.returnFrameBufferStorage();  // cannot fail: the allocation was never freed
  frameBuffer = display.getFrameBuffer();
  return frameBuffer != nullptr;
}

GfxRenderer::FrameBufferLoan::FrameBufferLoan(GfxRenderer& renderer) : renderer_(renderer) {
  // Nesting guard: if the framebuffer is already lent out (an outer loan),
  // stay inert so this end() cannot return storage the outer loan still owns.
  if (!renderer_.hasFrameBuffer()) return;
  renderer_.releaseFrameBufferForBuild();
  active_ = true;
}

void GfxRenderer::FrameBufferLoan::end() {
  if (!active_) return;
  active_ = false;
  if (!renderer_.restoreFrameBufferAfterBuild()) {
    // Only reachable if the framebuffer never existed, which begin() already
    // asserts against; kept as a backstop since running blind helps nobody.
    LOG_ERR("GFX", "Framebuffer restore failed - restarting");
    ESP.restart();
  }
}

bool GfxRenderer::isFontCacheScanning() const { return fontCacheManager_ && fontCacheManager_->isScanning(); }

void GfxRenderer::insertFont(const int fontId, EpdFontFamily font) {
  auto result = fontMap.insert({fontId, font});
  if (!result.second) {
    LOG_ERR("GFX", "Font ID %d already registered, ignoring duplicate", fontId);
  }
}

int GfxRenderer::resolveTextFontId(const int fontId, const char* text, const EpdFontFamily::Style style) const {
  if (fallbackFontMap_.empty() || text == nullptr || *text == '\0') {
    return fontId;
  }
  const auto fbIt = fallbackFontMap_.find(fontId);
  if (fbIt == fallbackFontMap_.end()) {
    return fontId;  // no fallback registered for this font
  }
  const int fallbackFontId = fbIt->second;
  const auto fontIt = fontMap.find(fontId);
  const auto fallbackIt = fontMap.find(fallbackFontId);
  if (fontIt == fontMap.end() || fallbackIt == fontMap.end()) {
    return fontId;  // unknown primary or fallback not loaded — let the caller handle it
  }
  const EpdFontFamily& primary = fontIt->second;
  const EpdFontFamily& fallback = fallbackIt->second;
  const char* cursor = text;
  uint32_t cp;
  while ((cp = utf8NextCodepoint(reinterpret_cast<const uint8_t**>(&cursor)))) {
    // Redirect whenever the primary font cannot draw a codepoint the fallback
    // can — otherwise the string renders with holes. Not just CJK: Greek,
    // Cyrillic, or any script the built-in UI fonts lack qualifies. ASCII is
    // skipped outright (every registered font covers it), keeping Latin
    // strings on the fast path, and a partial-coverage fallback is not worth
    // dragging the whole string into for glyphs it would also miss.
    if (cp >= 0x80 && !primary.hasCodepoint(cp, style) && fallback.hasCodepoint(cp, style)) {
      return fallbackFontId;
    }
  }
  return fontId;
}

void GfxRenderer::prewarmFallbackText(const int fontId, const TextGetter getter, const void* ctx,
                                      const uint32_t textCount, const EpdFontFamily::Style style) const {
  if (getter == nullptr || textCount == 0) {
    return;
  }
  // Resolve the fallback id from the first string that actually redirects; a
  // screen with no non-Latin strings resolves nothing and this is a no-op.
  int fallbackFontId = fontId;
  for (uint32_t i = 0; i < textCount && fallbackFontId == fontId; i++) {
    const char* text = getter(ctx, i);
    if (text == nullptr || *text == '\0') continue;
    fallbackFontId = resolveTextFontId(fontId, text, style);
  }
  if (fallbackFontId == fontId) {
    return;
  }
  const auto sdIt = sdCardFonts_.find(fallbackFontId);
  if (sdIt == sdCardFonts_.end()) {
    return;
  }
  const uint8_t styleMask = static_cast<uint8_t>(1u << (static_cast<uint8_t>(style) & 0x03));
  // Append one virtual index for U+2026: truncation measures every long row
  // as "label…", so an ellipsis missing from the batch forces a union rebuild
  // on the first repaint.
  struct WrapCtx {
    TextGetter getter;
    const void* ctx;
    uint32_t count;
  } wrap{getter, ctx, textCount};
  const auto withEllipsis = [](const void* wc, uint32_t i) -> const char* {
    const auto* w = static_cast<const WrapCtx*>(wc);
    return i < w->count ? w->getter(w->ctx, i) : "\xe2\x80\xa6";
  };
  // loadKernLig=false: see ensureSdGlyphsResident below.
  sdIt->second->prewarm(withEllipsis, &wrap, textCount + 1, styleMask, /*metadataOnly=*/false,
                        /*loadKernLig=*/false);
}

void GfxRenderer::prewarmFallbackText(const int fontId, const char* text, const EpdFontFamily::Style style) const {
  if (text == nullptr || *text == '\0') {
    return;
  }
  const int resolvedFontId = resolveTextFontId(fontId, text, style);
  if (resolvedFontId != fontId) {
    ensureSdGlyphsResident(resolvedFontId, text, style, false);
  }
}

void GfxRenderer::ensureSdGlyphsResident(const int fontId, const char* text, const EpdFontFamily::Style style,
                                         const bool metadataOnly) const {
  const auto sdIt = sdCardFonts_.find(fontId);
  if (sdIt == sdCardFonts_.end()) {
    return;
  }
  // SUP/SUB bits don't select a distinct .cpfont style bitstream — mask to the
  // base style. resolveStyleMask() inside prewarm folds absent styles.
  // loadKernLig=false: redirected fallback strings (non-Latin titles,
  // filenames) have no useful kern pairs, and the ~3KB class-table load plus
  // per-rebuild mini-matrix build cost heap and SD time exactly where these
  // strings live (heap-tight UI screens). The reader's PrewarmScope path keeps
  // kern; a kern-wanting request that subset-hits a kern-free mini tops the
  // matrix up in prewarmStyle without re-reading glyphs.
  const uint8_t styleMask = static_cast<uint8_t>(1u << (static_cast<uint8_t>(style) & 0x03));
  sdIt->second->prewarm(text, styleMask, metadataOnly, /*loadKernLig=*/false);
}

// Translate logical (x,y) coordinates to physical panel coordinates based on current orientation
// This should always be inlined for better performance
static inline void rotateCoordinates(const GfxRenderer::Orientation orientation, const int x, const int y, int* phyX,
                                     int* phyY, const uint16_t panelWidth, const uint16_t panelHeight) {
  switch (orientation) {
    case GfxRenderer::Portrait: {
      // Logical portrait (480x800) → panel (800x480)
      // Rotation: 90 degrees clockwise
      *phyX = y;
      *phyY = panelHeight - 1 - x;
      break;
    }
    case GfxRenderer::LandscapeClockwise: {
      // Logical landscape (800x480) rotated 180 degrees (swap top/bottom and left/right)
      *phyX = panelWidth - 1 - x;
      *phyY = panelHeight - 1 - y;
      break;
    }
    case GfxRenderer::PortraitInverted: {
      // Logical portrait (480x800) → panel (800x480)
      // Rotation: 90 degrees counter-clockwise
      *phyX = panelWidth - 1 - y;
      *phyY = x;
      break;
    }
    case GfxRenderer::LandscapeCounterClockwise: {
      // Logical landscape (800x480) aligned with panel orientation
      *phyX = x;
      *phyY = y;
      break;
    }
  }
}

enum class TextRotation { None, Rotated90CW };

namespace {
bb_gfx_canvas canvasOf(const GfxRenderer& renderer);
void logOffPanel(uint32_t pixels);

// Bytes in a packed glyph bitmap: rows back to back, 1 or 2 bits per pixel.
size_t glyphBytes(const int width, const int height, const bool twoBit) {
  const size_t bits = static_cast<size_t>(width) * static_cast<size_t>(height) * (twoBit ? 2 : 1);
  return (bits + 7) / 8;
}
}  // namespace

// Shared glyph rendering logic for normal and rotated text.
// Coordinate mapping and cursor advance direction are selected at compile time via the template parameter.
// Render a glyph scaled by scaleNum/scaleDen: 1/2 for SUP/SUB, 3/4 for small caps.
//
// Each destination pixel samples the source block it covers. At 1/2, any ink in
// the block draws, preserving thin strokes that nearest-neighbor sampling can skip;
// milder reductions need majority coverage so stems don't thicken.
//
// drawText() scales the advance width by the same factor so layout reserves exactly
// the right horizontal space for the scaled glyph.
static void renderCharScaled(const GfxRenderer& renderer, GfxRenderer::RenderMode renderMode,
                             const EpdFontFamily& fontFamily, const uint32_t cp, int cursorX, int cursorY,
                             const bool pixelState, const EpdFontFamily::Style style, const int scaleNum,
                             const int scaleDen) {
  if (renderer.grayPlanesAreAbsolute()) renderMode = GfxRenderer::BW;
  EpdGlyph solidFallback;
  const EpdGlyph* glyph = fontFamily.getGlyphMetrics(cp, solidFallback, style);
  if (!glyph) return;

  const EpdFontData* fontData = fontFamily.getData(style);
  if (glyph == &solidFallback) {
    // The box is as big as bb_gfx_reduced_glyph's drawing (rounded up).
    renderer.fillRect(cursorX + glyph->left * scaleNum / scaleDen, cursorY - glyph->top * scaleNum / scaleDen,
                      (glyph->width * scaleNum + scaleDen - 1) / scaleDen,
                      (glyph->height * scaleNum + scaleDen - 1) / scaleDen, pixelState);
    return;
  }
  const uint8_t* bitmap = renderer.getGlyphBitmap(fontData, glyph);
  if (!bitmap) return;

  const int srcW = glyph->width;
  const int srcH = glyph->height;
  // Scale the glyph bearing by the same factor so the scaled glyph sits at the correct
  // pixel offset from the (already-shifted) cursor position.
  const int baseX = cursorX + glyph->left * scaleNum / scaleDen;
  const int baseY = cursorY - glyph->top * scaleNum / scaleDen;
  const auto c = canvasOf(renderer);
  logOffPanel(bb_gfx_reduced_glyph(&c, bitmap, glyphBytes(srcW, srcH, fontData->is2Bit), srcW, srcH, fontData->is2Bit,
                                   scaleNum, scaleDen, baseX, baseY, pixelState));
}

template <TextRotation rotation = TextRotation::None>
static void renderCharImpl(const GfxRenderer& renderer, GfxRenderer::RenderMode renderMode,
                           const EpdFontFamily& fontFamily, const uint32_t cp, int cursorX, int cursorY,
                           const bool pixelState, const EpdFontFamily::Style style) {
  if (renderer.grayPlanesAreAbsolute()) renderMode = GfxRenderer::BW;
  EpdGlyph solidFallback;
  const EpdGlyph* glyph = fontFamily.getGlyphMetrics(cp, solidFallback, style);
  if (!glyph) {
    LOG_ERR("GFX", "No glyph for codepoint %d", cp);
    return;
  }

  const EpdFontData* fontData = fontFamily.getData(style);
  const bool is2Bit = fontData->is2Bit;
  const uint8_t width = glyph->width;
  const uint8_t height = glyph->height;
  const int left = glyph->left;
  const int top = glyph->top;

  // Tiled-grayscale band culling: if this glyph's physical y-extent is entirely
  // outside the active strip, skip it before the expensive bitmap decode. This
  // is what makes per-band re-rendering cheap. No-op outside strip mode.
  if constexpr (rotation == TextRotation::Rotated90CW) {
    const int ob = cursorX + fontData->ascender - top;
    const int ib = cursorY - left;
    if (!renderer.glyphIntersectsStrip(ob, ib - (width - 1), ob + height - 1, ib)) {
      return;
    }
  } else {
    const int gx0 = cursorX + left;
    const int gy0 = cursorY - top;
    if (!renderer.glyphIntersectsStrip(gx0, gy0, gx0 + width - 1, gy0 + height - 1)) {
      return;
    }
  }

  if (glyph == &solidFallback) {
    if constexpr (rotation == TextRotation::Rotated90CW) {
      renderer.fillRect(cursorX + fontData->ascender - top, cursorY - left - width + 1, height, width, pixelState);
    } else {
      renderer.fillRect(cursorX + left, cursorY - top, width, height, pixelState);
    }
    return;
  }
  const uint8_t* bitmap = renderer.getGlyphBitmap(fontData, glyph);
  if (bitmap == nullptr) return;

  // Logical placement of glyph pixel (0, 0) and the logical step for one move
  // along each glyph axis. Rotated text runs glyph x up the screen and glyph
  // y to the right.
  glyphBitmap::Frame frame;
  if constexpr (rotation == TextRotation::Rotated90CW) {
    frame = {cursorX + fontData->ascender - top, cursorY - left, 0, -1, 1, 0};
  } else {
    frame = {cursorX + left, cursorY - top, 1, 0, 0, 1};
  }
  renderer.drawGlyphBitmap(bitmap, width, height, frame, is2Bit, renderMode, pixelState);
}

// Draw an unscaled glyph placed by a logical frame: as if drawPixel() were called for each ink
// pixel, with clipping, rotation, the strip band and addressing resolved once per glyph in Rust.
void GfxRenderer::drawGlyphBitmap(const uint8_t* bitmap, const int width, const int height,
                                  const glyphBitmap::Frame& frame, const bool twoBit, const RenderMode mode,
                                  const bool state) const {
  if (width <= 0 || height <= 0) return;
  static_assert(sizeof(bb_gfx_frame) == 6 * sizeof(int32_t), "bb_gfx_frame layout");
  const bb_gfx_frame placed{frame.x, frame.y, frame.dxX, frame.dxY, frame.dyX, frame.dyY};
  const auto c = canvasOf(*this);
  bb_gfx_glyph(&c, bitmap, glyphBytes(width, height, twoBit), width, height, &placed, twoBit,
               static_cast<uint8_t>(mode), state);
}

// IMPORTANT: This function is in critical rendering path and is called for every pixel. Please keep it as simple and
// efficient as possible.
void GfxRenderer::drawPixel(const int x, const int y, const bool state) const {
  if (x < clipLeft_ || y < clipTop_ || x >= clipRight_ || y >= clipBottom_) return;
  int phyX = 0;
  int phyY = 0;

  // Note: this call should be inlined for better performance
  rotateCoordinates(orientation, x, y, &phyX, &phyY, panelWidth, panelHeight);

  // Bounds checking against runtime panel dimensions
  if (phyX < 0 || phyX >= panelWidth || phyY < 0 || phyY >= panelHeight) {
    LOG_ERR("GFX", "!! Outside range (%d, %d) -> (%d, %d)", x, y, phyX, phyY);
    return;
  }

  // Tiled grayscale: redirect writes to the strip scratch and clip to the
  // current band. Single predictable branch on the hot per-pixel path.
  uint8_t* target = frameBuffer;
  uint32_t rowY = static_cast<uint32_t>(phyY);
  if (_stripActive) {
    if (phyY < _stripY0 || phyY >= _stripY0 + _stripRows) {
      return;  // pixel outside the band currently being rendered
    }
    target = _stripBuf;
    rowY = static_cast<uint32_t>(phyY - _stripY0);
  }

  // Calculate byte position and bit position
  const uint32_t byteIndex = rowY * panelWidthBytes + (phyX / 8);
  const uint8_t bitPosition = 7 - (phyX % 8);  // MSB first

  if (state) {
    target[byteIndex] &= ~(1 << bitPosition);  // Clear bit
  } else {
    target[byteIndex] |= 1 << bitPosition;  // Set bit
  }
}

int GfxRenderer::getTextWidth(const int fontId, const char* text, const EpdFontFamily::Style style,
                              const BidiUtils::BidiBaseDir baseDir) const {
  if (text == nullptr || *text == '\0') {
    return 0;
  }

  // Measure with the same font drawText would render with (see resolveTextFontId)
  // so wrapping, truncation and centering of non-Latin strings stay consistent.
  const int resolvedFontId = resolveTextFontId(fontId, text, style);
  const auto fontIt = fontMap.find(resolvedFontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", resolvedFontId);
    return 0;
  }

  std::string visual;
  const char* renderedText = resolveVisualText(text, visual, baseDir);

  // Redirected to the SD fallback: batch-load the string's glyphs so the
  // per-codepoint measurement loop below doesn't fault them in one SD read
  // at a time (#2725).
  if (resolvedFontId != fontId) {
    ensureSdGlyphsResident(resolvedFontId, renderedText, style, true);
  }

  int w = 0, h = 0;
  fontIt->second.getTextDimensions(renderedText, &w, &h, style);
  return w;
}

void GfxRenderer::drawCenteredText(const int fontId, const int y, const char* text, const bool black,
                                   const EpdFontFamily::Style style, const BidiUtils::BidiBaseDir baseDir) const {
  const int x = (getScreenWidth() - getTextWidth(fontId, text, style, baseDir)) / 2;
  drawText(fontId, x, y, text, black, style, baseDir);
}

void GfxRenderer::drawText(const int fontId, const int x, const int y, const char* text, const bool black,
                           const EpdFontFamily::Style style, const BidiUtils::BidiBaseDir baseDir,
                           const int8_t tracking) const {
  // cannot draw a NULL / empty string
  if (text == nullptr || *text == '\0') {
    return;
  }

  // Route non-Latin strings to the fallback font when the requested font
  // lacks the glyphs (e.g. Chinese book titles drawn with a Latin UI font).
  const int resolvedFontId = resolveTextFontId(fontId, text, style);

  std::string visual;
  const char* renderedText = resolveVisualText(text, visual, baseDir);

  // Baseline from the resolved font; when the string was redirected to the
  // fallback, the caller positioned this line with the REQUESTED font's
  // metrics (row bands, icon centering), so center the fallback's line box
  // inside the requested font's line box instead of letting a taller/shorter
  // fallback hang below or float above the row's visual center.
  int yPos = y + getFontAscenderSize(resolvedFontId);
  if (resolvedFontId != fontId) {
    yPos += (getLineHeight(fontId) - getLineHeight(resolvedFontId)) / 2;
  }
  int lastBaseX = x;
  int lastBaseLeft = 0;
  int lastBaseWidth = 0;
  int lastBaseTop = 0;
  int32_t prevAdvanceFP = 0;  // 12.4 fixed-point: prev glyph's advance + next kern for snap

  if (fontCacheManager_ && fontCacheManager_->isScanning()) {
    fontCacheManager_->recordText(renderedText, resolvedFontId, style);
    return;
  }

  // Redirected to the SD fallback: batch-load the string's glyphs so the draw
  // loop below doesn't fault them in one SD read at a time (#2725).
  if (resolvedFontId != fontId) {
    ensureSdGlyphsResident(resolvedFontId, renderedText, style, false);
  }

  const auto fontIt = fontMap.find(resolvedFontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", resolvedFontId);
    return;
  }
  const auto& font = fontIt->second;

  const char* textCursor = renderedText;
  uint32_t cp;
  uint32_t prevCp = 0;
  while ((cp = utf8NextCodepoint(reinterpret_cast<const uint8_t**>(&textCursor)))) {
    // RTL vowel marks (Hebrew niqqud, Arabic harakat) ride the combining-mark
    // path: zero-advance overlays on the preceding base glyph (applyBidiVisual
    // emits base-then-marks per UAX#9 L3). anchorFor pins position-sensitive
    // niqqud (dagesh, shin/sin dots, holam) to their spot on the base; other
    // marks stay centered, raised above the base or (kasra) at their
    // font-native position. Fonts without their glyphs — the built-ins — miss
    // the getGlyph lookup and skip them, as before.
    if (utf8IsCombiningMark(cp) || BidiUtils::isTransparentMark(cp)) {
      const EpdGlyph* combiningGlyph = font.getGlyph(cp, style);
      if (!combiningGlyph) continue;
      const auto anchor = combiningMark::anchorFor(cp);
      const int raiseBy =
          combiningMark::raiseAboveBase(anchor, combiningGlyph->top, combiningGlyph->height, lastBaseTop);
      const int combiningX = combiningMark::anchorOver(anchor, lastBaseX, lastBaseLeft, lastBaseWidth,
                                                       combiningGlyph->left, combiningGlyph->width);
      renderCharImpl<TextRotation::None>(*this, renderMode, font, cp, combiningX, yPos - raiseBy, black, style);
      continue;
    }

    const uint32_t smallCapCp = (style & EpdFontFamily::SMALL_CAPS) ? utf8SmallCapsUpper(cp) : cp;
    const bool isSmallCap = smallCapCp != cp;
    if (style & EpdFontFamily::SMALL_CAPS) {
      cp = smallCapCp;  // no ligatures: small caps set each letter separately
    } else {
      cp = font.applyLigatures(cp, textCursor, style);
    }

    // Differential rounding: snap (previous advance + current kern) as one unit so
    // identical character pairs always produce the same pixel step regardless of
    // where they fall on the line.
    if (prevCp != 0) {
      const auto kernFP = font.getKerning(prevCp, cp, style);  // 4.4 fixed-point kern
      lastBaseX += fp4::toPixel(prevAdvanceFP + kernFP) + trackingBetween(prevCp, cp, tracking);
    }

    EpdGlyph solidFallback;
    const EpdGlyph* glyph = font.getGlyphMetrics(cp, solidFallback, style);

    lastBaseLeft = glyph ? glyph->left : 0;
    lastBaseWidth = glyph ? glyph->width : 0;
    lastBaseTop = glyph ? glyph->top : 0;
    prevAdvanceFP = glyph ? glyph->advanceX : 0;  // 12.4 fixed-point

    const bool isSupSub = (style & (EpdFontFamily::SUP | EpdFontFamily::SUB)) != 0;
    if (isSupSub) {
      // Halve the advance so the cursor advances by the same amount the scaled glyph
      // actually occupies, keeping spacing correct without needing a separate smaller font.
      prevAdvanceFP = (prevAdvanceFP + 1) / 2;
    } else if (isSmallCap) {
      prevAdvanceFP = scaleSmallCapAdvance(prevAdvanceFP);
    }

    if (isSupSub) {
      // yPos already carries the vertical offset applied by TextBlock::render().
      renderCharScaled(*this, renderMode, font, cp, lastBaseX, yPos, black, style, 1, 2);
    } else if (isSmallCap) {
      renderCharScaled(*this, renderMode, font, cp, lastBaseX, yPos, black, style, SMALL_CAPS_SCALE_NUM,
                       SMALL_CAPS_SCALE_DEN);
    } else {
      renderCharImpl<TextRotation::None>(*this, renderMode, font, cp, lastBaseX, yPos, black, style);
    }
    prevCp = cp;
  }
}

namespace {
const char* resolveVisualText(const char* text, std::string& visualBuffer, const BidiUtils::BidiBaseDir baseDir) {
  if (!text || *text == '\0') return text;

  if (baseDir != BidiUtils::BidiBaseDir::RTL) {
    // Byte-level scan: skip BiDi when no RTL script lead bytes are present.
    // Hebrew UTF-8 lead bytes: 0xD6-0xD7; Arabic/Syriac: 0xD8-0xDB.
    // This covers all RTL content without false negatives and avoids triggering
    // the full UAX#9 algorithm for Latin-extended, em-dashes, accented text, etc.
    bool hasRtlBytes = false;
    for (const unsigned char* q = reinterpret_cast<const unsigned char*>(text); *q; ++q) {
      if (*q >= 0xD6 && *q <= 0xDB) {
        hasRtlBytes = true;
        break;
      }
    }
    if (!hasRtlBytes) return text;
  }

  if (BidiUtils::applyBidiVisual(text, visualBuffer, static_cast<int>(baseDir)) && !visualBuffer.empty()) {
    return visualBuffer.c_str();
  }
  return text;
}
}  // namespace

// The layout bembeyaz-ffi checks too.
static_assert(sizeof(bb_gfx_canvas) == 4 * sizeof(size_t) + 32 &&
                  offsetof(bb_gfx_canvas, origin_y) == 3 * sizeof(size_t) &&
                  offsetof(bb_gfx_canvas, orientation) == 3 * sizeof(size_t) + 32,
              "bb_gfx_canvas layout");
static_assert(GfxRenderer::Portrait == BB_GFX_PORTRAIT &&
                  GfxRenderer::LandscapeClockwise == BB_GFX_LANDSCAPE_CLOCKWISE &&
                  GfxRenderer::PortraitInverted == BB_GFX_PORTRAIT_INVERTED &&
                  GfxRenderer::LandscapeCounterClockwise == BB_GFX_LANDSCAPE_COUNTER_CLOCKWISE,
              "bb_gfx_canvas orientations");
static_assert(Color::White == BB_GFX_WHITE && Color::LightGray == BB_GFX_LIGHT_GRAY &&
                  Color::DarkGray == BB_GFX_DARK_GRAY && Color::Black == BB_GFX_BLACK,
              "bb_gfx_fill_rect colors");

namespace {
// The rows being drawn into (the framebuffer, or the active strip) and the clip, for Rust.
bb_gfx_canvas canvasOf(const GfxRenderer& renderer) {
  bb_gfx_canvas c{};
  c.buf = renderer.getWriteTarget();
  c.stride = renderer.getDisplayWidthBytes();
  c.origin_y = renderer.getWriteOriginY();
  c.rows = renderer.getWriteRows();
  c.len = static_cast<size_t>(c.rows) * c.stride;
  c.panel_width = renderer.getDisplayWidth();
  c.panel_height = renderer.getDisplayHeight();
  const auto [clipX, clipY, clipW, clipH] = renderer.getClipRect();
  c.clip_left = clipX;
  c.clip_top = clipY;
  c.clip_right = clipX + clipW;
  c.clip_bottom = clipY + clipH;
  c.orientation = static_cast<uint8_t>(renderer.getOrientation());
  // Rust draws nothing into a missing buffer (lent out for a build, or before begin()); say so once.
  static bool reported = false;
  if (!c.buf && !reported) {
    reported = true;
    LOG_ERR("GFX", "!! drawing without a framebuffer");
  }
  return c;
}
}  // namespace

namespace {
// The whole framebuffer, whatever strip is being drawn, for the region copies (which ignore the
// clip).
bb_gfx_canvas frameCanvasOf(const GfxRenderer& renderer) {
  bb_gfx_canvas c{};
  c.buf = renderer.getFrameBuffer();
  c.stride = renderer.getDisplayWidthBytes();
  c.rows = renderer.getDisplayHeight();
  c.len = c.buf ? static_cast<size_t>(c.rows) * c.stride : 0;
  c.panel_width = renderer.getDisplayWidth();
  c.panel_height = renderer.getDisplayHeight();
  c.orientation = static_cast<uint8_t>(renderer.getOrientation());
  return c;
}
}  // namespace

namespace {
void logOffPanel(const uint32_t pixels) {
  if (pixels > 0) LOG_ERR("GFX", "!! %u pixels outside the panel", static_cast<unsigned>(pixels));
}
}  // namespace

void GfxRenderer::drawLine(const int x1, const int y1, const int x2, const int y2, const bool state) const {
  drawLine(x1, y1, x2, y2, 1, state);
}

void GfxRenderer::drawLine(const int x1, const int y1, const int x2, const int y2, const int lineWidth,
                           const bool state) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  logOffPanel(bb_gfx_line(&c, x1, y1, x2, y2, lineWidth, state));
}

void GfxRenderer::drawRect(const int x, const int y, const int width, const int height, const bool state) const {
  drawRect(x, y, width, height, 1, state);
}

// Border is inside the rectangle
void GfxRenderer::drawRect(const int x, const int y, const int width, const int height, const int lineWidth,
                           const bool state) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  logOffPanel(bb_gfx_rect(&c, x, y, width, height, lineWidth, state));
}

void GfxRenderer::drawArc(const int maxRadius, const int cx, const int cy, const int xDir, const int yDir,
                          const int lineWidth, const bool state) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  bb_gfx_arc(&c, maxRadius, cx, cy, xDir, yDir, lineWidth, state);
}

namespace {
uint8_t roundCorners(const bool topLeft, const bool topRight, const bool bottomLeft, const bool bottomRight) {
  return (topLeft ? BB_GFX_TOP_LEFT : 0) | (topRight ? BB_GFX_TOP_RIGHT : 0) | (bottomLeft ? BB_GFX_BOTTOM_LEFT : 0) |
         (bottomRight ? BB_GFX_BOTTOM_RIGHT : 0);
}
}  // namespace

// Border is inside the rectangle, rounded corners
void GfxRenderer::drawRoundedRect(const int x, const int y, const int width, const int height, const int lineWidth,
                                  const int cornerRadius, bool state) const {
  drawRoundedRect(x, y, width, height, lineWidth, cornerRadius, true, true, true, true, state);
}

// Border is inside the rectangle, rounded corners
void GfxRenderer::drawRoundedRect(const int x, const int y, const int width, const int height, const int lineWidth,
                                  const int cornerRadius, bool roundTopLeft, bool roundTopRight, bool roundBottomLeft,
                                  bool roundBottomRight, bool state) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  logOffPanel(bb_gfx_rounded_rect(&c, x, y, width, height, lineWidth, cornerRadius,
                                  roundCorners(roundTopLeft, roundTopRight, roundBottomLeft, roundBottomRight), state));
}

void GfxRenderer::fillRect(const int x, const int y, const int width, const int height, const bool state) const {
  if (state) {
    fillRectImpl<Color::Black>(x, y, width, height);
  } else {
    fillRectImpl<Color::White>(x, y, width, height);
  }
}

void GfxRenderer::fillRectDither(const int x, const int y, const int width, const int height, Color color) const {
  switch (color) {
    case Color::Clear:
      break;
    case Color::Black:
      fillRectImpl<Color::Black>(x, y, width, height);
      break;
    case Color::White:
      fillRectImpl<Color::White>(x, y, width, height);
      break;
    case Color::LightGray:
      fillRectImpl<Color::LightGray>(x, y, width, height);
      break;
    case Color::DarkGray:
      fillRectImpl<Color::DarkGray>(x, y, width, height);
      break;
  }
}

template <Color C>
void GfxRenderer::fillRectImpl(const int x, const int y, const int width, const int height) const {
  if constexpr (C == Color::Clear) return;
  if (width <= 0 || height <= 0) return;
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  bb_gfx_fill_rect(&c, x, y, width, height, static_cast<uint8_t>(C));
}

template void GfxRenderer::fillRectImpl<Color::Black>(int, int, int, int) const;
template void GfxRenderer::fillRectImpl<Color::White>(int, int, int, int) const;
template void GfxRenderer::fillRectImpl<Color::LightGray>(int, int, int, int) const;
template void GfxRenderer::fillRectImpl<Color::DarkGray>(int, int, int, int) const;

void GfxRenderer::maskRoundedRectOutsideCorners(const int x, const int y, const int width, const int height,
                                                const int radius, const Color color) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  logOffPanel(bb_gfx_mask_corners(&c, x, y, width, height, radius, static_cast<uint8_t>(color)));
}

void GfxRenderer::fillRoundedRect(const int x, const int y, const int width, const int height, const int cornerRadius,
                                  const Color color) const {
  fillRoundedRect(x, y, width, height, cornerRadius, true, true, true, true, color);
}

void GfxRenderer::fillRoundedRect(const int x, const int y, const int width, const int height, const int cornerRadius,
                                  bool roundTopLeft, bool roundTopRight, bool roundBottomLeft, bool roundBottomRight,
                                  const Color color) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  bb_gfx_fill_rounded_rect(&c, x, y, width, height, cornerRadius,
                           roundCorners(roundTopLeft, roundTopRight, roundBottomLeft, roundBottomRight),
                           static_cast<uint8_t>(color));
}

void GfxRenderer::drawImage(const uint8_t bitmap[], const int x, const int y, const int width, const int height) const {
  int rotatedX = 0;
  int rotatedY = 0;
  rotateCoordinates(orientation, x, y, &rotatedX, &rotatedY, panelWidth, panelHeight);
  // Rotate origin corner
  switch (orientation) {
    case Portrait:
      rotatedY = rotatedY - height;
      break;
    case PortraitInverted:
      rotatedX = rotatedX - width;
      break;
    case LandscapeClockwise:
      rotatedY = rotatedY - height;
      rotatedX = rotatedX - width;
      break;
    case LandscapeCounterClockwise:
      break;
  }
  // TODO: Rotate bits
  display.drawImage(bitmap, rotatedX, rotatedY, width, height);
}

void GfxRenderer::drawLevelRow(const uint8_t* row, const int width, const int x, const int y) const {
  if (width <= 0 || (fontCacheManager_ && fontCacheManager_->isScanning())) return;
  const auto c = canvasOf(*this);
  const size_t bytes = (static_cast<size_t>(width) + 3) / 4;
  bb_gfx_level_row(&c, row, bytes, width, x, y, static_cast<uint8_t>(renderMode), absoluteGrayPlanes);
}

void GfxRenderer::drawIcon(const uint8_t bitmap[], const int x, const int y, const int size) const {
  // Pixel by pixel in Rust rather than a byte-aligned blit, which would snap the icon to 8 px along
  // the rotated axis. The themes that call this all draw in Portrait.
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  if (size <= 0) return;
  const auto c = canvasOf(*this);
  const size_t bytes = static_cast<size_t>(size) * static_cast<size_t>((size + 7) / 8);
  logOffPanel(bb_gfx_icon(&c, bitmap, bytes, x, y, size));
}

static_assert(sizeof(bb_gfx_picture) == 40 && offsetof(bb_gfx_picture, crop_x) == 24 &&
                  offsetof(bb_gfx_picture, plane) == 36 && sizeof(bb_gfx_drawn) == 24,
              "bb_gfx_picture layout");
static_assert(GfxRenderer::BW == BB_GFX_PLANE_BW && GfxRenderer::GRAYSCALE_LSB == BB_GFX_PLANE_GRAY_LSB &&
                  GfxRenderer::GRAYSCALE_MSB == BB_GFX_PLANE_GRAY_MSB,
              "bb_gfx_picture planes");

namespace {
// Feeds Rust a Bitmap's rows.
struct BitmapRows {
  const Bitmap& bitmap;
  uint8_t* fileRow;
};

bool readBitmapRow(void* ctx, uint8_t* row) {
  auto* rows = static_cast<BitmapRows*>(ctx);
  return rows->bitmap.readNextRow(row, rows->fileRow) == BmpReaderError::Ok;
}
}  // namespace

bool GfxRenderer::drawPicture(const Bitmap& bitmap, const int x, const int y, const int maxWidth, const int maxHeight,
                              const float cropX, const float cropY, const bool whiteAsTransparent,
                              const bool oneBit) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return false;
  // One packed output row (2 bits per pixel; int, as wide images overflow a byte) and one file row.
  const int outputRowSize = (bitmap.getWidth() + 3) / 4;
  const auto rowBytesSize = static_cast<size_t>(bitmap.getRowBytes());
  auto rowScratch = makeUniqueNoThrow<uint8_t[]>(static_cast<size_t>(outputRowSize) + rowBytesSize);
  if (!rowScratch) {
    LOG_ERR("GFX", oneBit ? "!! Failed to allocate 1-bit BMP row buffers" : "!! Failed to allocate BMP row buffers");
    return false;
  }

  bb_gfx_picture picture{};
  picture.width = bitmap.getWidth();
  picture.height = bitmap.getHeight();
  picture.x = x;
  picture.y = y;
  picture.max_width = maxWidth;
  picture.max_height = maxHeight;
  picture.crop_x = cropX;
  picture.crop_y = cropY;
  picture.top_down = bitmap.isTopDown();
  picture.one_bit = oneBit;
  picture.white_transparent = whiteAsTransparent;
  picture.absolute_gray = absoluteGrayPlanes;
  picture.plane = static_cast<uint8_t>(renderMode);
  BitmapRows rows{bitmap, rowScratch.get() + outputRowSize};
  bb_gfx_drawn drawn{};
  const auto c = canvasOf(*this);
  if (!bb_gfx_draw_picture(&c, &picture, rowScratch.get(), static_cast<size_t>(outputRowSize), readBitmapRow, &rows,
                           &drawn)) {
    if (oneBit) {
      LOG_ERR("GFX", "Failed to read row %d from 1-bit bitmap", drawn.failed_row);
    } else {
      LOG_ERR("GFX", "Failed to read row %d from bitmap", drawn.failed_row);
    }
    return false;
  }
  if (!oneBit) {
    LOG_DBG("GFX", "Cropped %dx%d by %dx%d pix, is %s, scaled by %f", bitmap.getWidth(), bitmap.getHeight(),
            drawn.crop_x, drawn.crop_y, bitmap.isTopDown() ? "top-down" : "bottom-up", drawn.scale);
  }
  preserveImagePolarity(x, y, drawn.width, drawn.height);
  return true;
}

bool GfxRenderer::drawBitmap(const Bitmap& bitmap, const int x, const int y, const int maxWidth, const int maxHeight,
                             const float cropX, const float cropY, const bool whiteAsTransparent) const {
  // 1-bit bitmaps take the 1-bit path, which has no crop.
  const bool oneBit = bitmap.is1Bit() && cropX == 0.0f && cropY == 0.0f;
  return drawPicture(bitmap, x, y, maxWidth, maxHeight, cropX, cropY, whiteAsTransparent, oneBit);
}

bool GfxRenderer::drawBitmap1Bit(const Bitmap& bitmap, const int x, const int y, const int maxWidth,
                                 const int maxHeight) const {
  return drawPicture(bitmap, x, y, maxWidth, maxHeight, 0.0f, 0.0f, false, true);
}

void GfxRenderer::preserveImagePolarity(const int x, const int y, const int width, const int height) const {
  if (renderMode != BW || !display.isInverted() || _stripActive || !frameBuffer) return;
  const auto c = canvasOf(*this);
  bb_gfx_invert_rect(&c, x, y, width, height);
}

void GfxRenderer::fillPolygon(const int* xPoints, const int* yPoints, int numPoints, bool state) const {
  if (numPoints < 3) return;
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return;
  const auto c = canvasOf(*this);
  bb_gfx_fill_polygon(&c, xPoints, yPoints, static_cast<size_t>(numPoints), state);
}

// For performance measurement (using static to allow "const" methods)
static unsigned long start_ms = 0;

void GfxRenderer::clearScreen(const uint8_t color) const {
  start_ms = millis();
  if (_stripActive) {
    // Clear only the active band's scratch, not the shared framebuffer.
    memset(_stripBuf, color, static_cast<size_t>(panelWidthBytes) * _stripRows);
    return;
  }
  display.clearScreen(color);
}

void GfxRenderer::beginStripTarget(uint8_t* scratch, int stripY0, int stripRows) const {
  // Band is caller-guaranteed in-bounds (the reader's grayscale loop computes
  // it); assert catches future misuse in debug before it mis-renders or wraps
  // the downstream uint16_t cast in writeGrayscalePlaneStrip.
  assert(scratch != nullptr && stripRows > 0 && stripY0 >= 0 && stripY0 <= static_cast<int>(panelHeight) - stripRows);
  _stripBuf = scratch;
  _stripY0 = stripY0;
  _stripRows = stripRows;
  _stripActive = true;
}

void GfxRenderer::endStripTarget() const {
  _stripActive = false;
  _stripBuf = nullptr;
  _stripY0 = 0;
  _stripRows = 0;
}

bool GfxRenderer::glyphIntersectsStrip(int x0, int y0, int x1, int y1) const {
  if (!_stripActive) {
    return true;
  }
  // Rotate the two opposite bbox corners to physical coords. For 90-degree
  // orientations the physical bbox stays axis-aligned, so min/max of the two
  // rotated corners' Y bounds the glyph's physical y-extent.
  int ax, ay, bx, by;
  rotateCoordinates(orientation, x0, y0, &ax, &ay, panelWidth, panelHeight);
  rotateCoordinates(orientation, x1, y1, &bx, &by, panelWidth, panelHeight);
  const int minY = ay < by ? ay : by;
  const int maxY = ay > by ? ay : by;
  return !(maxY < _stripY0 || minY >= _stripY0 + _stripRows);
}

void GfxRenderer::invertScreen() const {
  for (uint32_t i = 0; i < frameBufferSize; i++) {
    frameBuffer[i] = ~frameBuffer[i];
  }
}

HalDisplay::RefreshMode GfxRenderer::applyPromotedRefresh(const HalDisplay::RefreshMode refreshMode) const {
  if (!promotedRefreshPending_) return refreshMode;
  promotedRefreshPending_ = false;
  return promotedRefresh_;
}

void GfxRenderer::displayBuffer(HalDisplay::RefreshMode refreshMode) const {
  auto elapsed = millis() - start_ms;
  LOG_DBG("GFX", "Time = %lu ms from clearScreen to displayBuffer", elapsed);
  refreshMode = applyPromotedRefresh(refreshMode);
  display.displayBuffer(refreshMode, fadingFix);
}

void GfxRenderer::displayBufferAsync(HalDisplay::RefreshMode refreshMode) const {
  refreshMode = applyPromotedRefresh(refreshMode);
  // The async path has no turn-off-screen hook, which the sunlight fading fix
  // relies on; keep those users on the blocking path.
  if (fadingFix) {
    display.displayBuffer(refreshMode, fadingFix);
    return;
  }
  display.displayBufferAsync(refreshMode);
}

void GfxRenderer::waitRefreshComplete() const { display.waitRefreshComplete(); }

bool GfxRenderer::supportsAsyncRefresh() const { return !fadingFix && display.supportsAsyncRefresh(); }

HalDisplay::GrayscaleCapabilities GfxRenderer::grayscaleCapabilities(HalDisplay::GrayscaleMode mode) const {
  auto caps = display.grayscaleCapabilities(mode);
  if (fadingFix) caps.asyncBase = false;
  return caps;
}

bool GfxRenderer::supportsAsyncGrayscaleBase() const { return grayscaleCapabilities().asyncBase; }

size_t GfxRenderer::readFramebufferRegion(int x, int y, int w, int h, uint8_t* dst, size_t dstCapacity) const {
  if (dst == nullptr) return 0;
  const auto c = frameCanvasOf(*this);
  return bb_gfx_read_region(&c, x, y, w, h, true, dst, dstCapacity);
}

void GfxRenderer::writeFramebufferRegion(int x, int y, int w, int h, const uint8_t* src) {
  if (src == nullptr) return;
  // src holds a copy readFramebufferRegion made of the same rectangle.
  const auto c = frameCanvasOf(*this);
  (void)bb_gfx_write_region(&c, x, y, w, h, true, src, bb_gfx_region_size(&c, x, y, w, h, true));
}

std::string GfxRenderer::truncatedText(const int fontId, const char* text, const int maxWidth,
                                       const EpdFontFamily::Style style) const {
  if (!text || maxWidth <= 0) return "";

  std::string item = text;
  // U+2026 HORIZONTAL ELLIPSIS (UTF-8: 0xE2 0x80 0xA6)
  const char* ellipsis = "\xe2\x80\xa6";
  int textWidth = getTextWidth(fontId, item.c_str(), style);
  if (textWidth <= maxWidth) {
    // Text fits, return as is
    return item;
  }

  size_t charCount = 0;
  for (const unsigned char c : item) {
    if ((c & 0xC0) != 0x80) ++charCount;
  }

  std::string candidate;
  candidate.reserve(item.size() + 3);
  const auto setCandidate = [&](const size_t characterCount) {
    size_t end = 0;
    for (size_t count = 0; end < item.size() && count < characterCount; ++count) {
      ++end;
      while (end < item.size() && (static_cast<unsigned char>(item[end]) & 0xC0) == 0x80) ++end;
    }
    candidate.assign(item.data(), end);
    candidate += ellipsis;
  };

  // Binary search avoids repeatedly measuring nearly the entire long title.
  size_t low = 0;
  size_t high = charCount;
  while (low < high) {
    const size_t mid = low + (high - low + 1) / 2;
    setCandidate(mid);
    if (getTextWidth(fontId, candidate.c_str(), style) < maxWidth) {
      low = mid;
    } else {
      high = mid - 1;
    }
  }

  setCandidate(low);
  return candidate;
}

std::vector<std::string> GfxRenderer::wrappedText(const int fontId, const char* text, const int maxWidth,
                                                  const int maxLines, const EpdFontFamily::Style style) const {
  std::vector<std::string> lines;

  if (!text || maxWidth <= 0 || maxLines <= 0) return lines;

  std::string remaining = text;
  std::string currentLine;

  while (!remaining.empty()) {
    if (static_cast<int>(lines.size()) == maxLines - 1) {
      // Last available line: combine any word already started on this line with
      // the rest of the text, then let truncatedText fit it with an ellipsis.
      std::string lastContent = currentLine.empty() ? remaining : currentLine + " " + remaining;
      lines.push_back(truncatedText(fontId, lastContent.c_str(), maxWidth, style));
      return lines;
    }

    // Find next word
    size_t spacePos = remaining.find(' ');
    std::string word;

    if (spacePos == std::string::npos) {
      word = remaining;
      remaining.clear();
    } else {
      word = remaining.substr(0, spacePos);
      remaining.erase(0, spacePos + 1);
    }

    std::string testLine = currentLine.empty() ? word : currentLine + " " + word;

    if (getTextWidth(fontId, testLine.c_str(), style) <= maxWidth) {
      currentLine = testLine;
    } else if (!currentLine.empty()) {
      lines.push_back(currentLine);
      currentLine.clear();
      // Requeue the word instead of deciding its fate here: the loop top then
      // applies the last-line ellipsis or the overlong split with the correct
      // remaining line budget.
      remaining = remaining.empty() ? word : word + " " + remaining;
    } else {
      // Single word wider than the line — a no-space filename or a CJK run,
      // which has no space boundaries at all. Hard-split at the widest UTF-8
      // prefix that fits (never below one codepoint, so the loop advances) and
      // requeue the rest to keep wrapping.
      size_t fit = 0;
      while (fit < word.size()) {
        size_t next = fit + 1;
        while (next < word.size() && (static_cast<unsigned char>(word[next]) & 0xC0) == 0x80) next++;
        if (fit > 0 && getTextWidth(fontId, word.substr(0, next).c_str(), style) > maxWidth) break;
        fit = next;
      }
      lines.push_back(word.substr(0, fit));
      const std::string rest = word.substr(fit);
      if (!rest.empty()) remaining = remaining.empty() ? rest : rest + " " + remaining;
    }
  }

  if (!currentLine.empty() && static_cast<int>(lines.size()) < maxLines) {
    lines.push_back(currentLine);
  }

  return lines;
}

// Note: Internal driver treats screen in command orientation; this library exposes a logical orientation
int GfxRenderer::getScreenWidth() const {
  switch (orientation) {
    case Portrait:
    case PortraitInverted:
      // 480px wide in portrait logical coordinates
      return panelHeight;
    case LandscapeClockwise:
    case LandscapeCounterClockwise:
      // 800px wide in landscape logical coordinates
      return panelWidth;
  }
  return panelHeight;
}

int GfxRenderer::getScreenHeight() const {
  switch (orientation) {
    case Portrait:
    case PortraitInverted:
      // 800px tall in portrait logical coordinates
      return panelWidth;
    case LandscapeClockwise:
    case LandscapeCounterClockwise:
      // 480px tall in landscape logical coordinates
      return panelHeight;
  }
  return panelWidth;
}

void GfxRenderer::tapToLogical(float nx, float ny, int& outX, int& outY) const {
  int phyX = static_cast<int>(nx * panelWidth);
  int phyY = static_cast<int>(ny * panelHeight);
  if (phyX < 0) phyX = 0;
  if (phyX > panelWidth - 1) phyX = panelWidth - 1;
  if (phyY < 0) phyY = 0;
  if (phyY > panelHeight - 1) phyY = panelHeight - 1;

  switch (orientation) {
    case Portrait:
      outX = panelHeight - 1 - phyY;
      outY = phyX;
      break;
    case PortraitInverted:
      outX = phyY;
      outY = panelWidth - 1 - phyX;
      break;
    case LandscapeClockwise:
      outX = panelWidth - 1 - phyX;
      outY = panelHeight - 1 - phyY;
      break;
    case LandscapeCounterClockwise:
    default:
      outX = phyX;
      outY = phyY;
      break;
  }
}

size_t GfxRenderer::getRegionByteSize(int lx, int ly, int lw, int lh) const {
  const auto c = frameCanvasOf(*this);
  return bb_gfx_region_size(&c, lx, ly, lw, lh, false);
}

bool GfxRenderer::copyRegionToBuffer(int lx, int ly, int lw, int lh, uint8_t* buf, size_t bufSize) const {
  const auto c = frameCanvasOf(*this);
  return bb_gfx_read_region(&c, lx, ly, lw, lh, false, buf, bufSize) > 0;
}

bool GfxRenderer::copyBufferToRegion(int lx, int ly, int lw, int lh, const uint8_t* buf, size_t bufSize) const {
  const auto c = frameCanvasOf(*this);
  return bb_gfx_write_region(&c, lx, ly, lw, lh, false, buf, bufSize);
}

int GfxRenderer::getSpaceWidth(const int fontId, const EpdFontFamily::Style style) const {
  // Advance table fast-path for SD card fonts during layout
  auto sdIt = sdCardFonts_.find(fontId);
  if (sdIt != sdCardFonts_.end() && sdIt->second->hasAdvanceTable()) {
    return fp4::toPixel(getSdCardSpaceAdvance(*sdIt->second, style));
  }

  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", fontId);
    return 0;
  }

  const EpdGlyph* spaceGlyph = fontIt->second.getGlyph(' ', style);
  return spaceGlyph ? fp4::toPixel(spaceGlyph->advanceX) : 0;  // snap 12.4 fixed-point to nearest pixel
}

int GfxRenderer::getSpaceAdvance(const int fontId, const uint32_t leftCp, const uint32_t rightCp,
                                 const EpdFontFamily::Style style) const {
  // Advance table fast-path for SD card fonts during layout.
  // Kern data is not loaded during layout (consistent with previous metadataOnly behavior),
  // so we return just the space advance without kerning.
  auto sdIt = sdCardFonts_.find(fontId);
  if (sdIt != sdCardFonts_.end() && sdIt->second->hasAdvanceTable()) {
    return fp4::toPixel(getSdCardSpaceAdvance(*sdIt->second, style));
  }

  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) return 0;
  const auto& font = fontIt->second;
  const EpdGlyph* spaceGlyph = font.getGlyph(' ', style);
  const int32_t spaceAdvanceFP = spaceGlyph ? static_cast<int32_t>(spaceGlyph->advanceX) : 0;
  // Combine space advance + flanking kern into one fixed-point sum before snapping.
  // Snapping the combined value avoids the +/-1 px error from snapping each component separately.
  const int32_t kernFP = static_cast<int32_t>(font.getKerning(leftCp, ' ', style)) +
                         static_cast<int32_t>(font.getKerning(' ', rightCp, style));
  return fp4::toPixel(spaceAdvanceFP + kernFP);
}

int GfxRenderer::getKerning(const int fontId, const uint32_t leftCp, const uint32_t rightCp,
                            const EpdFontFamily::Style style, const int8_t tracking) const {
  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) return 0;
  const int kernFP = fontIt->second.getKerning(leftCp, rightCp, style);      // 4.4 fixed-point
  return fp4::toPixel(kernFP) + trackingBetween(leftCp, rightCp, tracking);  // snap 4.4 fixed-point to nearest pixel
}

int GfxRenderer::getTextAdvanceX(const int fontId, const char* text, EpdFontFamily::Style style, const int8_t tracking,
                                 const BidiUtils::BidiBaseDir baseDir, const TextMeasureMode mode) const {
  // Match the font drawText would use for non-Latin strings (see resolveTextFontId).
  const int resolvedFontId = resolveTextFontId(fontId, text, style);
  // Measure the exact codepoint stream drawText renders: bidi-reordered and
  // Arabic-shaped (contextual presentation forms, Lam-Alef collapse).
  // Measuring the raw logical text counts the Alef a ligature absorbs and
  // uses base-letter advances instead of presentation-form advances, so RTL
  // lines come out wider than they draw — uneven word gaps and a ragged
  // right margin.
  std::string visual;
  text = resolveVisualText(text, visual, baseDir);

  // Advance table fast-path for SD card fonts during layout.
  // No kerning/ligature lookup — consistent with previous metadataOnly behavior
  // where kern/lig data was not loaded.
  auto sdIt = sdCardFonts_.find(resolvedFontId);
  if (mode == TextMeasureMode::Layout && sdIt != sdCardFonts_.end() && sdIt->second->hasAdvanceTable()) {
    int32_t widthFP = 0;
    int trackingPx = 0;
    uint32_t prevCp = 0;
    const bool isSupSub = (style & (EpdFontFamily::SUP | EpdFontFamily::SUB)) != 0;
    const uint8_t styleIdx = resolveSdCardStyle(*sdIt->second, style);
    const auto fontIt = fontMap.find(resolvedFontId);
    if (fontIt == fontMap.end()) {
      LOG_ERR("GFX", "Font %d not found", resolvedFontId);
      return 0;
    }
    const auto& font = fontIt->second;
    while (uint32_t cp = utf8NextCodepoint(reinterpret_cast<const uint8_t**>(&text))) {
      // RTL vowel marks (niqqud/harakat) are zero-advance overlays in drawText — no width.
      if (BidiUtils::isTransparentMark(cp)) {
        continue;
      }
      const uint32_t smallCapCp = (style & EpdFontFamily::SMALL_CAPS) ? utf8SmallCapsUpper(cp) : cp;
      const bool isSmallCap = smallCapCp != cp;
      cp = smallCapCp;
      int32_t advFP = sdIt->second->getAdvance(cp, styleIdx);
      if (!utf8IsCombiningMark(cp)) {
        if (advFP == 0 || (syntheticGlyph::isSolid(cp) && !font.hasCodepoint(cp, style))) {
          EpdGlyph solidFallback;
          const EpdGlyph* glyph = font.getGlyphMetrics(cp, solidFallback, style);
          advFP = glyph ? glyph->advanceX : 0;
        }
        trackingPx += trackingBetween(prevCp, cp, tracking);
        prevCp = cp;
      }
      widthFP += isSupSub ? (advFP + 1) / 2 : (isSmallCap ? scaleSmallCapAdvance(advFP) : advFP);
    }
    return fp4::toPixel(widthFP) + trackingPx;
  }

  const auto fontIt = fontMap.find(resolvedFontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", resolvedFontId);
    return 0;
  }

  uint32_t cp;
  uint32_t prevCp = 0;
  int widthPx = 0;
  int32_t prevAdvanceFP = 0;  // 12.4 fixed-point: prev glyph's advance + next kern for snap
  const auto& font = fontIt->second;
  while ((cp = utf8NextCodepoint(reinterpret_cast<const uint8_t**>(&text)))) {
    // RTL vowel marks (niqqud/harakat) are zero-advance overlays in drawText — no width.
    if (BidiUtils::isTransparentMark(cp)) {
      continue;
    }
    if (utf8IsCombiningMark(cp)) {
      continue;
    }
    const uint32_t smallCapCp = (style & EpdFontFamily::SMALL_CAPS) ? utf8SmallCapsUpper(cp) : cp;
    const bool isSmallCap = smallCapCp != cp;
    if (style & EpdFontFamily::SMALL_CAPS) {
      cp = smallCapCp;
    } else {
      cp = font.applyLigatures(cp, text, style);
    }

    // Differential rounding: snap (previous advance + current kern) together,
    // matching drawText so measurement and rendering agree exactly.
    if (prevCp != 0) {
      const auto kernFP = font.getKerning(prevCp, cp, style);  // 4.4 fixed-point kern
      widthPx += fp4::toPixel(prevAdvanceFP + kernFP) + trackingBetween(prevCp, cp, tracking);
    }

    EpdGlyph solidFallback;
    const EpdGlyph* glyph = font.getGlyphMetrics(cp, solidFallback, style);
    prevAdvanceFP = glyph ? glyph->advanceX : 0;
    if ((style & (EpdFontFamily::SUP | EpdFontFamily::SUB)) != 0) {
      prevAdvanceFP = (prevAdvanceFP + 1) / 2;
    } else if (isSmallCap) {
      prevAdvanceFP = scaleSmallCapAdvance(prevAdvanceFP);
    }
    prevCp = cp;
  }
  widthPx += fp4::toPixel(prevAdvanceFP);  // final glyph's advance
  return widthPx;
}

bool GfxRenderer::getCodepointMetrics(const int fontId, const uint32_t cp, const EpdFontFamily::Style style,
                                      int32_t& advanceFP, int& top) const {
  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) return false;
  const EpdGlyph* glyph = fontIt->second.getGlyph(cp, style);
  if (!glyph) return false;
  advanceFP = glyph->advanceX;
  top = glyph->top;
  return true;
}

int GfxRenderer::drawScaledCodepoint(const int fontId, const uint32_t cp, const EpdFontFamily::Style style, const int x,
                                     const int baselineY, const int scale256) const {
  if (scale256 <= 0) return 0;
  if (fontCacheManager_ && fontCacheManager_->isScanning()) {
    char utf8[5] = {};
    utf8AppendCodepoint(utf8, cp);
    fontCacheManager_->recordText(utf8, fontId, style);
    return 0;
  }
  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) return 0;
  const EpdFontFamily& font = fontIt->second;
  const EpdGlyph* glyph = font.getGlyph(cp, style);
  if (!glyph) return 0;
  const int advance = fp4::toPixel(static_cast<int32_t>(glyph->advanceX) * scale256 / 256);
  const EpdFontData* fontData = font.getData(style);
  const uint8_t* bitmap = getGlyphBitmap(fontData, glyph);
  if (!bitmap) return advance;

  const int srcW = glyph->width;
  const int srcH = glyph->height;
  const int left = x + glyph->left * scale256 / 256;
  const int top = baselineY - glyph->top * scale256 / 256;
  const auto c = canvasOf(*this);
  logOffPanel(bb_gfx_resampled_glyph(&c, bitmap, glyphBytes(srcW, srcH, fontData->is2Bit), srcW, srcH, fontData->is2Bit,
                                     scale256, left, top));
  return advance;
}

int GfxRenderer::getFontAscenderSize(const int fontId) const {
  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", fontId);
    return 0;
  }

  return fontIt->second.getData(EpdFontFamily::REGULAR)->ascender;
}

int GfxRenderer::getLineHeight(const int fontId) const {
  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", fontId);
    return 0;
  }

  return fontIt->second.getData(EpdFontFamily::REGULAR)->advanceY;
}

int GfxRenderer::getLineHeight(const int fontId, const float compression) const {
  return static_cast<int>(getLineHeight(fontId) * compression + 0.5f);
}

int GfxRenderer::getTextHeight(const int fontId) const {
  const auto fontIt = fontMap.find(fontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", fontId);
    return 0;
  }
  return fontIt->second.getData(EpdFontFamily::REGULAR)->ascender;
}

void GfxRenderer::drawTextRotated90CW(const int fontId, const int x, const int y, const char* text, const bool black,
                                      const EpdFontFamily::Style style) const {
  // Cannot draw a NULL / empty string
  if (text == nullptr || *text == '\0') {
    return;
  }

  // Route non-Latin strings to the fallback font (see resolveTextFontId).
  const int resolvedFontId = resolveTextFontId(fontId, text, style);
  // Redirected to the SD fallback: batch-load the string's glyphs so the draw
  // loop below doesn't fault them in one SD read at a time (#2725).
  if (resolvedFontId != fontId) {
    ensureSdGlyphsResident(resolvedFontId, text, style, false);
  }
  const auto fontIt = fontMap.find(resolvedFontId);
  if (fontIt == fontMap.end()) {
    LOG_ERR("GFX", "Font %d not found", resolvedFontId);
    return;
  }

  const auto& font = fontIt->second;

  int lastBaseY = y;
  int lastBaseLeft = 0;
  int lastBaseWidth = 0;
  int lastBaseTop = 0;
  int32_t prevAdvanceFP = 0;  // 12.4 fixed-point: prev glyph's advance + next kern for snap

  uint32_t cp;
  uint32_t prevCp = 0;
  while ((cp = utf8NextCodepoint(reinterpret_cast<const uint8_t**>(&text)))) {
    // RTL vowel marks (Hebrew niqqud, Arabic harakat) ride the combining-mark
    // path: zero-advance overlays on the preceding base glyph (applyBidiVisual
    // emits base-then-marks per UAX#9 L3). anchorFor pins position-sensitive
    // niqqud (dagesh, shin/sin dots, holam) to their spot on the base; other
    // marks stay centered, raised above the base or (kasra) at their
    // font-native position. Fonts without their glyphs — the built-ins — miss
    // the getGlyph lookup and skip them, as before.
    if (utf8IsCombiningMark(cp) || BidiUtils::isTransparentMark(cp)) {
      const EpdGlyph* combiningGlyph = font.getGlyph(cp, style);
      if (!combiningGlyph) continue;
      const auto anchor = combiningMark::anchorFor(cp);
      const int raiseBy =
          combiningMark::raiseAboveBase(anchor, combiningGlyph->top, combiningGlyph->height, lastBaseTop);
      const int combiningX = x - raiseBy;
      const int combiningY = combiningMark::anchorOverRotated90CW(anchor, lastBaseY, lastBaseLeft, lastBaseWidth,
                                                                  combiningGlyph->left, combiningGlyph->width);
      renderCharImpl<TextRotation::Rotated90CW>(*this, renderMode, font, cp, combiningX, combiningY, black, style);
      continue;
    }

    cp = font.applyLigatures(cp, text, style);

    // Differential rounding: snap (previous advance + current kern) as one unit,
    // subtracting for the rotated coordinate direction.
    if (prevCp != 0) {
      const auto kernFP = font.getKerning(prevCp, cp, style);  // 4.4 fixed-point kern
      lastBaseY -= fp4::toPixel(prevAdvanceFP + kernFP);       // snap 12.4 fixed-point to nearest pixel
    }

    EpdGlyph solidFallback;
    const EpdGlyph* glyph = font.getGlyphMetrics(cp, solidFallback, style);

    lastBaseLeft = glyph ? glyph->left : 0;
    lastBaseWidth = glyph ? glyph->width : 0;
    lastBaseTop = glyph ? glyph->top : 0;
    prevAdvanceFP = glyph ? glyph->advanceX : 0;  // 12.4 fixed-point

    renderCharImpl<TextRotation::Rotated90CW>(*this, renderMode, font, cp, x, lastBaseY, black, style);
    prevCp = cp;
  }
}

uint8_t* GfxRenderer::getFrameBuffer() const { return frameBuffer; }

size_t GfxRenderer::getBufferSize() const { return frameBufferSize; }

// unused
// void GfxRenderer::grayscaleRevert() const { display.grayscaleRevert(); }

void GfxRenderer::displayGrayscaleBase(HalDisplay::RefreshMode fallback) const {
  absoluteGrayPlanes = false;
  display.displayGrayscaleBase(fallback, fadingFix);
}

bool GfxRenderer::displayGrayscaleBase(HalDisplay::GrayscaleMode mode, HalDisplay::RefreshMode fallback) const {
  absoluteGrayPlanes = false;
  if (!display.displayGrayscaleBase(mode, fallback, fadingFix)) return false;
  absoluteGrayPlanes = mode != HalDisplay::GrayscaleMode::Overlay;
  return true;
}

void GfxRenderer::preconditionGrayscale() const { display.preconditionGrayscale(); }

void GfxRenderer::preconditionGrayscale(int x, int y, int w, int h) const {
  if (w <= 0 || h <= 0) return;
  // Rotate the logical rect's opposite corners to physical panel coords; the
  // physical bbox stays axis-aligned for all four orientations.
  int ax, ay, bx, by;
  rotateCoordinates(orientation, x, y, &ax, &ay, panelWidth, panelHeight);
  rotateCoordinates(orientation, x + w - 1, y + h - 1, &bx, &by, panelWidth, panelHeight);
  int x0 = ax < bx ? ax : bx, x1 = ax > bx ? ax : bx;
  int y0 = ay < by ? ay : by, y1 = ay > by ? ay : by;
  if (x0 < 0) x0 = 0;
  if (y0 < 0) y0 = 0;
  if (x1 >= panelWidth) x1 = panelWidth - 1;
  if (y1 >= panelHeight) y1 = panelHeight - 1;
  if (x1 < x0 || y1 < y0) return;
  display.preconditionGrayscale(static_cast<uint16_t>(x0), static_cast<uint16_t>(y0),
                                static_cast<uint16_t>(x1 - x0 + 1), static_cast<uint16_t>(y1 - y0 + 1));
}

void GfxRenderer::copyGrayscaleLsbBuffers() const { display.copyGrayscaleLsbBuffers(frameBuffer); }

void GfxRenderer::copyGrayscaleMsbBuffers() const { display.copyGrayscaleMsbBuffers(frameBuffer); }

void GfxRenderer::displayGrayBuffer() const {
  display.displayGrayBuffer(fadingFix);
  absoluteGrayPlanes = false;
}

void GfxRenderer::setRenderMode(RenderMode mode) {
  if (mode == BW && absoluteGrayPlanes) {
    display.cleanupGrayscaleBuffers(nullptr);  // cancel an unfinished absolute pass
    absoluteGrayPlanes = false;
  }
  renderMode = mode;
}

void GfxRenderer::writeGrayscalePlaneStrip(bool lsbPlane, const uint8_t* scratch, int yStart, int numRows) const {
  // Guard the uint16_t casts below: a negative would wrap to a huge length.
  assert(yStart >= 0 && numRows > 0 && yStart <= static_cast<int>(panelHeight) - numRows);
  display.writeGrayscalePlaneStrip(lsbPlane, scratch, static_cast<uint16_t>(yStart), static_cast<uint16_t>(numRows));
}

bool GfxRenderer::supportsStripGrayscale() const { return grayscaleCapabilities().stripUploads; }

bool GfxRenderer::combinesGrayscaleBase() const {
  return grayscaleCapabilities().base == HalDisplay::GrayscaleBase::Combined;
}

void GfxRenderer::freeBwBufferChunks() {
  for (auto& bwBufferChunk : bwBufferChunks) {
    if (bwBufferChunk) {
      free(bwBufferChunk);
      bwBufferChunk = nullptr;
    }
  }
}

/**
 * This should be called before grayscale buffers are populated.
 * A `restoreBwBuffer` call should always follow the grayscale render if this method was called.
 * Uses chunked allocation to avoid needing 48KB of contiguous memory.
 * Returns true if buffer was stored successfully, false if allocation failed.
 */
bool GfxRenderer::storeBwBuffer() {
  // Allocate and copy each chunk
  for (size_t i = 0; i < bwBufferChunks.size(); i++) {
    // Check if any chunks are already allocated
    if (bwBufferChunks[i]) {
      LOG_ERR("GFX", "!! BW buffer chunk %zu already stored - this is likely a bug, freeing chunk", i);
      free(bwBufferChunks[i]);
      bwBufferChunks[i] = nullptr;
    }

    const size_t offset = i * BW_BUFFER_CHUNK_SIZE;
    const size_t chunkSize = std::min(BW_BUFFER_CHUNK_SIZE, static_cast<size_t>(frameBufferSize - offset));
    bwBufferChunks[i] = static_cast<uint8_t*>(malloc(chunkSize));

    if (!bwBufferChunks[i]) {
      LOG_ERR("GFX", "!! Failed to allocate BW buffer chunk %zu (%zu bytes)", i, chunkSize);
      // Free previously allocated chunks
      freeBwBufferChunks();
      return false;
    }

    memcpy(bwBufferChunks[i], frameBuffer + offset, chunkSize);
  }

  LOG_DBG("GFX", "Stored BW buffer in %zu chunks (%zu bytes each)", bwBufferChunks.size(), BW_BUFFER_CHUNK_SIZE);
  return true;
}

/**
 * This can only be called if `storeBwBuffer` was called prior to the grayscale render.
 * It should be called to restore the BW buffer state after grayscale rendering is complete.
 * Uses chunked restoration to match chunked storage.
 */
void GfxRenderer::restoreBwBuffer(const bool resyncPanelBaseline) {
  // Check if all chunks are allocated
  bool missingChunks = false;
  for (const auto& bwBufferChunk : bwBufferChunks) {
    if (!bwBufferChunk) {
      missingChunks = true;
      break;
    }
  }

  if (missingChunks) {
    freeBwBufferChunks();
    return;
  }

  for (size_t i = 0; i < bwBufferChunks.size(); i++) {
    const size_t offset = i * BW_BUFFER_CHUNK_SIZE;
    const size_t chunkSize = std::min(BW_BUFFER_CHUNK_SIZE, static_cast<size_t>(frameBufferSize - offset));
    memcpy(frameBuffer + offset, bwBufferChunks[i], chunkSize);
  }

  if (resyncPanelBaseline) {
    display.cleanupGrayscaleBuffers(frameBuffer);
  }

  freeBwBufferChunks();
  LOG_DBG("GFX", "Restored and freed BW buffer chunks");
}

/**
 * Cleanup grayscale buffers using the current frame buffer.
 * Use this when BW buffer was re-rendered instead of stored/restored.
 */
void GfxRenderer::cleanupGrayscaleWithFrameBuffer() const {
  if (frameBuffer) {
    display.cleanupGrayscaleBuffers(frameBuffer);
  }
}

void GfxRenderer::getOrientedViewableTRBL(int* outTop, int* outRight, int* outBottom, int* outLeft) const {
  // Board truth: the bezel insets live in the active profile (panel-native
  // portrait frame); this only rotates them into the current orientation.
  const BoardConfig::ViewableInsets& vi = BoardConfig::ACTIVE.viewableInsets;
  switch (orientation) {
    case Portrait:
      *outTop = vi.top;
      *outRight = vi.right;
      *outBottom = vi.bottom;
      *outLeft = vi.left;
      break;
    case LandscapeClockwise:
      *outTop = vi.left;
      *outRight = vi.top;
      *outBottom = vi.right;
      *outLeft = vi.bottom;
      break;
    case PortraitInverted:
      *outTop = vi.bottom;
      *outRight = vi.left;
      *outBottom = vi.top;
      *outLeft = vi.right;
      break;
    case LandscapeCounterClockwise:
      *outTop = vi.right;
      *outRight = vi.bottom;
      *outBottom = vi.left;
      *outLeft = vi.top;
      break;
  }
}
