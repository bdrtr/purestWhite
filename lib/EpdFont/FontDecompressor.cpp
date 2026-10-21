#include "FontDecompressor.h"

#include <Arduino.h>
#include <FontAlloc.h>  // PSRAM-preferring font allocator (fiFontMalloc/Free)
#include <InflateStream.h>
#include <Logging.h>
#include <bembeyaz/compressed_font.h>

#include <algorithm>
#include <cstddef>

// Decompressed-glyph page slots and the hot-group buffers are placed in PSRAM
// when the board has it (fiFontMalloc), falling back to the internal heap
// otherwise — so the built-in compressed fonts get the same lift as the SD and
// vector paths. fiFontFree releases either region.

// The Rust side asserts the same layouts (rust/crates/bembeyaz-ffi/src/compressed.rs).
static_assert(sizeof(bb_compressed_group) == sizeof(EpdFontGroup) &&
                  offsetof(bb_compressed_group, compressed_size) == offsetof(EpdFontGroup, compressedSize) &&
                  offsetof(bb_compressed_group, uncompressed_size) == offsetof(EpdFontGroup, uncompressedSize) &&
                  offsetof(bb_compressed_group, glyph_count) == offsetof(EpdFontGroup, glyphCount) &&
                  offsetof(bb_compressed_group, first_glyph) == offsetof(EpdFontGroup, firstGlyphIndex) &&
                  sizeof(bb_compressed_entry) == 12 && offsetof(bb_compressed_entry, aligned_offset) == 8 &&
                  sizeof(bb_cpfont_glyph) == sizeof(EpdGlyph) &&
                  sizeof(bb_cpfont_interval) == sizeof(EpdUnicodeInterval) && sizeof(EpdLigaturePair) == 8 &&
                  sizeof(bb_compressed_font) == 9 * sizeof(void*),
              "bb_compressed layout");

namespace {
// bembeyaz-font's view of a compressed font. EpdFontData has no glyph count: fontconvert lays the
// glyphs out as the intervals cover them, and a font without intervals (the host tests') counts
// the glyphs its consecutive groups name.
bb_compressed_font viewOf(const EpdFontData* font) {
  size_t glyphs = 0;
  if (font->intervalCount > 0) {
    const EpdUnicodeInterval& last = font->intervals[font->intervalCount - 1];
    glyphs = last.offset + (last.last - last.first + 1);
  }
  if (!font->glyphToGroup) {
    for (uint16_t i = 0; i < font->groupCount; i++) {
      glyphs = std::max<size_t>(glyphs, font->groups[i].firstGlyphIndex + font->groups[i].glyphCount);
    }
  }
  return {reinterpret_cast<const bb_cpfont_glyph*>(font->glyph),
          glyphs,
          reinterpret_cast<const bb_cpfont_interval*>(font->intervals),
          font->intervalCount,
          reinterpret_cast<const bb_compressed_group*>(font->groups),
          font->groupCount,
          font->glyphToGroup,
          reinterpret_cast<const uint8_t*>(font->ligaturePairs),
          font->ligaturePairs ? font->ligaturePairCount : 0};
}
}  // namespace

FontDecompressor::~FontDecompressor() { deinit(); }

bool FontDecompressor::init() {
  clearCache();
  return true;
}

void FontDecompressor::deinit() {
  freePageBuffer();
  freeHotGroup();
}

void FontDecompressor::clearCache() {
  freePageBuffer();
  freeHotGroup();
}

void FontDecompressor::freePageBuffer() {
  for (uint8_t s = 0; s < pageSlotCount; s++) {
    fiFontFree(pageSlots[s].buffer);
    fiFontFree(pageSlots[s].glyphs);
    pageSlots[s] = {};
  }
  pageSlotCount = 0;
}

void FontDecompressor::freeHotGroup() {
  fiFontFree(hotGroup);
  hotGroup = nullptr;
  hotGroupCapacity = 0;
  hotGroupFont = nullptr;
  hotGroupIndex = UINT16_MAX;
  fiFontFree(hotGlyphBuf);
  hotGlyphBuf = nullptr;
  hotGlyphBufCapacity = 0;
}

bool FontDecompressor::ensureCapacity(uint8_t*& buf, uint32_t& capacity, uint32_t needed) {
  if (capacity >= needed) return true;
  // Grow-only, free-then-malloc: every caller fully rewrites the buffer after a grow, so the
  // old contents are dead -- freeing first gives the allocator its best shot on a tight heap.
  fiFontFree(buf);
  buf = static_cast<uint8_t*>(fiFontMalloc(needed));  // owned by FontDecompressor, freed in freeHotGroup()
  capacity = buf ? needed : 0;
  return buf != nullptr;
}

