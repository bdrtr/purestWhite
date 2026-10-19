#include "CssParser.h"

#include <Logging.h>

#include <array>
#include <cstddef>

// Rust writes CssStyle and CssAncestor in place: the layouts must match its Style and Ancestor.
static_assert(sizeof(CssStyle) == 128 && alignof(CssStyle) == 4);
static_assert(offsetof(CssStyle, textIndent) == 8 && offsetof(CssStyle, fontSize) == 96);
static_assert(offsetof(CssStyle, display) == 104 && offsetof(CssStyle, borderTop) == 110);
static_assert(offsetof(CssStyle, borderWidthsDefined) == 118 && offsetof(CssStyle, preserveWhitespace) == 123);
static_assert(offsetof(CssStyle, defined) == 124 && sizeof(CssPropertyFlags) == 4);
static_assert(sizeof(CssLength) == 8 && sizeof(CssBorderSide) == 2);
static_assert(sizeof(CssAncestor) == 28 && offsetof(CssAncestor, classCount) == 24);

namespace {

// Each CssPropertyFlags field is bit i of the little-endian word Rust reads, in declaration order.
constexpr bool flagIsBit(CssPropertyFlags flags, const unsigned bit) {
  const auto bytes = __builtin_bit_cast(std::array<uint8_t, 4>, flags);
  const uint32_t word = bytes[0] | bytes[1] << 8 | bytes[2] << 16 | static_cast<uint32_t>(bytes[3]) << 24;
  return word == 1u << bit;
}
constexpr CssPropertyFlags only(void (*set)(CssPropertyFlags&)) {
  CssPropertyFlags flags;
  set(flags);
  return flags;
}
static_assert(flagIsBit(only([](CssPropertyFlags& f) { f.textAlign = 1; }), 0));
static_assert(flagIsBit(only([](CssPropertyFlags& f) { f.imageWidth = 1; }), 14));
static_assert(flagIsBit(only([](CssPropertyFlags& f) { f.display = 1; }), 15));
static_assert(flagIsBit(only([](CssPropertyFlags& f) { f.direction = 1; }), 16));
static_assert(flagIsBit(only([](CssPropertyFlags& f) { f.whiteSpace = 1; }), 30));

bb_css_style* out(CssStyle& style) { return reinterpret_cast<bb_css_style*>(&style); }

// Cache file name (version is CssParser::CSS_CACHE_VERSION)
constexpr char rulesCache[] = "/css_rules.cache";
constexpr char rulesCacheTmp[] = "/css_rules.cache.tmp";
constexpr char rulesCacheBackup[] = "/css_rules.cache.bak";

// Buffer size for reading CSS files
constexpr size_t READ_BUFFER_SIZE = 512;

bb_css_source sourceFor(HalFile& file) {
  return {
      &file,
      [](void* context, uint8_t* buf, const size_t len) {
        return static_cast<size_t>(static_cast<HalFile*>(context)->read(buf, len)) == len;
      },
      [](void* context) { return static_cast<size_t>(static_cast<HalFile*>(context)->available()); },
      [](void* context, const size_t len) {
        auto* file = static_cast<HalFile*>(context);
        return static_cast<size_t>(file->available()) >= len && file->seekCur(len);
      },
  };
}

}  // namespace

CssParser::ParseResult CssParser::loadFromStream(HalFile& source) {
  if (!source) {
    LOG_ERR("CSS", "Cannot read from invalid file");
    return ParseResult::Error;
  }

  // About 2.2 KB, on this stack as the C++ parser's buffers were.
  bb_css_tokenizer tokenizer;
  bb_css_tokenizer_init(&tokenizer);
  size_t totalRead = 0;
  char buffer[READ_BUFFER_SIZE];
  while (source.available()) {
    const int bytesRead = source.read(buffer, sizeof(buffer));
    if (bytesRead <= 0) break;
    totalRead += static_cast<size_t>(bytesRead);
    bb_css_feed(&rules_, &tokenizer, buffer, static_cast<size_t>(bytesRead));
  }
  const int result = bb_css_finish(&rules_, &tokenizer);

  switch (bb_css_rules_stopped(&rules_)) {
    case BB_CSS_STOPPED_AT_LIMIT:
      LOG_ERR("CSS", "CSS rule store limit reached at %zu rules", ruleCount());
      break;
    case BB_CSS_STOPPED_OUT_OF_MEMORY:
      LOG_ERR("CSS", "OOM while growing CSS rule store at %zu rules", ruleCount());
      break;
    default:
      break;
  }
  if (bb_css_tokenizer_truncated(&tokenizer)) {
    LOG_ERR("CSS", "CSS input exceeded parser buffer; cache will remain partial");
  }
  LOG_DBG("CSS", "Parsed %zu rules from %zu bytes", ruleCount(), totalRead);
  return result == BB_CSS_COMPLETE ? ParseResult::Complete : ParseResult::Partial;
}

CssAncestor CssParser::makeAncestor(const std::string_view tagName, const std::string_view classAttr,
                                    const std::string_view idAttr) {
  CssAncestor ancestor;
  bb_css_make_ancestor(tagName.data(), tagName.size(), classAttr.data(), classAttr.size(), idAttr.data(), idAttr.size(),
                       reinterpret_cast<bb_css_ancestor*>(&ancestor));
  return ancestor;
}

