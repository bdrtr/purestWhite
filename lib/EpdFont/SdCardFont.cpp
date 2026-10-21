#include "SdCardFont.h"

#include <FontPsram.h>  // PSRAM-preferring resident buffers (font memory lift)
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <Utf8.h>
#include <bembeyaz/cpfont.h>

#include <algorithm>
#include <cstddef>
#include <cstring>
#include <memory>

#include "EpdFontFamily.h"

// Resident SD-font buffers (glyph/kern arenas, interval + advance tables, the
// overflow ring) are placed in PSRAM when the board has it — freeing scarce
// internal SRAM — via these helpers. On a no-PSRAM board they fall back to the
// same internal heap `new[]`/`delete[]` used before, so behavior (and the
// fragmentation-avoidance logic below) is unchanged there. Every buffer routed
// through psramNewArray MUST be released with psramDeleteArray.
using freeink::font::psramDeleteArray;
using freeink::font::psramNewArray;

static_assert(sizeof(EpdKernClassEntry) == 3,
              "EpdKernClassEntry must be 3 bytes to match bembeyaz-font's mini kern tables");
static_assert(sizeof(EpdLigaturePair) == 8, "EpdLigaturePair must be 8 bytes to match .cpfont file layout");
// Checked against rust/crates/bembeyaz-ffi/src/cpfont.rs too.
static_assert(sizeof(bb_cpfont_style) == 52 && offsetof(bb_cpfont_style, bitmap_offset) == 32 &&
                  offsetof(bb_cpfont_style, ascender) == 36 && offsetof(bb_cpfont_style, kern_right_entries) == 42 &&
                  offsetof(bb_cpfont_style, advance_y) == 44 && offsetof(bb_cpfont_style, present) == 48 &&
                  sizeof(bb_cpfont_header) == 220 && offsetof(bb_cpfont_header, content_hash) == 208 &&
                  offsetof(bb_cpfont_header, version) == 212 && offsetof(bb_cpfont_header, two_bit) == 214 &&
                  offsetof(bb_cpfont_header, style_count) == 215 && offsetof(bb_cpfont_header, failed_entry) == 216 &&
                  sizeof(bb_cpfont_interval_fault) == 12 && offsetof(bb_cpfont_interval_fault, span) == 8 &&
                  offsetof(bb_cpfont_interval_table, count) == sizeof(void*) &&
                  offsetof(bb_cpfont_interval_table, compact) == sizeof(void*) + 4 &&
                  offsetof(bb_cpfont_interval_table, shared) == sizeof(void*) + 5 &&
                  sizeof(bb_cpfont_interval_load) == 32 && offsetof(bb_cpfont_interval_load, bytes) == 12 &&
                  offsetof(bb_cpfont_interval_load, fault) == 16 && offsetof(bb_cpfont_interval_load, owner) == 28 &&
                  offsetof(bb_cpfont_interval_load, compact) == 29,
              "bb_cpfont layout");
static_assert(sizeof(bb_cpfont_glyph) == 16 && sizeof(EpdGlyph) == 16 &&
                  alignof(bb_cpfont_glyph) == alignof(EpdGlyph) &&
                  offsetof(bb_cpfont_glyph, advance_x) == offsetof(EpdGlyph, advanceX) &&
                  offsetof(bb_cpfont_glyph, left) == offsetof(EpdGlyph, left) &&
                  offsetof(bb_cpfont_glyph, top) == offsetof(EpdGlyph, top) &&
                  offsetof(bb_cpfont_glyph, data_length) == offsetof(EpdGlyph, dataLength) &&
                  offsetof(bb_cpfont_glyph, data_offset) == offsetof(EpdGlyph, dataOffset),
              "bb_cpfont_glyph layout");
static_assert(SdCardFont::MAX_STYLES == BB_CPFONT_MAX_STYLES, "bb_cpfont styles");
static_assert(CPFONT_VERSION == BB_CPFONT_FORMAT_VERSION, "bump bembeyaz-font's cpfont::VERSION too");

namespace {

// Walks a null-terminated UTF-8 string and appends each unique codepoint to
// codepoints[0..cpCount-1] via O(n²) dedup.  Returns true if the buffer
// reached maxCount (cap hit), false if all codepoints fit.
bool collectUniqueCodepoints(const char* text, uint32_t* codepoints, uint32_t& cpCount, uint32_t maxCount,
                             const bool includeSmallCaps) {
  const auto append = [&](const uint32_t cp) {
    if (std::find(codepoints, codepoints + cpCount, cp) == codepoints + cpCount) {
      if (cpCount >= maxCount) return true;
      codepoints[cpCount++] = cp;
    }
    return false;
  };
  const unsigned char* p = reinterpret_cast<const unsigned char*>(text);
  while (*p) {
    const uint32_t cp = utf8NextCodepoint(&p);
    if (cp == 0) break;
    if (append(cp) || (includeSmallCaps && append(utf8SmallCapsUpper(cp)))) return true;
  }
  return false;
}

// resetStyleMiniData retention bounds (see the PerStyle comment in the header).
constexpr size_t MINI_RETAIN_MIN_FREE_HEAP = 40 * 1024;
constexpr uint8_t MINI_UNDERUSE_RUNS_BEFORE_FREE = 3;
// Working headroom left outside the mini bitmap arena's single contiguous block.
constexpr uint32_t PREWARM_MAX_ALLOC_RESERVE = 4 * 1024;

// Keep-if-fits buffer reuse: only reallocate when the needed size exceeds the
// current capacity. Freeing + reallocating slightly different sizes every page
// turn punches non-coalescing holes in the heap (the freed block rarely fits the
// next page's need), eroding the largest contiguous block all session. With
// reuse, capacities converge on the book's max page after a few turns and page
// turns stop touching the allocator. Only three small instantiations exist
// (interval/glyph/byte arrays), so template bloat is negligible.
template <typename T, typename CapT>
bool ensureArrayCapacity(T*& buf, CapT& capacity, const uint32_t needed) {
  if (buf && capacity >= needed) return true;
  psramDeleteArray(buf);
  buf = psramNewArray<T>(needed > 0 ? needed : 1);
  capacity = buf ? static_cast<CapT>(needed) : 0;
  return buf != nullptr;
}

// The Rust readers' view of an open .cpfont file.
bb_cpfont_file cpfontFile(HalFile& file) {
  const auto read = [](void* f, uint8_t* buf, const size_t len) -> size_t {
    const int count = static_cast<HalFile*>(f)->read(buf, len);
    return count > 0 ? static_cast<size_t>(count) : 0;
  };
  const auto seek = [](void* f, const uint32_t position) { return static_cast<HalFile*>(f)->seekSet(position); };
  return {read, seek, &file};
}

}  // namespace

SdCardFont::~SdCardFont() { freeAll(); }

// --- Per-style free/cleanup ---

void SdCardFont::freeStyleMiniData(PerStyle& s) {
  psramDeleteArray(s.miniIntervals);
  s.miniIntervals = nullptr;
  psramDeleteArray(s.miniGlyphs);
  s.miniGlyphs = nullptr;
  psramDeleteArray(s.miniBitmap);
  s.miniBitmap = nullptr;
  s.miniIntervalCount = 0;
  s.miniGlyphCount = 0;
  s.miniIntervalCapacity = 0;
  s.miniGlyphCapacity = 0;
  s.miniBitmapCapacity = 0;
  s.miniBitmapUsed = 0;
  s.miniUnderuseRuns = 0;
  freeStyleMiniKern(s);
  memset(&s.miniData, 0, sizeof(s.miniData));
  s.epdFont.data = &s.stubData;
}

void SdCardFont::resetStyleMiniData(PerStyle& s) {
  // Retention is a bet that the next scope needs similar data. Don't hold it
  // when the heap is tight: the arenas are rebuildable for one page's worth of
  // allocations, and this floor keeps retained fonts out of the way of section
  // builds and the render path's own floors.
  if (ESP.getFreeHeap() < MINI_RETAIN_MIN_FREE_HEAP) {
    freeStyleMiniData(s);
    return;
  }
  // Data (intervals/glyphs/bitmaps/kern) deliberately survives the scope: the
  // next prewarm subset-checks against it, which is what lets the idle prewarm
  // of page N+1 serve the actual page turn with zero SD reads.
}

void SdCardFont::freeStyleLigatures(PerStyle& s) {
  // Both font views borrow the resident ligature table.
  s.stubData.ligaturePairs = nullptr;
  s.stubData.ligaturePairCount = 0;
  s.miniData.ligaturePairs = nullptr;
  s.miniData.ligaturePairCount = 0;
  psramDeleteArray(s.ligaturePairs);
  s.ligaturePairs = nullptr;
  s.ligaturesLoaded = false;
}

void SdCardFont::freeStyleMiniKern(PerStyle& s) {
  psramDeleteArray(s.miniKernLeftClasses);
  s.miniKernLeftClasses = nullptr;
  psramDeleteArray(s.miniKernRightClasses);
  s.miniKernRightClasses = nullptr;
  psramDeleteArray(s.miniKernMatrix);
  s.miniKernMatrix = nullptr;
  s.miniKernLeftEntryCount = 0;
  s.miniKernRightEntryCount = 0;
  s.miniKernLeftClassCount = 0;
  s.miniKernRightClassCount = 0;
  s.miniKernLeftCapacity = 0;
  s.miniKernRightCapacity = 0;
  s.miniKernMatrixCapacity = 0;
  s.miniKernBuilt = false;
}

void SdCardFont::freeStyleAll(PerStyle& s) {
  freeStyleMiniData(s);
  // An earlier style owns any shared interval table.
  if (!s.intervals.shared) psramDeleteArray(s.intervals.entries);
  s.intervals = {};
  freeStyleLigatures(s);
  psramDeleteArray(s.kernBlockIndex);
  s.kernBlockIndex = nullptr;
  s.kernBlockIndexReady = false;
  s.toc.present = false;
}