bool FontDecompressor::decompressGroup(const EpdFontData* fontData, uint16_t groupIndex, uint8_t* outBuf,
                                       uint32_t outSize) {
  const EpdFontGroup& group = fontData->groups[groupIndex];

  const uint32_t tDecomp = millis();
  // One-shot: outBuf holds the whole group, so no window. The decoder state lives only for this call.
  InflateStream inflate;
  if (!inflate.init(false)) {  // logs the allocation that failed
    stats.decompressTimeMs += millis() - tDecomp;
    return false;
  }
  inflate.setSource(&fontData->bitmap[group.compressedOffset], group.compressedSize);
  if (!inflate.read(outBuf, outSize)) {
    stats.decompressTimeMs += millis() - tDecomp;
    LOG_ERR("FDC", "Decompression failed for group %u", groupIndex);
    return false;
  }
  stats.decompressTimeMs += millis() - tDecomp;
  return true;
}

// --- getBitmap: page buffer → hot group → decompress ---

const uint8_t* FontDecompressor::getBitmap(const EpdFontData* fontData, const EpdGlyph* glyph, uint32_t glyphIndex) {
  const uint32_t tStart = micros();
  stats.getBitmapCalls++;

  if (!fontData->groups || fontData->groupCount == 0) {
    stats.getBitmapTimeUs += micros() - tStart;
    return &fontData->bitmap[glyph->dataOffset];
  }

  // Check page buffer slots (populated by prewarmCache — one slot per font style)
  for (uint8_t s = 0; s < pageSlotCount; s++) {
    const auto& slot = pageSlots[s];
    if (slot.fontData != fontData || slot.glyphCount == 0) continue;
    const uint32_t offset = bb_compressed_find_entry(slot.glyphs, slot.glyphCount, glyphIndex);
    if (offset != UINT32_MAX) {
      stats.cacheHits++;
      stats.getBitmapTimeUs += micros() - tStart;
      return &slot.buffer[offset];
    }
    break;  // Right slot, but the glyph isn't in it or wasn't extracted; don't check other slots
  }

  // Fallback: hot group slot
  const bb_compressed_font view = viewOf(fontData);
  const int32_t foundGroup = bb_compressed_group_of(&view, glyphIndex);
  if (foundGroup < 0) {
    LOG_ERR("FDC", "Glyph %u not found in any group", glyphIndex);
    stats.getBitmapTimeUs += micros() - tStart;
    return nullptr;
  }
  const auto groupIndex = static_cast<uint16_t>(foundGroup);

  // Check if hot group already has this group decompressed — if not, decompress it
  if (!(hotGroup != nullptr && hotGroupFont == fontData && hotGroupIndex == groupIndex)) {
    stats.cacheMisses++;
    const EpdFontGroup& group = fontData->groups[groupIndex];

    // ensureCapacity may free the buffer, so the cached-group identity dies with it either way.
    hotGroupFont = nullptr;
    hotGroupIndex = UINT16_MAX;
    if (!ensureCapacity(hotGroup, hotGroupCapacity, group.uncompressedSize)) {
      LOG_ERR("FDC", "Failed to allocate %u bytes for hot group %u", group.uncompressedSize, groupIndex);
      stats.getBitmapTimeUs += micros() - tStart;
      return nullptr;
    }

    if (!decompressGroup(fontData, groupIndex, hotGroup, group.uncompressedSize)) {
      stats.getBitmapTimeUs += micros() - tStart;
      return nullptr;
    }

    hotGroupFont = fontData;
    hotGroupIndex = groupIndex;
    stats.hotGroupBytes = group.uncompressedSize;
  } else {
    stats.cacheHits++;
  }

  // Compact just the requested glyph from byte-aligned data into scratch buffer
  if (!ensureCapacity(hotGlyphBuf, hotGlyphBufCapacity, glyph->dataLength)) {
    LOG_ERR("FDC", "Failed to allocate %u bytes for glyph scratch", (unsigned)glyph->dataLength);
    stats.getBitmapTimeUs += micros() - tStart;
    return nullptr;
  }

  bb_compressed_unpack(&view, groupIndex, glyphIndex, hotGroup, fontData->groups[groupIndex].uncompressedSize,
                       hotGlyphBuf, hotGlyphBufCapacity);
  stats.getBitmapTimeUs += micros() - tStart;
  return hotGlyphBuf;
}

// --- Prewarm: pre-decompress glyph bitmaps for a page of text ---