CssStyle CssParser::resolveStyle(const std::string_view tagName, const std::string_view classAttr,
                                 const std::string_view idAttr, const CssAncestor* ancestors,
                                 const size_t ancestorCount, const bool firstLetter) const {
  const bb_css_query query{tagName.data(),
                           tagName.size(),
                           classAttr.data(),
                           classAttr.size(),
                           idAttr.data(),
                           idAttr.size(),
                           reinterpret_cast<const bb_css_ancestor*>(ancestors),
                           ancestorCount,
                           firstLetter};
  CssStyle style;
  bb_css_resolve(&rules_, &query, out(style));
  return style;
}

CssStyle CssParser::parseInlineStyle(const std::string_view styleValue) {
  CssStyle style;
  bb_css_parse_inline(styleValue.data(), styleValue.size(), out(style));
  return style;
}

// Cache files

bool CssParser::hasCache() const { return Storage.exists((cachePath + rulesCache).c_str()); }

bool CssParser::restoreCacheBackupIfNeeded() const {
  if (cachePath.empty()) {
    return false;
  }

  const std::string finalPath = cachePath + rulesCache;
  if (Storage.exists(finalPath.c_str())) {
    return true;
  }

  const std::string backupPath = cachePath + rulesCacheBackup;
  if (!Storage.exists(backupPath.c_str())) {
    return false;
  }

  if (!Storage.rename(backupPath.c_str(), finalPath.c_str())) {
    LOG_ERR("CSS", "Failed to restore CSS cache backup");
    return false;
  }

  LOG_DBG("CSS", "Restored CSS cache backup after interrupted replacement");
  return true;
}

void CssParser::deleteCache() const {
  if (hasCache()) Storage.remove((cachePath + rulesCache).c_str());
  Storage.remove((cachePath + rulesCacheTmp).c_str());
  Storage.remove((cachePath + rulesCacheBackup).c_str());
}

CssParser::CacheStatus CssParser::inspectCache() const {
  if (cachePath.empty() || (!hasCache() && !restoreCacheBackupIfNeeded())) {
    return CacheStatus::Missing;
  }

  HalFile file;
  if (!Storage.openFileForRead("CSS", cachePath + rulesCache, file)) {
    return CacheStatus::Invalid;
  }
  const bb_css_source source = sourceFor(file);
  switch (bb_css_cache_inspect(&source)) {
    case BB_CSS_CACHE_COMPLETE:
      return CacheStatus::Complete;
    case BB_CSS_CACHE_PARTIAL:
      return CacheStatus::Partial;
    default:
      return CacheStatus::Invalid;
  }
}

bool CssParser::saveToCache(const bool complete) const {
  if (cachePath.empty()) {
    return false;
  }

  const std::string finalPath = cachePath + rulesCache;
  const std::string tmpPath = cachePath + rulesCacheTmp;
  const std::string backupPath = cachePath + rulesCacheBackup;

  Storage.remove(tmpPath.c_str());

  HalFile file;
  if (!Storage.openFileForWrite("CSS", tmpPath, file)) {
    return false;
  }

  const auto write = [](void* context, const uint8_t* data, const size_t len) {
    return static_cast<HalFile*>(context)->write(data, len) == len;
  };
  const bool writeOk = bb_css_cache_save(&rules_, complete, write, &file);
  if (!writeOk || !file.close()) {
    LOG_ERR("CSS", "Failed to write temporary CSS cache");
    file.close();
    Storage.remove(tmpPath.c_str());
    return false;
  }

  const bool hadExistingCache = Storage.exists(finalPath.c_str());
  if (hadExistingCache) {
    Storage.remove(backupPath.c_str());
    if (!Storage.rename(finalPath.c_str(), backupPath.c_str())) {
      LOG_ERR("CSS", "Failed to back up existing CSS cache");
      Storage.remove(tmpPath.c_str());
      return false;
    }
  }

  if (!Storage.rename(tmpPath.c_str(), finalPath.c_str())) {
    LOG_ERR("CSS", "Failed to promote temporary CSS cache");
    Storage.remove(tmpPath.c_str());
    if (Storage.exists(backupPath.c_str()) && !Storage.rename(backupPath.c_str(), finalPath.c_str())) {
      LOG_ERR("CSS", "Failed to restore previous CSS cache");
    }
    return false;
  }

  Storage.remove(backupPath.c_str());

  LOG_DBG("CSS", "Saved %zu rules to %s cache", ruleCount(), complete ? "complete" : "partial");
  return true;
}

CssParser::CacheLoadResult CssParser::loadFromCache() {
  if (cachePath.empty()) {
    return CacheLoadResult::Invalid;
  }

  HalFile file;
  if (!Storage.openFileForRead("CSS", cachePath + rulesCache, file)) {
    if (!restoreCacheBackupIfNeeded() || !Storage.openFileForRead("CSS", cachePath + rulesCache, file)) {
      return CacheLoadResult::Invalid;
    }
  }

  const bb_css_source source = sourceFor(file);
  switch (bb_css_cache_load(&rules_, &source)) {
    case BB_CSS_LOAD_COMPLETE:
      LOG_DBG("CSS", "Loaded %zu rules from cache", ruleCount());
      return CacheLoadResult::Complete;
    case BB_CSS_LOAD_LOW_MEMORY:
      return CacheLoadResult::LowMemory;
    case BB_CSS_LOAD_STALE:
      LOG_DBG("CSS", "Cache version mismatch (expected %u), removing stale cache for rebuild", CSS_CACHE_VERSION);
      // Explicitly close() file before calling Storage.remove()
      file.close();
      Storage.remove((cachePath + rulesCache).c_str());
      return CacheLoadResult::Invalid;
    default:
      LOG_DBG("CSS", "Invalid CSS cache");
      return CacheLoadResult::Invalid;
  }
}