// --- Global free/cleanup ---

void SdCardFont::releaseResidentCaches() {
  clearOverflow();
  clearPersistentCache();
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    if (!styles_[i].toc.present) continue;
    freeStyleMiniData(styles_[i]);  // also frees mini kern and restores the stub EpdFontData
    freeStyleLigatures(styles_[i]);
    applyGlyphMissCallback(i);  // keep the on-demand miss path alive on the stub
  }
}

void SdCardFont::freeAll() {
  clearOverflow();
  clearPersistentCache();
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    freeStyleAll(styles_[i]);
  }
  styleCount_ = 0;
  contentHash_ = 0;
  twoBit_ = false;
  loaded_ = false;
}

void SdCardFont::clearOverflow() {
  for (uint32_t i = 0; i < overflowCount_; i++) {
    psramDeleteArray(overflow_[i].bitmap);
    overflow_[i].bitmap = nullptr;
    overflow_[i].codepoint = 0;
  }
  overflowCount_ = 0;
  overflowNext_ = 0;
}

// --- Per-style kern/ligature ---

void SdCardFont::applyKernLigaturePointers(PerStyle& s, EpdFontData& data) const {
  // Kern data uses the per-page mini tables (renumbered class IDs). The full
  // kern matrix is never resident — see PerStyle::miniKernMatrix comment.
  data.kernLeftClasses = s.miniKernLeftClasses;
  data.kernRightClasses = s.miniKernRightClasses;
  // Packed class maps and dense matrix, as stored in the .cpfont and mapped in place; the split
  // and sparse forms are built-in only. Set explicitly rather than relying on the caller's
  // initialisation: getKerning() picks the representation by which pointer is non-null.
  data.kernLeftCodepoints = nullptr;
  data.kernLeftClassIds = nullptr;
  data.kernRightCodepoints = nullptr;
  data.kernRightClassIds = nullptr;
  data.kernRowOffsets = nullptr;
  data.kernSparseCols = nullptr;
  data.kernSparseValues = nullptr;
  data.kernMatrix = s.miniKernMatrix;
  data.kernLeftEntryCount = s.miniKernLeftEntryCount;
  data.kernRightEntryCount = s.miniKernRightEntryCount;
  data.kernLeftClassCount = s.miniKernLeftClassCount;
  data.kernRightClassCount = s.miniKernRightClassCount;
  // Ligatures are small (typically < 1KB) so they stay resident.
  data.ligaturePairs = s.ligaturePairs;
  data.ligaturePairCount = s.toc.ligature_pairs;
}

bool SdCardFont::loadStyleLigatures(PerStyle& s) {
  if (s.ligaturesLoaded) return true;
  if (s.toc.ligature_pairs == 0) {
    s.ligaturesLoaded = true;
    return true;
  }

  HalFile file;
  if (!Storage.openFileForRead("SDCF", filePath_, file)) {
    LOG_ERR("SDCF", "Failed to open .cpfont for ligatures: %s", filePath_);
    return false;
  }
  s.ligaturePairs = psramNewArray<EpdLigaturePair>(s.toc.ligature_pairs);
  if (!s.ligaturePairs) {
    LOG_ERR("SDCF", "Failed to allocate ligature pairs");
    return false;
  }
  if (!file.seekSet(s.toc.ligatures_offset)) {
    LOG_ERR("SDCF", "Failed to seek to ligature data");
    freeStyleLigatures(s);
    return false;
  }
  size_t sz = s.toc.ligature_pairs * sizeof(EpdLigaturePair);
  if (file.read(reinterpret_cast<uint8_t*>(s.ligaturePairs), sz) != static_cast<int>(sz)) {
    LOG_ERR("SDCF", "Failed to read ligature pairs");
    freeStyleLigatures(s);
    return false;
  }

  s.ligaturesLoaded = true;

  // Make ligatures visible to the stub (used when no mini data built yet).
  // Kern stays nullptr on the stub — it is only wired in miniData via
  // applyKernLigaturePointers() after buildMiniKernMatrix() runs.
  s.stubData.ligaturePairs = s.ligaturePairs;
  s.stubData.ligaturePairCount = s.toc.ligature_pairs;

  LOG_DBG("SDCF", "Ligatures loaded: %u", s.toc.ligature_pairs);
  return true;
}

// --- Per-page mini kern matrix ---

static_assert(sizeof(bb_cpfont_mini_kern) == 12 && offsetof(bb_cpfont_mini_kern, left_entries) == 4 &&
                  offsetof(bb_cpfont_mini_kern, left_classes) == 8 && offsetof(bb_cpfont_mini_kern, failed_row) == 10,
              "bb_cpfont_mini_kern layout");

// Build a small per-page kern matrix containing ONLY the (leftClass, rightClass)
// pairs reachable from codepoints in the current text. Class IDs are renumbered
// to a dense 1..N range so the resulting matrix is usedLeft × usedRight (typical
// Latin page: ~25×25 bytes) instead of the font's full ~180×200 (~36KB).
// The class tables are streamed from SD rather than kept resident, so reading
// holds no per-style kern allocation between pages.
//
// Correctness: EpdFont::getKerning only touches `kernLeftClasses` /
// `kernRightClasses` / `kernMatrix` / the count fields — we swap all of them to
// the mini versions together in applyKernLigaturePointers, so a codepoint not
// on this page simply returns class 0 (no kerning), which was the pre-existing
// behavior for any codepoint outside the kern classes.
bool SdCardFont::buildMiniKernMatrix(PerStyle& s, const uint32_t* codepoints, uint32_t cpCount) {
  // No freeStyleMiniKern here: it zeroed the capacities, which forced the
  // ensureArrayCapacity calls below to reallocate every page and defeated the
  // buffer reuse. prewarmStyle is the only caller and the success path
  // overwrites the contents and all four counts, so keeping the buffers is
  // safe. A page with no applicable kern pairs gets zero counts (buffers kept),
  // so it kerns as none instead of through the previous page's tables.
  if (s.toc.kern_left_entries == 0 || s.toc.kern_right_entries == 0 || cpCount == 0) {
    s.miniKernLeftEntryCount = 0;
    s.miniKernRightEntryCount = 0;
    s.miniKernLeftClassCount = 0;
    s.miniKernRightClassCount = 0;
    return true;  // font has no kern classes — nothing to build
  }

  HalFile file;
  if (!Storage.openFileForRead("SDCF", filePath_, file)) {
    LOG_ERR("SDCF", "Failed to open .cpfont for mini kern: %s", filePath_);
    freeStyleMiniKern(s);
    return false;
  }

  // Freed on return.
  const size_t scratchBytes = bb_cpfont_mini_kern_scratch_len(&s.toc, cpCount);
  auto scratch = makeUniqueNoThrow<uint8_t[]>(scratchBytes);
  if (!scratch) {
    LOG_ERR("SDCF", "Failed to allocate mini kern scratch (%u bytes)", static_cast<unsigned>(scratchBytes));
    freeStyleMiniKern(s);
    return false;
  }

  // Reuse the mini buffers across pages when they fit: per-page sizes differ by a few entries, and
  // freeing and reallocating them every page fragments the heap.
  const auto reserve = [](void* context, const bb_cpfont_mini_kern* kern, uint8_t** left, uint8_t** right,
                          int8_t** matrix) {
    auto& style = *static_cast<PerStyle*>(context);
    if (!ensureArrayCapacity(style.miniKernLeftClasses, style.miniKernLeftCapacity, kern->left_entries) ||
        !ensureArrayCapacity(style.miniKernRightClasses, style.miniKernRightCapacity, kern->right_entries) ||
        !ensureArrayCapacity(style.miniKernMatrix, style.miniKernMatrixCapacity, kern->matrix_bytes)) {
      return false;
    }
    *left = reinterpret_cast<uint8_t*>(style.miniKernLeftClasses);
    *right = reinterpret_cast<uint8_t*>(style.miniKernRightClasses);
    *matrix = style.miniKernMatrix;
    return true;
  };
  bb_cpfont_file source = cpfontFile(file);
  bb_cpfont_mini_kern kern{};
  switch (bb_cpfont_build_mini_kern(&source, &s.toc, codepoints, cpCount, scratch.get(), scratchBytes, s.kernBlockIndex,
                                    &s.kernBlockIndexReady, reserve, &s, &kern)) {
    case 0:
      break;
    case BB_CPFONT_KERN_RESERVE:
      LOG_ERR("SDCF", "Failed to allocate mini kern (%u+%u+%u bytes)", kern.left_entries * 3u, kern.right_entries * 3u,
              kern.matrix_bytes);
      freeStyleMiniKern(s);
      return false;
    case BB_CPFONT_KERN_ROW_SEEK:
      LOG_ERR("SDCF", "Failed to seek to kern row %u", kern.failed_row);
      freeStyleMiniKern(s);
      return false;
    case BB_CPFONT_KERN_ROW_READ:
      LOG_ERR("SDCF", "Failed to read kern row %u", kern.failed_row);
      freeStyleMiniKern(s);
      return false;
    default:
      LOG_ERR("SDCF", "Failed to read kern classes");
      freeStyleMiniKern(s);
      return false;
  }

  s.miniKernLeftEntryCount = kern.left_entries;
  s.miniKernRightEntryCount = kern.right_entries;
  s.miniKernLeftClassCount = kern.left_classes;
  s.miniKernRightClassCount = kern.right_classes;
  if (kern.left_classes == 0) return true;  // no kern pairs applicable on this page

  LOG_DBG("SDCF", "Built mini kern: %u×%u matrix (%u bytes, full was %u×%u = %u bytes)", kern.left_classes,
          kern.right_classes, kern.matrix_bytes, s.toc.kern_left_classes, s.toc.kern_right_classes,
          static_cast<uint32_t>(s.toc.kern_left_classes) * s.toc.kern_right_classes);
  return true;
}