int FontDecompressor::prewarmCache(const EpdFontData* fontData, const char* utf8Text) {
  if (!fontData || !fontData->groups || !utf8Text) return 0;

  // Allocate the next available slot (caller must call freePageBuffer/clearCache to reset)
  if (pageSlotCount >= MAX_PAGE_SLOTS) {
    LOG_ERR("FDC", "All %u page buffer slots full, cannot prewarm fontData=%p", MAX_PAGE_SLOTS, (void*)fontData);
    return -1;
  }
  PageSlot& slot = pageSlots[pageSlotCount];

  // Step 1: the glyphs this page needs, ligature outputs included.
  const bb_compressed_font view = viewOf(fontData);
  uint32_t neededGlyphs[MAX_PAGE_GLYPHS];
  bool glyphsCapped = false;
  const auto glyphCount =
      static_cast<uint16_t>(bb_compressed_gather(&view, utf8Text, neededGlyphs, MAX_PAGE_GLYPHS, &glyphsCapped));
  if (glyphsCapped) {
    LOG_DBG("FDC", "Glyph cap (%u) reached during prewarm; excess glyphs will use hot-group fallback", MAX_PAGE_GLYPHS);
  }

  if (glyphCount == 0) return 0;

  // Step 2: the groups they need, and the bytes of their packed bitmaps.
  uint16_t neededGroups[BB_COMPRESSED_MAX_PAGE_GROUPS];
  uint32_t totalBytes = 0;
  bool groupsCapped = false;
  const auto groupCount = static_cast<uint8_t>(
      bb_compressed_plan(&view, neededGlyphs, glyphCount, neededGroups, &totalBytes, &groupsCapped));
  if (groupsCapped) {
    LOG_DBG("FDC", "Group cap (%u) reached during prewarm; some groups will use hot-group fallback",
            BB_COMPRESSED_MAX_PAGE_GROUPS);
  }

  stats.uniqueGroupsAccessed = groupCount;

  // Step 3: Allocate page buffer and lookup table for this slot
  slot.buffer = static_cast<uint8_t*>(fiFontMalloc(totalBytes));
  slot.glyphs = static_cast<bb_compressed_entry*>(fiFontMalloc(glyphCount * sizeof(bb_compressed_entry)));
  if (!slot.buffer || !slot.glyphs) {
    LOG_ERR("FDC", "Failed to allocate page buffer (%u bytes, %u glyphs)", totalBytes, glyphCount);
    fiFontFree(slot.buffer);
    fiFontFree(slot.glyphs);
    slot = {};
    return glyphCount;
  }
  stats.pageBufferBytes += totalBytes;
  stats.pageGlyphsBytes += glyphCount * sizeof(bb_compressed_entry);

  slot.fontData = fontData;
  slot.glyphCount = glyphCount;
  pageSlotCount++;

  // The lookup table, sorted by glyph for getBitmap(), with each glyph's offset in its group.
  bb_compressed_index(&view, neededGlyphs, glyphCount, neededGroups, groupCount, slot.glyphs);

  // Step 4: For each unique group, decompress to temp buffer and extract needed glyphs
  uint32_t writeOffset = 0;
  int missed = 0;

  for (uint8_t g = 0; g < groupCount; g++) {
    uint16_t groupIdx = neededGroups[g];
    const EpdFontGroup& group = fontData->groups[groupIdx];

    auto* tempBuf = static_cast<uint8_t*>(fiFontMalloc(group.uncompressedSize));
    if (!tempBuf) {
      LOG_ERR("FDC", "Failed to allocate temp buffer (%u bytes) for group %u", group.uncompressedSize, groupIdx);
      missed++;
      continue;
    }
    if (group.uncompressedSize > stats.peakTempBytes) {
      stats.peakTempBytes = group.uncompressedSize;
    }

    if (!decompressGroup(fontData, groupIdx, tempBuf, group.uncompressedSize)) {
      fiFontFree(tempBuf);
      missed++;
      continue;
    }

    // Extract needed glyphs directly from the byte-aligned temp buffer, compacting on the fly.
    writeOffset = bb_compressed_extract(&view, groupIdx, tempBuf, group.uncompressedSize, slot.glyphs, slot.glyphCount,
                                        slot.buffer, totalBytes, writeOffset);

    fiFontFree(tempBuf);
  }

  LOG_DBG("FDC", "Prewarm: %u glyphs in %u bytes from %u groups (%d missed)", glyphCount, writeOffset, groupCount,
          missed);

  return missed;
}

// --- Stats ---

void FontDecompressor::resetStats() { stats = Stats{}; }

void FontDecompressor::logStats(const char* label) {
  const uint32_t total = stats.cacheHits + stats.cacheMisses;
  LOG_DBG("FDC", "[%s] hits=%lu misses=%lu (%.1f%% hit rate)", label, stats.cacheHits, stats.cacheMisses,
          total > 0 ? 100.0f * stats.cacheHits / total : 0.0f);
  LOG_DBG("FDC", "[%s] decompress=%lums groups_accessed=%u", label, stats.decompressTimeMs, stats.uniqueGroupsAccessed);
  LOG_DBG("FDC", "[%s] mem: pageBuf=%lu pageGlyphs=%lu hotGroup=%lu peakTemp=%lu", label, stats.pageBufferBytes,
          stats.pageGlyphsBytes, stats.hotGroupBytes, stats.peakTempBytes);
  if (stats.getBitmapCalls > 0) {
    LOG_DBG("FDC", "[%s] getBitmap: %lu calls, %luus total, %luus/call avg", label, stats.getBitmapCalls,
            stats.getBitmapTimeUs, stats.getBitmapTimeUs / stats.getBitmapCalls);
  }
  resetStats();
}