// --- Glyph miss callback ---

void SdCardFont::applyGlyphMissCallback(uint8_t styleIdx) {
  overflowCtx_[styleIdx].self = this;
  overflowCtx_[styleIdx].styleIdx = styleIdx;

  auto& s = styles_[styleIdx];
  s.stubData.glyphMissHandler = &SdCardFont::onGlyphMiss;
  s.stubData.glyphMissCtx = &overflowCtx_[styleIdx];
  s.stubData.coverageHandler = &SdCardFont::onCoverageQuery;
}

bool SdCardFont::onCoverageQuery(void* ctx, const uint32_t codepoint) {
  const auto* octx = static_cast<OverflowContext*>(ctx);
  const PerStyle& s = octx->self->styles_[octx->styleIdx];
  if (!s.intervals.entries) return false;  // coverage index freed/never loaded
  return octx->self->findGlobalGlyphIndex(s, codepoint) >= 0;
}

// --- Load ---

bool SdCardFont::load(const char* path) {
  freeAll();
  if (strlen(path) >= sizeof(filePath_)) {
    LOG_ERR("SDCF", "Path too long (%zu bytes, max %zu)", strlen(path), sizeof(filePath_) - 1);
    return false;
  }
  strncpy(filePath_, path, sizeof(filePath_) - 1);
  filePath_[sizeof(filePath_) - 1] = '\0';

  HalFile file;
  if (!Storage.openFileForRead("SDCF", path, file)) {
    LOG_ERR("SDCF", "Failed to open .cpfont: %s", path);
    return false;
  }

  bb_cpfont_file source = cpfontFile(file);
  bb_cpfont_header font{};
  switch (bb_cpfont_read_header(&font, &source)) {
    case 0:
      break;
    case BB_CPFONT_HEADER_ENDED:
      LOG_ERR("SDCF", "Failed to read header");
      return false;
    case BB_CPFONT_NOT_CPFONT:
      LOG_ERR("SDCF", "Invalid magic bytes");
      return false;
    case BB_CPFONT_WRONG_VERSION:
      LOG_ERR("SDCF", "Unsupported version: %u (expected %u)", font.version, CPFONT_VERSION);
      return false;
    case BB_CPFONT_BAD_STYLE_COUNT:
      LOG_ERR("SDCF", "Invalid style count: %u", font.style_count);
      return false;
    case BB_CPFONT_TOC_ENDED:
      LOG_ERR("SDCF", "Failed to read style TOC entry %u", font.failed_entry);
      return false;
    case BB_CPFONT_BAD_STYLE_ID:
      LOG_ERR("SDCF", "Invalid styleId %u in TOC", font.failed_entry);
      return false;
    case BB_CPFONT_BAD_COUNTS: {
      const auto& h = font.styles[font.failed_entry];
      LOG_ERR("SDCF", "Style %u: unreasonable counts (iv=%u, gl=%u, kL=%u, kR=%u)", font.failed_entry, h.interval_count,
              h.glyph_count, h.kern_left_entries, h.kern_right_entries);
      return false;
    }
    default:
      return false;
  }

  for (uint8_t i = 0; i < MAX_STYLES; i++) styles_[i].toc = font.styles[i];
  twoBit_ = font.two_bit;
  styleCount_ = font.style_count;
  contentHash_ = font.content_hash;

  // Per present style: its interval table (see PerStyle::intervals), then its kern block index.
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    auto& s = styles_[i];
    if (!s.toc.present) continue;

    // Validated before any later code (findGlobalGlyphIndex, glyph reads) trusts it: a malformed
    // file could otherwise drive out-of-range glyph indices into bogus on-disk reads.
    const bb_cpfont_interval_table* earlier[MAX_STYLES] = {};
    for (uint8_t k = 0; k < i; k++) {
      if (styles_[k].toc.present) earlier[k] = &styles_[k].intervals;
    }
    const auto reserve = [](void*, const size_t bytes) { return psramNewArray<uint8_t>(bytes); };
    bb_cpfont_interval_load loaded{};
    switch (bb_cpfont_load_intervals(&source, &s.toc, earlier, i, &s.intervals, reserve, nullptr, &loaded)) {
      case 0:
        break;
      case BB_CPFONT_INTERVALS_SEEK:
        LOG_ERR("SDCF", "Failed to seek to intervals for style %u", i);
        freeAll();
        return false;
      case BB_CPFONT_INTERVALS_READ:
        LOG_ERR("SDCF", "Failed to read interval %u for style %u", loaded.record, i);
        freeAll();
        return false;
      case BB_CPFONT_INTERVALS_INVALID:
        if (loaded.fault.reversed) {
          LOG_ERR("SDCF", "Style %u: invalid interval %u (first 0x%lX > last 0x%lX)", i, loaded.record,
                  static_cast<unsigned long>(loaded.first), static_cast<unsigned long>(loaded.last));
        } else {
          LOG_ERR("SDCF", "Style %u: invalid interval layout at %u (overlap=%d span=%u offMis=%d offOver=%d)", i,
                  loaded.record, loaded.fault.overlaps, loaded.fault.span, loaded.fault.offset_mismatch,
                  loaded.fault.offset_overruns);
        }
        file.close();
        freeAll();
        return false;
      case BB_CPFONT_INTERVALS_SEEK_BACK:
        LOG_ERR("SDCF", "Failed to seek back to intervals for style %u", i);
        freeAll();
        return false;
      case BB_CPFONT_INTERVALS_RESERVE:
        if (loaded.compact) {
          LOG_ERR("SDCF", "Failed to allocate compact intervals for style %u", i);
        } else {
          LOG_ERR("SDCF", "Failed to allocate %u intervals for style %u", s.toc.interval_count, i);
        }
        freeAll();
        return false;
      case BB_CPFONT_INTERVALS_COMPACT_READ:
        LOG_ERR("SDCF", "Failed to read compact interval %u for style %u", loaded.record, i);
        freeAll();
        return false;
      case BB_CPFONT_INTERVALS_FULL_READ:
        LOG_ERR("SDCF", "Failed to read intervals for style %u", i);
        freeAll();
        return false;
      default:
        freeAll();
        return false;
    }
    if (s.intervals.shared) {
      LOG_DBG("SDCF", "Style %u: sharing style %u's %u-interval table (%u B not allocated)", i, loaded.owner,
              s.toc.interval_count, loaded.bytes);
    }

    if (s.toc.kern_left_entries > 0 && s.toc.kern_right_entries > 0) {
      // Optional: without it, every mini kern build reads the whole class tables.
      s.kernBlockIndex = psramNewArray<uint16_t>(bb_cpfont_kern_index_len(&s.toc));
      if (!s.kernBlockIndex) LOG_DBG("SDCF", "No kern block index for style %u; reading whole class tables", i);
    }

    // Initialize stub data
    memset(&s.stubData, 0, sizeof(s.stubData));
    s.stubData.advanceY = s.toc.advance_y;
    s.stubData.ascender = s.toc.ascender;
    s.stubData.descender = s.toc.descender;
    s.stubData.is2Bit = twoBit_;

    s.epdFont.data = &s.stubData;
    applyGlyphMissCallback(i);
  }

  loaded_ = true;

  LOG_DBG("SDCF", "Loaded: %s (v%u, %u styles)", path, CPFONT_VERSION, styleCount_);
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    if (!styles_[i].toc.present) continue;
    const auto& h = styles_[i].toc;
    LOG_DBG("SDCF", "  style[%u]: %u intervals, %u glyphs, advY=%u, asc=%d, desc=%d, kernL=%u, kernR=%u, ligs=%u", i,
            h.interval_count, h.glyph_count, h.advance_y, h.ascender, h.descender, h.kern_left_entries,
            h.kern_right_entries, h.ligature_pairs);
  }
  return true;
}

// --- Codepoint lookup ---

int32_t SdCardFont::findGlobalGlyphIndex(const PerStyle& s, uint32_t codepoint) const {
  return bb_cpfont_find_glyph(&s.intervals, codepoint);
}

// --- Prewarm ---

namespace {
const char* singleTextGetter(const void* ctx, uint32_t) { return static_cast<const char*>(ctx); }
}  // namespace

int SdCardFont::prewarm(const char* utf8Text, uint8_t styleMask, bool metadataOnly, bool loadKernLig, bool accumulate) {
  return prewarm(&singleTextGetter, utf8Text, 1, styleMask, metadataOnly, loadKernLig, accumulate);
}

int SdCardFont::prewarm(TextGetter getter, const void* ctx, uint32_t textCount, uint8_t styleMask, bool metadataOnly,
                        bool loadKernLig, bool accumulate) {
  if (!loaded_ || getter == nullptr) return -1;
  styleMask = resolveStyleMask(styleMask);
  if (styleMask == 0) return 0;

  unsigned long startMs = millis();

  // Cap the unique-codepoint budget by what the heap can actually hold as a
  // full mini arena (glyph structs + bitmaps, working headroom left over).
  // Multi-string batches only: a several-hundred-chapter CJK table of
  // contents would otherwise extract up to MAX_PAGE_GLYPHS and fail the whole
  // arena allocation — better to load the first screens' worth and let
  // scrolling union-in the rest page by page. Per-string requests are small
  // and already bounded by the union gate in prewarmStyle (running the check
  // there would also log per draw call); metadata-only prewarms load no
  // bitmaps. Bytes/glyph prefers the measured average from the resident mini:
  // Hangul ink boxes run well under the advanceY-squared em estimate, which
  // otherwise roughly halves the usable budget.
  uint32_t cpBudget = MAX_PAGE_GLYPHS;
  if (!metadataOnly && textCount > 1) {
    uint8_t refStyle = MAX_STYLES;
    for (uint8_t si = 0; si < MAX_STYLES && refStyle == MAX_STYLES; si++) {
      if ((styleMask & (1 << si)) && styles_[si].toc.present) refStyle = si;
    }
    if (refStyle < MAX_STYLES) {
      const auto& s = styles_[refStyle];
      const uint32_t bpp = twoBit_ ? 2 : 1;
      uint32_t bitmapPerGlyph = (static_cast<uint32_t>(s.toc.advance_y) * s.toc.advance_y * bpp) / 8 + 4;
      if (s.miniGlyphCount > 0 && s.miniBitmapUsed > 0) {
        bitmapPerGlyph = s.miniBitmapUsed / s.miniGlyphCount;
      }
      const uint32_t perGlyph = bitmapPerGlyph + sizeof(EpdGlyph);
      constexpr uint32_t PREWARM_HEAP_HEADROOM = 16 * 1024;
      const uint32_t freeHeap = ESP.getFreeHeap();
      const uint32_t budgetBytes = freeHeap > PREWARM_HEAP_HEADROOM ? freeHeap - PREWARM_HEAP_HEADROOM : 0;
      const uint32_t budgetGlyphs = budgetBytes / (perGlyph > 0 ? perGlyph : 1);
      if (budgetGlyphs < cpBudget) {
        cpBudget = budgetGlyphs;
      }
    }
  }
  if (cpBudget == 0) return -1;

  // Step 1: Extract unique codepoints from the UTF-8 texts (shared across all styles).
  // Dedup uses O(n^2) linear scan — worst case is MAX_PAGE_GLYPHS (512) unique codepoints
  // = ~131K comparisons, but in practice pages contain far fewer unique codepoints so the
  // actual cost is much lower. This is dwarfed by SD I/O that follows. Alternatives (hash
  // set, bitmap) exceed the 256-byte stack limit or add template bloat.
  // Heap-allocated: MAX_PAGE_GLYPHS * 4 = 2048 bytes, too large for stack (limit < 256 bytes)
  std::unique_ptr<uint32_t[]> codepoints(new (std::nothrow) uint32_t[MAX_PAGE_GLYPHS]);
  if (!codepoints) {
    LOG_ERR("SDCF", "Failed to allocate codepoint buffer (%u bytes)", MAX_PAGE_GLYPHS * 4);
    return -1;
  }
  uint32_t cpCount = 0;

  for (uint32_t ti = 0; ti < textCount && cpCount < cpBudget; ti++) {
    const char* text = getter(ctx, ti);
    if (text == nullptr) continue;
    const unsigned char* p = reinterpret_cast<const unsigned char*>(text);
    while (*p && cpCount < cpBudget) {
      uint32_t cp = utf8NextCodepoint(&p);
      if (cp == 0) break;

      bool found = false;
      for (uint32_t i = 0; i < cpCount; i++) {
        if (codepoints[i] == cp) {
          found = true;
          break;
        }
      }
      if (!found) {
        codepoints[cpCount++] = cp;
      }
    }
  }

  // Always include the replacement character
  {
    bool hasReplacement = false;
    for (uint32_t i = 0; i < cpCount; i++) {
      if (codepoints[i] == REPLACEMENT_GLYPH) {
        hasReplacement = true;
        break;
      }
    }
    if (!hasReplacement && cpCount < MAX_PAGE_GLYPHS) {
      codepoints[cpCount++] = REPLACEMENT_GLYPH;
    }
  }

  // Add ligature output codepoints from all styles being prewarmed.
  // Skip during metadata-only prewarm (layout measurement): ligatures are
  // loaded per style in prewarmStyle() during the full render prewarm.
  if (!metadataOnly && loadKernLig) {
    for (uint8_t si = 0; si < MAX_STYLES; si++) {
      if (!(styleMask & (1 << si)) || !styles_[si].toc.present) continue;
      auto& s = styles_[si];

      loadStyleLigatures(s);
      if (s.ligaturePairs && s.toc.ligature_pairs > 0) {
        for (uint8_t li = 0; li < s.toc.ligature_pairs && cpCount < MAX_PAGE_GLYPHS; li++) {
          uint32_t leftCp = s.ligaturePairs[li].pair >> 16;
          uint32_t rightCp = s.ligaturePairs[li].pair & 0xFFFF;
          uint32_t outCp = s.ligaturePairs[li].ligatureCp;

          bool hasLeft = false, hasRight = false;
          for (uint32_t i = 0; i < cpCount; i++) {
            if (codepoints[i] == leftCp) hasLeft = true;
            if (codepoints[i] == rightCp) hasRight = true;
            if (hasLeft && hasRight) break;
          }
          if (!hasLeft || !hasRight) continue;

          bool hasOut = false;
          for (uint32_t i = 0; i < cpCount; i++) {
            if (codepoints[i] == outCp) {
              hasOut = true;
              break;
            }
          }
          if (!hasOut) {
            codepoints[cpCount++] = outCp;
          }
        }
      }
    }
  }

  // Sort codepoints for ordered interval building
  std::sort(codepoints.get(), codepoints.get() + cpCount);

  // Prewarm each requested style
  int totalMissed = 0;
  for (uint8_t si = 0; si < MAX_STYLES; si++) {
    if (!(styleMask & (1 << si)) || !styles_[si].toc.present) continue;
    int missedForStyle = prewarmStyle(si, codepoints.get(), cpCount, metadataOnly, loadKernLig, accumulate);
    if (missedForStyle == PREWARM_ARENA_TOO_LARGE) {
      // The arena is one contiguous block, so a fragmented heap can fail it with
      // ample free bytes. Retry with the estimated largest prefix, backing off
      // if variable-size glyphs made that estimate too large.
      const uint32_t perGlyph = styles_[si].measuredBytesPerGlyph > 0 ? styles_[si].measuredBytesPerGlyph : 1;
      const uint32_t maxAlloc = ESP.getMaxAllocHeap();
      const uint32_t arenaBytes = maxAlloc > PREWARM_MAX_ALLOC_RESERVE ? maxAlloc - PREWARM_MAX_ALLOC_RESERVE : 0;
      uint32_t fit = arenaBytes / perGlyph;
      if (fit > cpCount) fit = cpCount;
      while (fit > 0) {
        LOG_DBG("SDCF", "Arena retry: %u -> %u glyphs (%uB/glyph, maxAlloc=%u)", cpCount, fit, perGlyph, maxAlloc);
        missedForStyle = prewarmStyle(si, codepoints.get(), fit, metadataOnly, loadKernLig, accumulate);
        if (missedForStyle != PREWARM_ARENA_TOO_LARGE) break;
        fit /= 2;
      }
      if (missedForStyle == PREWARM_ARENA_TOO_LARGE) {
        missedForStyle = static_cast<int>(cpCount);  // nothing resident
      } else {
        missedForStyle += static_cast<int>(cpCount - fit);  // the dropped suffix is absent too
      }
    }
    totalMissed += missedForStyle;
  }

  stats_.prewarmTotalMs = millis() - startMs;
  return totalMissed;
}

int SdCardFont::prewarmStyle(uint8_t styleIdx, const uint32_t* codepoints, uint32_t cpCount, bool metadataOnly,
                             bool loadKernLig, bool accumulate) {
  auto& s = styles_[styleIdx];

  // Idle-prewarm hit: mini data persists across PrewarmScopes (resetStyleMiniData
  // keeps it), so when the previous scope -- typically the idle prewarm of this
  // exact page -- already loaded every requested codepoint the font covers, this
  // page needs zero SD reads. A mini built metadata-only cannot serve a full
  // request (no bitmaps). Any uncovered codepoint falls through to the rebuild.
  if (s.miniGlyphCount > 0 && !(s.miniMetadataOnly && !metadataOnly)) {
    bool covered = true;
    int missedInMini = 0;
    for (uint32_t i = 0; i < cpCount && covered; i++) {
      const uint32_t cp = codepoints[i];
      bool inMini = false;
      for (uint32_t iv = 0; iv < s.miniIntervalCount; iv++) {
        if (cp < s.miniIntervals[iv].first) break;  // intervals sorted ascending
        if (cp <= s.miniIntervals[iv].last) {
          inMini = true;
          break;
        }
      }
      if (inMini) continue;
      if (findGlobalGlyphIndex(s, cp) < 0) {
        missedInMini++;  // not in font coverage: the rebuild couldn't load it either
      } else {
        covered = false;
      }
    }
    if (covered) {
      // A kern-wanting request (reader path) can subset-hit a mini that a
      // kern-free UI prewarm built: top up the kern matrix and ligatures without
      // re-reading any glyphs. The matrix covers every resident glyph so later
      // subset hits can skip it; a request-only matrix leaves them to rebuild.
      if (!metadataOnly && loadKernLig && !s.miniKernBuilt) {
        auto resident = makeUniqueNoThrow<uint32_t[]>(s.miniGlyphCount);
        uint32_t residentCount = 0;
        if (resident) {
          for (uint32_t iv = 0; iv < s.miniIntervalCount; iv++) {
            for (uint32_t cp = s.miniIntervals[iv].first;
                 cp <= s.miniIntervals[iv].last && residentCount < s.miniGlyphCount; cp++) {
              resident[residentCount++] = cp;
            }
          }
        } else {
          LOG_ERR("SDCF", "OOM: resident kern codepoints (%u) for style %u", s.miniGlyphCount, styleIdx);
        }
        const uint32_t* kernCps = resident ? resident.get() : codepoints;
        const uint32_t kernCount = resident ? residentCount : cpCount;
        if (loadStyleLigatures(s)) {
          s.miniKernBuilt = buildMiniKernMatrix(s, kernCps, kernCount) && resident != nullptr;
          applyKernLigaturePointers(s, s.miniData);
        }
      }
      return missedInMini;
    }
  }

  // Trim oversized buffers only on a cache miss. Trimming when a scope closes
  // would discard a freshly prefetched page before its actual draw. Count each
  // rebuild once, retaining the arena until several pages use less than 3/4.
  if (s.miniHysteresisPending && s.miniBitmapCapacity > 0 && s.miniBitmapUsed > 0) {
    s.miniHysteresisPending = false;
    if (s.miniBitmapUsed < s.miniBitmapCapacity - s.miniBitmapCapacity / 4) {
      if (++s.miniUnderuseRuns >= MINI_UNDERUSE_RUNS_BEFORE_FREE) {
        LOG_DBG("SDCF", "mini release (underuse): used=%u cap=%u", s.miniBitmapUsed, s.miniBitmapCapacity);
        freeStyleMiniData(s);
      }
    } else {
      s.miniUnderuseRuns = 0;
    }
  }

  // Incremental callers merge the resident mini's codepoints so the rebuild below
  // accumulates instead of replacing. Screens draw several distinct fallback
  // strings per refresh (file browser rows, chapter lists, the reader status
  // bar after the page scope); replacing meant every string evicted every
  // other string's glyphs, so each measure/draw re-hit the SD forever. With
  // the union, residency converges after one pass and redraws are RAM-only.
  // Over MAX_PAGE_GLYPHS the union is abandoned (request-only rebuild), which
  // bounds mini RAM to the same worst case as a single dense page.
  std::unique_ptr<uint32_t[]> unionCps;
  if (accumulate && s.miniGlyphCount > 0 && s.miniIntervalCount > 0 && ESP.getFreeHeap() < MINI_RETAIN_MIN_FREE_HEAP) {
    // Heap-tight (e.g. a chapter list stacked over an open book). Size-aware:
    // a small union (a UI screen's worth of titles, a few KB) is exactly what
    // stops per-string eviction from re-reading the SD on every repaint, so
    // allow it as long as the estimated arena leaves headroom. Only when the
    // union would crowd the remaining heap (page-scale arenas) drop the
    // retained data and rebuild request-only below: bounded like the
    // pre-merge behavior, and the freed arena gives the small alloc room.
    const uint32_t unionMaxCount = s.miniGlyphCount + cpCount;  // pre-dedup upper bound
    const uint32_t avgBitmapBytes =
        (s.miniBitmapUsed > 0 && s.miniGlyphCount > 0) ? s.miniBitmapUsed / s.miniGlyphCount : 64;
    const uint32_t estArenaBytes = unionMaxCount * (static_cast<uint32_t>(sizeof(EpdGlyph)) + avgBitmapBytes);
    constexpr uint32_t UNION_PRESSURE_HEADROOM = 12 * 1024;
    if (estArenaBytes + UNION_PRESSURE_HEADROOM > ESP.getFreeHeap()) {
      freeStyleMiniData(s);
    }
  }
  if (accumulate && s.miniGlyphCount > 0 && s.miniIntervalCount > 0) {
    const uint32_t unionMax = s.miniGlyphCount + cpCount;
    unionCps.reset(new (std::nothrow) uint32_t[unionMax]);
    if (unionCps) {
      // Two-way sorted merge: resident stream walks the mini intervals
      // (ascending), request stream is the caller's sorted codepoint array.
      uint32_t n = 0;
      uint32_t ivIdx = 0;
      uint32_t ivCp = s.miniIntervals[0].first;
      bool ivActive = true;
      uint32_t ri = 0;
      while ((ivActive || ri < cpCount) && n < unionMax) {
        uint32_t next;
        if (ivActive && (ri >= cpCount || ivCp <= codepoints[ri])) {
          next = ivCp;
          if (ri < cpCount && codepoints[ri] == ivCp) ri++;
          if (ivCp < s.miniIntervals[ivIdx].last) {
            ivCp++;
          } else if (++ivIdx < s.miniIntervalCount) {
            ivCp = s.miniIntervals[ivIdx].first;
          } else {
            ivActive = false;
          }
        } else {
          next = codepoints[ri++];
        }
        unionCps[n++] = next;
      }
      if (!ivActive && ri >= cpCount && n <= MAX_PAGE_GLYPHS) {
        // A full mini must stay full: a metadata-only request may not drop
        // bitmaps other strings are still rendering from.
        metadataOnly = metadataOnly && s.miniMetadataOnly;
        codepoints = unionCps.get();
        cpCount = n;
      } else {
        unionCps.reset();
      }
    }
  }

  // Map codepoints to global glyph indices for this style
  struct CpGlyphMapping {
    uint32_t codepoint;
    int32_t globalIndex;
  };
  auto mappings = makeUniqueNoThrow<CpGlyphMapping[]>(cpCount);
  if (!mappings) {
    LOG_ERR("SDCF", "Failed to allocate mapping array for style %u", styleIdx);
    return static_cast<int>(cpCount);
  }

  uint32_t validCount = 0;
  for (uint32_t i = 0; i < cpCount; i++) {
    int32_t idx = findGlobalGlyphIndex(s, codepoints[i]);
    if (idx >= 0) {
      mappings[validCount].codepoint = codepoints[i];
      mappings[validCount].globalIndex = idx;
      validCount++;
    }
  }
  int missed = static_cast<int>(cpCount - validCount);

  if (validCount == 0) {
    freeStyleMiniData(s);
    s.epdFont.data = &s.stubData;
    return missed;
  }

  // Build mini intervals from sorted codepoints. Reset counts and fall back to the
  // stub until the rebuild completes, but KEEP the existing buffers (keep-if-fits
  // reuse) — the free-and-realloc-per-page pattern here was a primary fragmenter.
  s.miniIntervalCount = 0;
  s.miniGlyphCount = 0;
  s.miniKernLeftEntryCount = 0;
  s.miniKernRightEntryCount = 0;
  s.miniKernLeftClassCount = 0;
  s.miniKernRightClassCount = 0;
  memset(&s.miniData, 0, sizeof(s.miniData));
  s.epdFont.data = &s.stubData;

  if (!ensureArrayCapacity(s.miniIntervals, s.miniIntervalCapacity, validCount)) {
    LOG_ERR("SDCF", "Failed to allocate mini intervals for style %u", styleIdx);
    return static_cast<int>(cpCount);
  }

  s.miniIntervalCount = 0;
  uint32_t rangeStart = 0;
  for (uint32_t i = 1; i <= validCount; i++) {
    if (i == validCount || mappings[i].codepoint != mappings[i - 1].codepoint + 1) {
      s.miniIntervals[s.miniIntervalCount].first = mappings[rangeStart].codepoint;
      s.miniIntervals[s.miniIntervalCount].last = mappings[i - 1].codepoint;
      s.miniIntervals[s.miniIntervalCount].offset = rangeStart;
      s.miniIntervalCount++;
      rangeStart = i;
    }
  }

  // Mini glyph array (reused across pages when it fits)
  if (!ensureArrayCapacity(s.miniGlyphs, s.miniGlyphCapacity, validCount)) {
    LOG_ERR("SDCF", "Failed to allocate mini glyphs for style %u", styleIdx);
    freeStyleMiniData(s);
    return static_cast<int>(cpCount);
  }
  s.miniGlyphCount = validCount;

  // Build sorted read order for sequential I/O
  auto readOrder = makeUniqueNoThrow<uint32_t[]>(validCount);
  if (!readOrder) {
    LOG_ERR("SDCF", "Failed to allocate read order for style %u", styleIdx);
    freeStyleMiniData(s);
    return static_cast<int>(cpCount);
  }
  for (uint32_t i = 0; i < validCount; i++) readOrder[i] = i;
  std::sort(readOrder.get(), readOrder.get() + validCount,
            [&](uint32_t a, uint32_t b) { return mappings[a].globalIndex < mappings[b].globalIndex; });

  HalFile file;
  if (!Storage.openFileForRead("SDCF", filePath_, file)) {
    LOG_ERR("SDCF", "Failed to reopen .cpfont for prewarm (style %u)", styleIdx);
    freeStyleMiniData(s);
    return static_cast<int>(cpCount);
  }

  unsigned long sdStart = millis();
  uint32_t seekCount = 0;

  // Glyph records in glyph index order.
  bb_cpfont_file source = cpfontFile(file);
  struct GlyphReads {
    const CpGlyphMapping* mappings;
    const uint32_t* order;
    EpdGlyph* glyphs;
  } reads{mappings.get(), readOrder.get(), s.miniGlyphs};
  uint32_t glyphSeeks = 0;
  uint32_t failedGlyph = 0;
  const int glyphsRead = bb_cpfont_read_glyphs(
      &source, &s.toc, twoBit_, validCount,
      [](void* context, const size_t i) {
        const auto* r = static_cast<GlyphReads*>(context);
        return static_cast<uint32_t>(r->mappings[r->order[i]].globalIndex);
      },
      [](void* context, const size_t i, const bb_cpfont_glyph* glyph) {
        const auto* r = static_cast<GlyphReads*>(context);
        memcpy(&r->glyphs[r->order[i]], glyph, sizeof(EpdGlyph));
      },
      &reads, &glyphSeeks, &failedGlyph);
  seekCount += glyphSeeks;
  if (glyphsRead == BB_CPFONT_GLYPH_SEEK) {
    LOG_ERR("SDCF", "Prewarm: failed to seek to glyph %d (style %u)", static_cast<int>(failedGlyph), styleIdx);
    file.close();
    freeStyleMiniData(s);
    return static_cast<int>(cpCount);
  }
  if (glyphsRead != 0) {
    LOG_ERR("SDCF", "Prewarm: short glyph read (style %u, glyph %d)", styleIdx, static_cast<int>(failedGlyph));
    freeStyleMiniData(s);
    return static_cast<int>(cpCount);
  }

  // Mapping is no longer needed once glyph metadata has been read.
  mappings.reset();
  uint32_t totalBitmapSize = 0;

  if (!metadataOnly) {
    // Compute total bitmap size
    for (uint32_t i = 0; i < validCount; i++) {
      totalBitmapSize += s.miniGlyphs[i].dataLength;
    }

    // Rounded up: the retry multiplies this back out to size a glyph set, and a
    // floored figure can yield an arena that still does not fit.
    if (validCount > 0) s.measuredBytesPerGlyph = (totalBitmapSize + validCount - 1) / validCount;

    // Release temporary allocations before replacing the large bitmap so their
    // holes can coalesce with the old arena. Recreate only the small read order.
    if (totalBitmapSize > s.miniBitmapCapacity) {
      readOrder.reset();
      psramDeleteArray(s.miniBitmap);
      s.miniBitmap = nullptr;
      s.miniBitmapCapacity = 0;
      if (ESP.getMaxAllocHeap() < totalBitmapSize) {
        // Growing metadata can split the old bitmap's free region. Reserve the
        // bitmap before rebuilding that metadata; the reserved capacity makes
        // this a single extra pass, with no recursive growth on the next pass.
        freeStyleMiniData(s);
        file = HalFile{};
        bool bitmapReady = ensureArrayCapacity(s.miniBitmap, s.miniBitmapCapacity, totalBitmapSize);
        if (!bitmapReady && hasAdvanceTable()) {
          // Layout advances are rebuildable; release them before falling back to fewer glyphs.
          const uint32_t largestBefore = ESP.getMaxAllocHeap();
          clearPersistentCache();
          LOG_DBG("SDCF", "Retrying bitmap after advance-cache release: maxAlloc=%u -> %u", largestBefore,
                  ESP.getMaxAllocHeap());
          bitmapReady = ensureArrayCapacity(s.miniBitmap, s.miniBitmapCapacity, totalBitmapSize);
        }
        if (!bitmapReady) {
          LOG_ERR("SDCF", "Failed to reserve mini bitmap (%u bytes) for style %u", totalBitmapSize, styleIdx);
          return PREWARM_ARENA_TOO_LARGE;
        }
        LOG_DBG("SDCF", "Reserved bitmap before metadata rebuild (%u bytes)", totalBitmapSize);
        stats_.sdReadTimeMs += millis() - sdStart;
        stats_.seekCount += seekCount;
        return prewarmStyle(styleIdx, codepoints, cpCount, metadataOnly, loadKernLig, accumulate);
      }
    }
    if (!ensureArrayCapacity(s.miniBitmap, s.miniBitmapCapacity, totalBitmapSize)) {
      LOG_ERR("SDCF", "Failed to allocate mini bitmap (%u bytes) for style %u", totalBitmapSize, styleIdx);
      freeStyleMiniData(s);
      return PREWARM_ARENA_TOO_LARGE;
    }
    s.miniBitmapUsed = totalBitmapSize;  // underuse-hysteresis signal for the next cache miss

    if (!readOrder) {
      readOrder = makeUniqueNoThrow<uint32_t[]>(validCount);
      if (!readOrder) {
        LOG_ERR("SDCF", "Failed to allocate bitmap read order for style %u", styleIdx);
        freeStyleMiniData(s);
        return static_cast<int>(cpCount);
      }
      for (uint32_t i = 0; i < validCount; i++) readOrder[i] = i;
    }

    // Read bitmap data sorted by file offset
    std::sort(readOrder.get(), readOrder.get() + validCount,
              [&](uint32_t a, uint32_t b) { return s.miniGlyphs[a].dataOffset < s.miniGlyphs[b].dataOffset; });

    uint32_t bitmapSeeks = 0;
    const int bitmapsRead =
        bb_cpfont_read_bitmaps(&source, &s.toc, reinterpret_cast<bb_cpfont_glyph*>(s.miniGlyphs), validCount,
                               readOrder.get(), validCount, s.miniBitmap, totalBitmapSize, &bitmapSeeks);
    seekCount += bitmapSeeks;
    if (bitmapsRead == BB_CPFONT_BITMAP_SEEK) {
      LOG_ERR("SDCF", "Prewarm: failed to seek to bitmap (style %u)", styleIdx);
      file.close();
      freeStyleMiniData(s);
      return static_cast<int>(cpCount);
    }
    if (bitmapsRead != 0) {
      LOG_ERR("SDCF", "Prewarm: short bitmap read (style %u)", styleIdx);
      freeStyleMiniData(s);
      return static_cast<int>(cpCount);
    }
  }

  uint32_t sdTime = millis() - sdStart;
  readOrder.reset();

  // Full render prewarm: load the ligatures (one-time per style) and then build
  // the per-page mini kern matrix restricted to class pairs reachable from this
  // page's codepoints. Skip during metadata-only prewarm — layout only needs
  // advanceX and the mini kern would be thrown away before rendering.
  bool ligaturesOk = false;
  bool kernOk = false;
  if (!metadataOnly && loadKernLig) {
    ligaturesOk = loadStyleLigatures(s);
    if (ligaturesOk) kernOk = buildMiniKernMatrix(s, codepoints, cpCount);
  }

  // Populate miniData and swap
  s.miniMetadataOnly = metadataOnly;
  s.miniKernBuilt = kernOk;
  s.miniHysteresisPending = !metadataOnly;  // one hysteresis evaluation per rebuild
  memset(&s.miniData, 0, sizeof(s.miniData));
  s.miniData.bitmap = s.miniBitmap;
  s.miniData.glyph = s.miniGlyphs;
  s.miniData.intervals = s.miniIntervals;
  s.miniData.intervalCount = s.miniIntervalCount;
  s.miniData.advanceY = s.toc.advance_y;
  s.miniData.ascender = s.toc.ascender;
  s.miniData.descender = s.toc.descender;
  s.miniData.is2Bit = twoBit_;
  if (ligaturesOk) {
    // A failed kern build leaves the mini kern empty, so the page keeps its ligatures and kerns as none.
    applyKernLigaturePointers(s, s.miniData);
  }
  s.miniData.glyphMissHandler = &SdCardFont::onGlyphMiss;
  s.miniData.glyphMissCtx = &overflowCtx_[styleIdx];
  s.miniData.coverageHandler = &SdCardFont::onCoverageQuery;

  s.epdFont.data = &s.miniData;

  // Accumulate stats
  stats_.sdReadTimeMs += sdTime;
  stats_.seekCount += seekCount;
  stats_.uniqueGlyphs += validCount;
  stats_.bitmapBytes += totalBitmapSize;

  return missed;
}

// --- Cache management ---

void SdCardFont::clearCache() {
  clearOverflow();
  // Note: advance table is intentionally preserved here. It persists across
  // layout passes so repeated section indexing amortizes SD reads. Use
  // clearPersistentCache() to wipe it.
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    if (!styles_[i].toc.present) continue;
    resetStyleMiniData(styles_[i]);
    applyGlyphMissCallback(i);
  }
}

// --- Advance table ---

void SdCardFont::clearPersistentCache() {
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    psramDeleteArray(advanceTable_[i]);
    advanceTable_[i] = nullptr;
    advanceTableSize_[i] = 0;
    advanceTableCapacity_[i] = 0;
  }
}

bool SdCardFont::advanceTableLookup(uint8_t styleIdx, uint32_t codepoint, uint16_t* outAdvance) const {
  const AdvanceEntry* table = advanceTable_[styleIdx];
  const uint32_t size = advanceTableSize_[styleIdx];
  if (!table || size == 0) return false;
  uint32_t lo = 0, hi = size;
  while (lo < hi) {
    uint32_t mid = lo + (hi - lo) / 2;
    if (table[mid].codepoint < codepoint) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  if (lo < size && table[lo].codepoint == codepoint) {
    if (outAdvance) *outAdvance = table[lo].advanceX;
    return true;
  }
  return false;
}

bool SdCardFont::growAdvanceTable(const uint8_t styleIdx, const uint32_t needed) {
  if (needed <= advanceTableCapacity_[styleIdx]) return true;
  constexpr uint32_t MIN_ADVANCE_CAPACITY = 64;
  uint32_t capacity = std::max({needed, advanceTableCapacity_[styleIdx] * 2, MIN_ADVANCE_CAPACITY});
  capacity = std::min(capacity, ADVANCE_CACHE_LIMIT);
  AdvanceEntry* grown = psramNewArray<AdvanceEntry>(capacity);
  if (!grown && capacity > needed) {
    capacity = needed;
    grown = psramNewArray<AdvanceEntry>(capacity);
  }
  if (!grown) return false;
  if (advanceTableSize_[styleIdx] > 0) {
    memcpy(grown, advanceTable_[styleIdx], advanceTableSize_[styleIdx] * sizeof(AdvanceEntry));
  }
  psramDeleteArray(advanceTable_[styleIdx]);
  advanceTable_[styleIdx] = grown;
  advanceTableCapacity_[styleIdx] = capacity;
  return true;
}

void SdCardFont::mergeIntoAdvanceTable(uint8_t styleIdx, const AdvanceEntry* sortedNew, uint32_t newCount) {
  if (newCount == 0) return;
  const uint32_t oldSize = advanceTableSize_[styleIdx];
  if (oldSize >= ADVANCE_CACHE_LIMIT) return;  // already full

  // Cap the merged size at ADVANCE_CACHE_LIMIT. Anything past the cap is
  // dropped from the tail of the sorted merge — a deterministic, bounded loss
  // that doesn't bias which codepoints get cached on subsequent passes.
  uint32_t mergedCap = oldSize + newCount;
  if (mergedCap > ADVANCE_CACHE_LIMIT) mergedCap = ADVANCE_CACHE_LIMIT;

  if (!growAdvanceTable(styleIdx, mergedCap)) {
    LOG_ERR("SDCF", "mergeIntoAdvanceTable: alloc failed (%u entries) style %u", mergedCap, styleIdx);
    return;
  }

  // Merge from the back so existing entries move at most once and are never
  // overwritten before they are read. The largest `drop` entries fall past the cap.
  AdvanceEntry* table = advanceTable_[styleIdx];
  uint32_t drop = oldSize + newCount - mergedCap;
  uint32_t i = oldSize, j = newCount, k = oldSize + newCount;
  while (j > 0) {
    const bool takeNew = i == 0 || sortedNew[j - 1].codepoint >= table[i - 1].codepoint;
    const AdvanceEntry entry = takeNew ? sortedNew[--j] : table[--i];
    --k;
    if (drop > 0) {
      --drop;
    } else {
      table[k] = entry;
    }
  }
  advanceTableSize_[styleIdx] = mergedCap;
}

bool SdCardFont::hasAdvanceTable() const {
  for (uint8_t i = 0; i < MAX_STYLES; i++) {
    if (advanceTableSize_[i] > 0) return true;  // a pre-grown table can be allocated but empty
  }
  return false;
}

uint16_t SdCardFont::getAdvance(uint32_t codepoint, uint8_t style) const {
  style &= (MAX_STYLES - 1);
  if (!advanceTable_[style]) return 0;
  const AdvanceEntry* table = advanceTable_[style];
  const uint32_t size = advanceTableSize_[style];
  // Binary search sorted by codepoint
  uint32_t lo = 0, hi = size;
  while (lo < hi) {
    uint32_t mid = lo + (hi - lo) / 2;
    if (table[mid].codepoint < codepoint) {
      lo = mid + 1;
    } else {
      hi = mid;
    }
  }
  if (lo < size && table[lo].codepoint == codepoint) {
    return table[lo].advanceX;
  }
  return 0;
}

// Given a sorted array of unique codepoints, resolve glyph indices per style,
// batch-read advanceX from SD, and merge into the persistent advance table.
// Caller owns the codepoints buffer.
int SdCardFont::fetchAdvancesForCodepoints(uint32_t* codepoints, uint32_t cpCount, uint8_t styleMask) {
  int totalMissed = 0;
  for (uint8_t si = 0; si < MAX_STYLES; si++) {
    if (!(styleMask & (1 << si)) || !styles_[si].toc.present) continue;
    const auto& s = styles_[si];

    // Stop fetching once the cache is full — further inserts would be dropped
    // by the merge anyway. The renderer fast path tolerates missing entries
    // (returns 0); the slow path is still correct for those codepoints.
    if (advanceTableSize_[si] >= ADVANCE_CACHE_LIMIT) continue;
    if (std::all_of(codepoints, codepoints + cpCount,
                    [&](const uint32_t cp) { return advanceTableLookup(si, cp, nullptr); })) {
      continue;
    }

    // For each codepoint in `codepoints`, skip those already cached, then
    // resolve to a glyph index. Build a parallel array sorted by glyph index
    // for sequential SD reads.
    struct CpIdx {
      uint32_t codepoint;
      int32_t glyphIndex;
    };
    std::unique_ptr<CpIdx[]> mappings(new (std::nothrow) CpIdx[cpCount]);
    if (!mappings) {
      LOG_ERR("SDCF", "buildAdvanceTable: failed to allocate mappings for style %u", si);
      totalMissed += cpCount;
      continue;
    }

    uint32_t needCount = 0;
    uint32_t missedThisStyle = 0;
    const int32_t replacementIdx = findGlobalGlyphIndex(s, REPLACEMENT_GLYPH);
    for (uint32_t i = 0; i < cpCount; i++) {
      const uint32_t cp = codepoints[i];
      if (advanceTableLookup(si, cp, nullptr)) continue;  // already cached
      int32_t idx = findGlobalGlyphIndex(s, cp);
      if (idx < 0) {
        if (replacementIdx < 0) {
          missedThisStyle++;
          continue;
        }
        idx = replacementIdx;
      }
      mappings[needCount].codepoint = cp;
      mappings[needCount].glyphIndex = idx;
      needCount++;
    }
    totalMissed += static_cast<int>(missedThisStyle);

    if (needCount == 0) continue;

    // Sort by glyph index so SD reads are mostly sequential.
    std::sort(mappings.get(), mappings.get() + needCount,
              [](const CpIdx& a, const CpIdx& b) { return a.glyphIndex < b.glyphIndex; });

    // Open file once and read advanceX for each needed glyph.
    HalFile file;
    if (!Storage.openFileForRead("SDCF", filePath_, file)) {
      LOG_ERR("SDCF", "buildAdvanceTable: failed to open .cpfont for style %u", si);
      continue;
    }

    std::unique_ptr<AdvanceEntry[]> staged(new (std::nothrow) AdvanceEntry[needCount]);
    if (!staged) {
      LOG_ERR("SDCF", "buildAdvanceTable: failed to allocate staging for style %u", si);
      file.close();
      continue;
    }

    struct AdvanceReads {
      const CpIdx* mappings;
      AdvanceEntry* staged;
      uint32_t fetched;
    } reads{mappings.get(), staged.get(), 0};
    bb_cpfont_file source = cpfontFile(file);
    uint32_t failedGlyph = 0;
    const int glyphsRead = bb_cpfont_read_glyphs(
        &source, &s.toc, twoBit_, needCount,
        [](void* context, const size_t i) {
          return static_cast<uint32_t>(static_cast<AdvanceReads*>(context)->mappings[i].glyphIndex);
        },
        [](void* context, const size_t i, const bb_cpfont_glyph* glyph) {
          auto* r = static_cast<AdvanceReads*>(context);
          r->staged[i].codepoint = r->mappings[i].codepoint;
          r->staged[i].advanceX = glyph->advance_x;
          r->fetched = i + 1;
        },
        &reads, nullptr, &failedGlyph);
    if (glyphsRead == BB_CPFONT_GLYPH_SEEK) {
      LOG_ERR("SDCF", "buildAdvanceTable: failed to seek to glyph %d (style %u)", static_cast<int>(failedGlyph), si);
    } else if (glyphsRead != 0) {
      LOG_ERR("SDCF", "buildAdvanceTable: short glyph read (style %u, glyph %d)", si, static_cast<int>(failedGlyph));
    }
    const uint32_t fetched = reads.fetched;
    file.close();

    if (fetched > 0) {
      // Sort staged by codepoint, then merge into the persistent table.
      std::sort(staged.get(), staged.get() + fetched,
                [](const AdvanceEntry& a, const AdvanceEntry& b) { return a.codepoint < b.codepoint; });
      mergeIntoAdvanceTable(si, staged.get(), fetched);
    }

    LOG_DBG("SDCF", "Advance table style %u: +%u from SD, total=%u/%u", si, fetched, advanceTableSize_[si],
            ADVANCE_CACHE_LIMIT);
  }

  return totalMissed;
}

int SdCardFont::buildAdvanceTablePacked(const char* const* segments, const size_t* segmentLens,
                                        const size_t segmentCount, const bool includeSpace, const bool includeHyphen,
                                        uint8_t styleMask, const char* extraText, const bool includeSmallCaps) {
  if (!loaded_) return -1;
  styleMask = resolveStyleMask(styleMask);
  if (styleMask == 0) return 0;

  unsigned long startMs = millis();

  // Grow nearly full tables now rather than during the merge: the merge runs
  // while the codepoint scratch below is live, and a table grown then lands
  // above it, inside the hole the scratch leaves when it is freed.
  constexpr uint32_t ADVANCE_GROWTH_HEADROOM = 64;
  for (uint8_t si = 0; si < MAX_STYLES; si++) {
    if (!(styleMask & (1 << si)) || !styles_[si].toc.present) continue;
    const uint32_t size = advanceTableSize_[si];
    if (size < ADVANCE_CACHE_LIMIT && advanceTableCapacity_[si] - size < ADVANCE_GROWTH_HEADROOM) {
      if (!growAdvanceTable(si, std::min(size + ADVANCE_GROWTH_HEADROOM, ADVANCE_CACHE_LIMIT))) {
        LOG_DBG("SDCF", "Advance table pre-grow failed for style %u; the merge grows it", si);
      }
    }
  }

  static constexpr uint32_t MAX_UNIQUE_CODEPOINTS = 4096;
  size_t maxCodepoints = extraText ? std::min<size_t>(strlen(extraText), MAX_UNIQUE_CODEPOINTS) : 0;
  for (size_t seg = 0; seg < segmentCount; ++seg) {
    maxCodepoints = std::min<size_t>(MAX_UNIQUE_CODEPOINTS,
                                     maxCodepoints + std::min<size_t>(segmentLens[seg], MAX_UNIQUE_CODEPOINTS));
  }
  if (includeSmallCaps) maxCodepoints = std::min<size_t>(MAX_UNIQUE_CODEPOINTS, maxCodepoints * 2);
  // Byte counts bound codepoints. Short table words need no heap scratch; paragraphs keep the existing cap.
  static constexpr size_t INLINE_CODEPOINTS = 32;
  uint32_t inlineCodepoints[INLINE_CODEPOINTS + 2];  // space and hyphen can be injected below
  std::unique_ptr<uint32_t[]> heapCodepoints;
  uint32_t* codepoints = inlineCodepoints;
  if (maxCodepoints > INLINE_CODEPOINTS) {
    heapCodepoints = makeUniqueNoThrow<uint32_t[]>(maxCodepoints + 2);
    if (!heapCodepoints) {
      LOG_ERR("SDCF", "buildAdvanceTable: failed to allocate codepoint buffer (%u bytes)",
              static_cast<unsigned>((maxCodepoints + 2) * sizeof(uint32_t)));
      return -1;
    }
    codepoints = heapCodepoints.get();
  }
  uint32_t cpCount = 0;
  bool hitCap = false;

  // Each segment holds consecutive NUL-terminated words; walk word by word.
  for (size_t seg = 0; seg < segmentCount && !hitCap; ++seg) {
    const char* p = segments[seg];
    const char* const end = p + segmentLens[seg];
    while (p < end && !hitCap) {
      hitCap = collectUniqueCodepoints(p, codepoints, cpCount, maxCodepoints, includeSmallCaps);
      p += strlen(p) + 1;
    }
  }
  if (extraText && !hitCap) {
    hitCap = collectUniqueCodepoints(extraText, codepoints, cpCount, maxCodepoints, includeSmallCaps);
  }

  if (includeSpace && std::none_of(codepoints, codepoints + cpCount, [](uint32_t c) { return c == ' '; }))
    codepoints[cpCount++] = ' ';
  if (includeHyphen && std::none_of(codepoints, codepoints + cpCount, [](uint32_t c) { return c == '-'; }))
    codepoints[cpCount++] = '-';

  if (hitCap) {
    LOG_ERR("SDCF", "buildAdvanceTable: unique codepoint cap (%u) hit, layout may be approximate",
            MAX_UNIQUE_CODEPOINTS);
  }
  std::sort(codepoints, codepoints + cpCount);
  int totalMissed = fetchAdvancesForCodepoints(codepoints, cpCount, styleMask);
  stats_.prewarmTotalMs = millis() - startMs;
  return totalMissed;
}

int SdCardFont::buildAdvanceTable(const char* utf8Text, uint8_t styleMask, const char* extraText) {
  const size_t len = strlen(utf8Text);
  return buildAdvanceTablePacked(&utf8Text, &len, 1, false, false, styleMask, extraText);
}

// --- Stats ---

void SdCardFont::logStats(const char* label) {
  LOG_DBG("SDCF", "[%s] total=%ums sd_read=%ums seeks=%u glyphs=%u bitmap=%u bytes", label, stats_.prewarmTotalMs,
          stats_.sdReadTimeMs, stats_.seekCount, stats_.uniqueGlyphs, stats_.bitmapBytes);
}

void SdCardFont::resetStats() { stats_ = Stats{}; }

// --- Public accessors ---

EpdFont* SdCardFont::getEpdFont(uint8_t style) {
  style &= (MAX_STYLES - 1);
  if (!styles_[style].toc.present) return nullptr;
  return &styles_[style].epdFont;
}

bool SdCardFont::hasStyle(uint8_t style) const { return styles_[style & (MAX_STYLES - 1)].toc.present; }

uint8_t SdCardFont::resolveStyle(uint8_t style) const {
  static const uint8_t kFallbacks[MAX_STYLES][MAX_STYLES] = {
      // REGULAR: REGULAR -> BOLD -> ITALIC -> BOLD_ITALIC
      {EpdFontFamily::REGULAR, EpdFontFamily::BOLD, EpdFontFamily::ITALIC, EpdFontFamily::BOLD_ITALIC},
      // BOLD: BOLD -> REGULAR -> BOLD_ITALIC -> ITALIC
      {EpdFontFamily::BOLD, EpdFontFamily::REGULAR, EpdFontFamily::BOLD_ITALIC, EpdFontFamily::ITALIC},
      // ITALIC: ITALIC -> REGULAR -> BOLD_ITALIC -> BOLD
      {EpdFontFamily::ITALIC, EpdFontFamily::REGULAR, EpdFontFamily::BOLD_ITALIC, EpdFontFamily::BOLD},
      // BOLD_ITALIC: BOLD_ITALIC -> BOLD -> ITALIC -> REGULAR
      {EpdFontFamily::BOLD_ITALIC, EpdFontFamily::BOLD, EpdFontFamily::ITALIC, EpdFontFamily::REGULAR},
  };

  const uint8_t styleBits = style & (MAX_STYLES - 1);
  for (uint8_t candidate : kFallbacks[styleBits]) {
    if (styles_[candidate].toc.present) return candidate;
  }
  return EpdFontFamily::REGULAR;
}

uint8_t SdCardFont::resolveStyleMask(uint8_t styleMask) const {
  uint8_t resolvedMask = 0;
  for (uint8_t si = 0; si < MAX_STYLES; si++) {
    if (styleMask & (1 << si)) {
      resolvedMask |= static_cast<uint8_t>(1u << resolveStyle(si));
    }
  }
  return resolvedMask;
}

// --- On-demand glyph loading (overflow buffer) ---

const EpdGlyph* SdCardFont::onGlyphMiss(void* ctx, uint32_t codepoint) {
  auto* oc = static_cast<OverflowContext*>(ctx);
  auto* self = oc->self;
  uint8_t styleIdx = oc->styleIdx;

  if (!self->loaded_ || styleIdx >= MAX_STYLES || !self->styles_[styleIdx].toc.present) return nullptr;
  const auto& s = self->styles_[styleIdx];
  if (!s.intervals.entries) return nullptr;

  // Check overflow cache first (matching both codepoint and style)
  for (uint32_t i = 0; i < self->overflowCount_; i++) {
    if (self->overflow_[i].codepoint == codepoint && self->overflow_[i].styleIdx == styleIdx) {
      return &self->overflow_[i].glyph;
    }
  }

  // Look up global glyph index via full intervals
  int32_t globalIdx = self->findGlobalGlyphIndex(s, codepoint);
  if (globalIdx < 0) return nullptr;

  // Pick overflow slot (ring buffer). Read into temporaries first so the
  // existing slot stays valid if SD I/O fails. Bookkeeping (count/next)
  // is deferred until after all I/O succeeds to avoid inconsistent state.
  uint32_t slot = self->overflowNext_;
  bool wasAtCapacity = (self->overflowCount_ == OVERFLOW_CAPACITY);

  // Read glyph metadata into temporary
  HalFile file;
  if (!Storage.openFileForRead("SDCF", self->filePath_, file)) {
    LOG_ERR("SDCF", "Overflow: failed to open .cpfont");
    return nullptr;
  }

  EpdGlyph tempGlyph = {};
  bb_cpfont_file source = cpfontFile(file);
  const int glyphRead = bb_cpfont_read_glyph(&source, &s.toc, self->twoBit_, static_cast<uint32_t>(globalIdx),
                                             reinterpret_cast<bb_cpfont_glyph*>(&tempGlyph));
  if (glyphRead == BB_CPFONT_GLYPH_SEEK) {
    LOG_ERR("SDCF", "Overflow: failed to seek to glyph for U+%04X style %u", codepoint, styleIdx);
    file.close();
    return nullptr;
  }
  if (glyphRead != 0) {
    LOG_ERR("SDCF", "Overflow: failed to read glyph metadata for U+%04X style %u", codepoint, styleIdx);
    return nullptr;
  }

  // Read bitmap data into temporary (if any)
  uint8_t* tempBitmap = nullptr;
  if (tempGlyph.dataLength > 0) {
    tempBitmap = psramNewArray<uint8_t>(tempGlyph.dataLength);
    if (!tempBitmap) {
      LOG_ERR("SDCF", "Overflow: failed to allocate %u bytes for U+%04X bitmap", tempGlyph.dataLength, codepoint);
      return nullptr;
    }
    if (!file.seekSet(s.toc.bitmap_offset + tempGlyph.dataOffset)) {
      LOG_ERR("SDCF", "Overflow: failed to seek to bitmap for U+%04X", codepoint);
      psramDeleteArray(tempBitmap);
      file.close();
      return nullptr;
    }
    if (file.read(tempBitmap, tempGlyph.dataLength) != static_cast<int>(tempGlyph.dataLength)) {
      LOG_ERR("SDCF", "Overflow: failed to read bitmap for U+%04X", codepoint);
      psramDeleteArray(tempBitmap);
      return nullptr;
    }
  }

  // All reads succeeded — commit to slot and advance ring buffer
  if (wasAtCapacity) {
    psramDeleteArray(self->overflow_[slot].bitmap);
  } else {
    self->overflowCount_++;
  }
  self->overflowNext_ = (slot + 1) % OVERFLOW_CAPACITY;
  self->overflow_[slot].glyph = tempGlyph;
  self->overflow_[slot].bitmap = tempBitmap;
  self->overflow_[slot].codepoint = codepoint;
  self->overflow_[slot].styleIdx = styleIdx;

  LOG_DBG("SDCF", "Overflow: loaded U+%04X style %u on demand (slot %u/%u)", codepoint, styleIdx, slot,
          OVERFLOW_CAPACITY);

  return &self->overflow_[slot].glyph;
}

bool SdCardFont::isOverflowGlyph(const EpdGlyph* glyph) const {
  for (uint32_t i = 0; i < overflowCount_; i++) {
    if (&overflow_[i].glyph == glyph) return true;
  }
  return false;
}

const uint8_t* SdCardFont::getOverflowBitmap(const EpdGlyph* glyph) const {
  for (uint32_t i = 0; i < overflowCount_; i++) {
    if (&overflow_[i].glyph == glyph) {
      return overflow_[i].bitmap;
    }
  }
  return nullptr;
}

SdCardFont* SdCardFont::fromMissCtx(void* ctx) { return static_cast<OverflowContext*>(ctx)->self; }
