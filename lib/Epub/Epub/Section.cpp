#include "Section.h"

#include <FontCacheManager.h>
#include <GfxRenderer.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <bembeyaz/section.h>

#include <algorithm>
#include <cstddef>
#include <string_view>
#include <type_traits>
#include <utility>

#include "Epub/css/CssParser.h"
#include "Page.h"
#include "SectionFileIo.h"
#include "hyphenation/Hyphenator.h"
#include "parsers/ChapterHtmlSlimParser.h"

namespace {
// The file's format, its version and its history live in rust/crates/bembeyaz-section (file.rs,
// page.rs for a page's body); bump VERSION there for a layout change that invalidates cached pages.

using section_file_io::fileOf;

bb_section_spec specOf(const ReaderRenderSpec& spec) {
  return {.font_id = spec.fontId,
          .line_compression = spec.lineCompression,
          .extra_paragraph_spacing = spec.extraParagraphSpacing,
          .paragraph_indent_spaces = spec.paragraphIndentSpaces,
          .paragraph_alignment = spec.paragraphAlignment,
          .viewport_width = spec.viewportWidth,
          .viewport_height = spec.viewportHeight,
          .hyphenation_enabled = spec.hyphenationEnabled,
          .embedded_style = spec.embeddedStyle,
          .image_rendering = spec.imageRendering,
          .focus_reading_enabled = spec.focusReadingEnabled,
          .character_spacing = spec.characterSpacing,
          .word_spacing_percent = spec.wordSpacingPercent};
}

// The header holds every spec field at its own size; a field that grows needs a format change.
static_assert(sizeof(ReaderRenderSpec::fontId) == sizeof(int32_t) &&
                  sizeof(ReaderRenderSpec::lineCompression) == sizeof(float) &&
                  sizeof(ReaderRenderSpec::viewportWidth) == sizeof(uint16_t) &&
                  sizeof(ReaderRenderSpec::viewportHeight) == sizeof(uint16_t) &&
                  sizeof(ReaderRenderSpec::paragraphIndentSpaces) == 1 &&
                  sizeof(ReaderRenderSpec::paragraphAlignment) == 1 && sizeof(ReaderRenderSpec::imageRendering) == 1 &&
                  sizeof(ReaderRenderSpec::characterSpacing) == 1 && sizeof(ReaderRenderSpec::wordSpacingPercent) == 1,
              "bb_section_spec fields");
static_assert(sizeof(bb_section_spec) == 24 && offsetof(bb_section_spec, word_spacing_percent) == 21 &&
                  sizeof(bb_section_lut_entry) == 12 && offsetof(bb_section_lut_entry, visible) == 8 &&
                  sizeof(bb_section_loaded) == 16 && offsetof(bb_section_loaded, consumed) == 4 &&
                  offsetof(bb_section_loaded, version) == 12 && sizeof(bb_section_watermark) == 8,
              "bb_section layout");

using Anchors = std::remove_reference_t<decltype(std::declval<const ChapterHtmlSlimParser&>().getAnchors())>;

const char* anchorAt(void* context, const size_t i, size_t* len, uint16_t* page) {
  const auto& anchor = (*static_cast<const Anchors*>(context))[i];
  *len = anchor.first.size();
  *page = anchor.second;
  return anchor.first.data();
}

// Runs a bembeyaz-section lookup on the committed file.
template <typename T, typename Lookup>
std::optional<T> lookUp(const std::string& path, Lookup&& lookup) {
  HalFile f;
  if (!Storage.openFileForRead("SCT", path, f)) {
    return std::nullopt;
  }
  const bb_section_file view = fileOf(f);
  T result{};
  if (!lookup(&view, &result)) {
    return std::nullopt;
  }
  return result;
}
}  // namespace

// Out-of-line so the unique_ptr<ChapterHtmlSlimParser> in BuildContext can be
// constructed/destroyed where the parser's full definition is visible.
Section::Section(const std::shared_ptr<Epub>& epub, const int spineIndex, GfxRenderer& renderer)
    : epub(epub),
      spineIndex(spineIndex),
      renderer(renderer),
      filePath(epub->getCachePath() + "/sections/" + std::to_string(spineIndex) + ".bin") {}

// Suspend any in-progress build so every section.reset() / navigation / sleep path
// persists the pages already laid out as a partial .bin instead of discarding them
// (no-op once a build has completed or never started).
Section::~Section() { suspendBuild(); }

uint32_t Section::onPageComplete(std::unique_ptr<Page> page) {
  if (!file) {
    LOG_ERR("SCT", "File not open for writing page %d", builtPageCount_);
    return 0;
  }

  const uint32_t position = file.position();
  if (!page->serialize(file)) {
    LOG_ERR("SCT", "Failed to serialize page %d", builtPageCount_);
    return 0;
  }
  LOG_DBG("SCT", "Page %d processed", builtPageCount_);

  builtPageCount_++;
  // pageCount is the pages available to read: a rebuild over a partial only raises it
  // once it has laid out more pages than the partial already covers.
  if (builtPageCount_ > pageCount) {
    pageCount = builtPageCount_;
  }
  return position;
}

void Section::writeSectionFileHeader(const ReaderRenderSpec& spec) {
  if (!file) {
    LOG_DBG("SCT", "File not open for writing header");
    return;
  }
  // Written with the incomplete version; commitBuildFile() stamps the real one last.
  const bb_section_spec s = specOf(spec);
  uint8_t header[BB_SECTION_HEADER_SIZE];
  bb_section_header(&s, header);
  file.write(header, sizeof(header));
}

bool Section::loadSectionFile(const ReaderRenderSpec& spec) {
  if (!Storage.openFileForRead("SCT", filePath, file)) {
    return false;
  }

  const bb_section_file view = fileOf(file);
  const bb_section_spec wanted = specOf(spec);
  bb_section_loaded loaded{};
  const int status = bb_section_check(&view, &wanted, &loaded);
  // Explicit close() required: member variable persists beyond function scope
  file.close();
  if (status != BB_SECTION_LOADED) {
    if (status == BB_SECTION_UNKNOWN_VERSION) {
      LOG_ERR("SCT", "Deserialization failed: Unknown version %u", loaded.version);
    } else if (status == BB_SECTION_OTHER_SPEC) {
      LOG_ERR("SCT", "Deserialization failed: Parameters do not match");
    } else {
      LOG_ERR("SCT", "Deserialization failed: malformed partial section");
      pageCount = 0;
    }
    clearCache();
    return false;
  }
  pageCount = loaded.page_count;
  if (loaded.partial) {
    // A partial's pageCount is the watermark of a suspended build; estimatedTotalPages
    // extrapolates from how far it had parsed.
    partialBytesConsumed_ = loaded.consumed;
    partialTotalBytes_ = loaded.total;
    partial_ = true;
    partialPageCount_ = pageCount;
  }
  LOG_DBG("SCT", "Deserialization succeeded: %d pages%s", pageCount, loaded.partial ? " (partial)" : "");
  return true;
}

// Your updated class method (assuming you are using the 'SD' object, which is a wrapper for a specific filesystem)
bool Section::clearCache() const {
  const std::string tmpBin = binTmpPath();
  if (Storage.exists(tmpBin.c_str())) {
    Storage.remove(tmpBin.c_str());
  }
  if (!Storage.exists(filePath.c_str())) {
    LOG_DBG("SCT", "Cache does not exist, no action needed");
    return true;
  }

  if (!Storage.remove(filePath.c_str())) {
    LOG_ERR("SCT", "Failed to clear cache");
    return false;
  }

  LOG_DBG("SCT", "Cache cleared successfully");
  return true;
}

bool Section::createSectionFile(const ReaderRenderSpec& spec, const std::function<void()>& popupFn) {
  // One-shot build: start, then lay out the whole section in a single pass.
  if (!startBuild(spec, popupFn)) {
    return false;
  }
  if (!buildSomeMore(0)) {  // 0 = build to completion
    return false;
  }
  return buildComplete_;
}

bool Section::startBuild(const ReaderRenderSpec& spec, const std::function<void()>& popupFn) {
  if (build_) {
    LOG_ERR("SCT", "startBuild called while a build is already active");
    return false;
  }
  // Reclaim rebuildable font caches before CSS and layout allocations.
  if (auto* fontCache = renderer.getFontCacheManager()) {
    fontCache->releaseSdFontCaches();
  }
  buildComplete_ = false;
  builtPageCount_ = 0;
  // Pages from a loaded partial stay readable (from filePath) while this build writes
  // to the tmp .bin, so availability never drops below the partial's watermark.
  pageCount = partial_ ? partialPageCount_ : 0;

  // Remove a stale tmp .bin from a crash-interrupted build; this build recreates it.
  {
    const std::string staleTmp = binTmpPath();
    if (Storage.exists(staleTmp.c_str())) {
      Storage.remove(staleTmp.c_str());
    }
  }

  const auto localPath = epub->getSpineItem(spineIndex).href;
  const auto htmlDir = epub->getCachePath() + "/html";
  const auto htmlPath = htmlDir + "/" + std::to_string(spineIndex) + ".html";
  const auto tmpHtmlPath = htmlDir + "/.tmp_" + std::to_string(spineIndex) + ".html";

  // Create cache directory if it doesn't exist
  {
    const auto sectionsDir = epub->getCachePath() + "/sections";
    Storage.mkdir(sectionsDir.c_str());
  }

  // Reuse the previously unzipped HTML if we already have it. The unzipped HTML is keyed only on the
  // book (it lives in the per-book cache dir), not on render settings, so it survives the invalidation
  // that wipes the layout (.bin) caches when font/margin/orientation change -- rebuilds then skip zip
  // inflation entirely. It's promoted by an atomic rename as soon as the inflate succeeds (below), so
  // even a window-only giant spine -- whose .bin never finalizes -- still caches its HTML, letting a
  // reopen skip the multi-second inflate. If htmlPath exists it is known-complete.
  const bool reusedHtml = Storage.exists(htmlPath.c_str());
  bool htmlCached = reusedHtml;
  if (reusedHtml) {
    LOG_DBG("SCT", "Reusing cached HTML %s", htmlPath.c_str());
  } else {
    Storage.mkdir(htmlDir.c_str());

    // Retry logic for SD card timing issues
    bool streamed = false;
    uint32_t fileSize = 0;
    for (int attempt = 0; attempt < 3 && !streamed; attempt++) {
      if (attempt > 0) {
        LOG_DBG("SCT", "Retrying stream (attempt %d)...", attempt + 1);
        delay(50);  // Brief delay before retry
      }

      // Remove any incomplete file from previous attempt before retrying
      if (Storage.exists(tmpHtmlPath.c_str())) {
        Storage.remove(tmpHtmlPath.c_str());
      }

      HalFile tmpHtml;
      if (!Storage.openFileForWrite("SCT", tmpHtmlPath, tmpHtml)) {
        continue;
      }
      // Larger chunks mean far fewer SD writes inflating the HTML; a 1KB chunk turned a 584KB
      // single-spine novel into ~570 tiny writes (multi-second). 8KB keeps the transient buffers
      // small while cutting the write count 8x.
      streamed = epub->readItemContentsToStream(localPath, tmpHtml, 8192);
      fileSize = tmpHtml.size();
      // Explicitly close() file before calling Storage.remove()
      tmpHtml.close();

      // If streaming failed, remove the incomplete file immediately
      if (!streamed && Storage.exists(tmpHtmlPath.c_str())) {
        Storage.remove(tmpHtmlPath.c_str());
        LOG_DBG("SCT", "Removed incomplete temp file after failed attempt");
      }
    }

    if (!streamed) {
      LOG_ERR("SCT", "Failed to stream item contents to temp file after retries");
      return false;
    }

    LOG_DBG("SCT", "Streamed temp HTML to %s (%d bytes)", tmpHtmlPath.c_str(), fileSize);

    // Promote to the persistent HTML cache immediately -- the inflate is complete and the bytes are
    // valid regardless of whether the layout build finishes, so reopening (even a window-only spine
    // that never finalizes its .bin) skips re-inflation. If the rename fails we just parse the temp.
    if (Storage.rename(tmpHtmlPath.c_str(), htmlPath.c_str())) {
      htmlCached = true;
    } else {
      LOG_DBG("SCT", "Failed to promote HTML cache; parsing from temp");
    }
  }

  if (!Storage.openFileForWrite("SCT", binTmpPath(), file)) {
    if (!reusedHtml) Storage.remove(tmpHtmlPath.c_str());
    return false;
  }
  // Header is written with the incomplete-version sentinel; finalizeBuild() commits it.
  writeSectionFileHeader(spec);

  auto ctx = makeUniqueNoThrow<BuildContext>();
  if (!ctx) {
    LOG_ERR("SCT", "OOM: BuildContext");
    file.close();
    Storage.remove(binTmpPath().c_str());
    if (!reusedHtml) Storage.remove(tmpHtmlPath.c_str());
    return false;
  }
  // htmlCached == "htmlPath is the live cache" (reused, or just promoted). finalizeBuild/abandonBuild
  // then leave the cached HTML alone; only an un-promoted temp (rename failed) is theirs to clean up.
  ctx->reusedHtml = htmlCached;
  ctx->htmlPath = htmlPath;
  ctx->tmpHtmlPath = tmpHtmlPath;
  ctx->parsePath = htmlCached ? htmlPath : tmpHtmlPath;

  // Derive the content base directory and image cache path prefix for the parser
  const size_t lastSlash = localPath.find_last_of('/');
  ctx->contentBase = (lastSlash != std::string::npos) ? localPath.substr(0, lastSlash + 1) : "";
  ctx->imageBasePath = epub->getCachePath() + "/img_" + std::to_string(spineIndex) + "_";

  if (spec.embeddedStyle) {
    ctx->cssParser = epub->getCssParser();
    if (ctx->cssParser) {
      const CssParser::CacheLoadResult cacheResult = ctx->cssParser->loadFromCache();
      if (cacheResult == CssParser::CacheLoadResult::LowMemory) {
        LOG_ERR("SCT", "Insufficient heap to hydrate CSS; section build deferred");
        ctx->cssParser->clear();
        file.close();
        Storage.remove(binTmpPath().c_str());
        if (!ctx->reusedHtml) Storage.remove(ctx->tmpHtmlPath.c_str());
        return false;
      }
      if (cacheResult == CssParser::CacheLoadResult::Invalid) {
        LOG_ERR("SCT", "Failed to load CSS from cache");
      }
    }
  }

  // Collect TOC anchors for this spine so the parser can insert page breaks at chapter boundaries
  std::vector<std::string> tocAnchors;
  const int startTocIndex = epub->getTocIndexForSpineIndex(spineIndex);
  if (startTocIndex >= 0) {
    for (int i = startTocIndex; i < epub->getTocItemsCount(); i++) {
      auto entry = epub->getTocItem(i);
      if (entry.spineIndex != spineIndex) break;
      if (!entry.anchor.empty()) {
        tocAnchors.push_back(std::move(entry.anchor));
      }
    }
  }

  // The parser stores the path/contentBase/imageBasePath by reference, so they must
  // live in the BuildContext (which outlives the parser). The page-complete callback
  // captures the BuildContext pointer to append to its in-RAM LUT; build_ owns the
  // context for the parser's whole lifetime.
  BuildContext* ctxPtr = ctx.get();
  ctx->parser = makeUniqueNoThrow<ChapterHtmlSlimParser>(
      epub, ctxPtr->parsePath, renderer, spec.fontId, spec.lineCompression, spec.extraParagraphSpacing,
      spec.paragraphAlignment, spec.viewportWidth, spec.viewportHeight, spec.hyphenationEnabled,
      spec.focusReadingEnabled,
      [this, ctxPtr](std::unique_ptr<Page> page, const uint16_t paragraphIndex, const uint16_t listItemIndex,
                     const uint32_t visibleTextOffset) {
        ctxPtr->lut.push_back(
            {this->onPageComplete(std::move(page)), paragraphIndex, listItemIndex, visibleTextOffset});
      },
      spec.embeddedStyle, ctxPtr->contentBase, ctxPtr->imageBasePath, spec.imageRendering, std::move(tocAnchors),
      popupFn, ctxPtr->cssParser);
  if (!ctx->parser) {
    LOG_ERR("SCT", "OOM: ChapterHtmlSlimParser");
    if (ctx->cssParser) ctx->cssParser->clear();
    file.close();
    Storage.remove(binTmpPath().c_str());
    if (!reusedHtml) Storage.remove(tmpHtmlPath.c_str());
    return false;
  }

  ctx->parser->setTextSpacing(spec.characterSpacing, spec.wordSpacingPercent);
  ctx->parser->setParagraphIndentSpaces(spec.paragraphIndentSpaces);
  Hyphenator::setPreferredLanguage(epub->getLanguage());
  build_ = std::move(ctx);

  if (!build_->parser->beginParse()) {
    LOG_ERR("SCT", "Failed to begin parse");
    abandonBuild();
    return false;
  }
  build_->totalBytes = build_->parser->parseTotalBytes();
  return true;
}

bool Section::buildSomeMore(const int maxPages) {
  if (!build_ || !build_->parser) {
    LOG_ERR("SCT", "buildSomeMore with no active build");
    return false;
  }
  // Pace on pages laid out by THIS build, not pageCount: during a rebuild over a partial,
  // pageCount stays pinned at the partial's watermark until the build passes it, which
  // would otherwise turn one "small" chunk into a blocking rebuild of the whole watermark.
  const int startCount = builtPageCount_;
  for (;;) {
    const auto status = build_->parser->parseStep();
    if (status == ChapterHtmlSlimParser::ParseStatus::Error) {
      LOG_ERR("SCT", "Parse error during incremental build");
      abandonBuild();
      return false;
    }
    if (status == ChapterHtmlSlimParser::ParseStatus::Done) {
      return finalizeBuild();
    }
    // ParseStatus::More: yield once we've laid out the requested number of pages.
    if (maxPages > 0 && (builtPageCount_ - startCount) >= maxPages) {
      build_->bytesConsumed = build_->parser->parseBytesConsumed();
      return true;
    }
  }
}

bool Section::hasHtmlCache() const {
  const std::string htmlPath = epub->getCachePath() + "/html/" + std::to_string(spineIndex) + ".html";
  return Storage.exists(htmlPath.c_str());
}

std::optional<uint16_t> Section::findAnchorDuringBuild(const std::string_view anchor) const {
  if (!build_ || !build_->parser) return std::nullopt;
  for (const auto& [key, page] : build_->parser->getAnchors()) {
    if (key == anchor) return page;
  }
  return std::nullopt;
}

std::optional<uint16_t> Section::findAnchor(const std::string& anchor) const {
  if (const auto page = findAnchorDuringBuild(anchor)) {
    return page;
  }
  // Fall back to the on-disk anchor map: a finalized section, or a partial whose map
  // covers everything up to its watermark (nullopt past it -- build further and retry).
  return getPageForAnchor(anchor);
}

int Section::getTocIndexForPage(const int page) const {
  int selected = epub->getTocIndexForSpineIndex(spineIndex);
  int latestPage = -1;
  struct TocAnchor {
    size_t hash;
    int index;
  };
  // Names on disk longer than every target cannot match one; the walk skips them.
  size_t longestTarget = 0;
  std::vector<uint8_t> nameBuffer;
  // Bound scratch space to 512 bytes on C3; anchor text stays in the existing caches.
  constexpr size_t BATCH_SIZE = 64;
  auto targets = makeUniqueNoThrow<TocAnchor[]>(BATCH_SIZE);
  if (!targets) {
    LOG_ERR("SCT", "OOM: TOC anchor lookup");
    return selected;
  }
  HalFile anchorFile;
  const bool haveAnchorMap = Storage.openFileForRead("SCT", filePath, anchorFile);
  size_t count = 0;
  const auto considerAnchor = [&](const std::string_view anchor, const uint16_t anchorPage, const bool fromDisk) {
    const size_t hash = std::hash<std::string_view>{}(anchor);
    for (size_t i = 0; i < count; ++i) {
      if (targets[i].hash != hash || epub->getTocItem(targets[i].index).anchor != anchor) continue;
      const int resolvedPage = fromDisk ? findAnchorDuringBuild(anchor).value_or(anchorPage) : anchorPage;
      if (resolvedPage <= page &&
          (resolvedPage > latestPage || (resolvedPage == latestPage && targets[i].index > selected))) {
        selected = targets[i].index;
        latestPage = resolvedPage;
      }
    }
  };
  const auto resolveBatch = [&]() {
    if (build_ && build_->parser) {
      for (const auto& [anchor, anchorPage] : build_->parser->getAnchors()) {
        considerAnchor(anchor, anchorPage, false);
      }
    }
    if (haveAnchorMap) {
      const bb_section_file view = fileOf(anchorFile);
      nameBuffer.resize(longestTarget + 2);  // the name, then its page
      bb_section_anchors(
          &view, nameBuffer.data(), nameBuffer.size(), const_cast<void*>(static_cast<const void*>(&considerAnchor)),
          [](void* context, const char* name, const size_t len, const uint16_t anchorPage) {
            (*static_cast<const decltype(considerAnchor)*>(context))(std::string_view(name, len), anchorPage, true);
          });
    }
    count = 0;
    longestTarget = 0;
  };
  const int first = std::max(0, selected);
  for (int i = first; i < epub->getTocItemsCount(); ++i) {
    const auto item = epub->getTocItem(i);
    if (item.spineIndex != spineIndex) continue;
    if (item.anchor.empty()) {
      if (page >= 0 && latestPage <= 0) {
        selected = i;
        latestPage = 0;
      }
    } else {
      targets[count++] = {std::hash<std::string_view>{}(item.anchor), i};
      longestTarget = std::max(longestTarget, item.anchor.size());
      if (count == BATCH_SIZE) resolveBatch();
    }
  }
  if (count > 0) resolveBatch();
  return selected;
}

uint16_t Section::estimatedTotalPages() const {
  // Extrapolation from a suspended session's watermark trailer. A static snapshot, so no EMA
  // damping is needed. Also the best guess while a rebuild is running but hasn't laid out
  // enough pages yet to extrapolate from its own progress.
  const auto partialEstimate = [this]() -> uint16_t {
    if (!partial_ || partialBytesConsumed_ == 0 || partialTotalBytes_ <= partialBytesConsumed_) {
      return pageCount;
    }
    const uint64_t est = static_cast<uint64_t>(partialPageCount_) * partialTotalBytes_ / partialBytesConsumed_;
    if (est <= pageCount) return pageCount;
    return est > 60000 ? 60000 : static_cast<uint16_t>(est);
  };

  if (!build_) {
    return partial_ ? partialEstimate() : pageCount;  // partial -> extrapolate, finalized -> exact
  }
  const uint32_t consumed = build_->bytesConsumed;
  const uint32_t total = build_->totalBytes;
  if (builtPageCount_ == 0 || consumed == 0 || total <= consumed) return partialEstimate();

  // Raw extrapolation: scale the pages built so far by the fraction of HTML still unparsed. This
  // re-derives from a growing, non-uniform sample, so it jitters up and down as the build crosses
  // dense vs sparse regions of the chapter.
  const uint64_t raw = static_cast<uint64_t>(builtPageCount_) * total / consumed;

  // Damp that jitter with an exponential moving average. Step it once per build advance (keyed on
  // bytesConsumed) rather than per status-bar redraw, so the smoothing rate doesn't depend on how
  // often we repaint. As the build nears the end, consumed -> total and raw -> the built count, so
  // the average settles onto the true count (and finalizeBuild then returns the exact pageCount).
  constexpr float ALPHA = 0.25f;  // weight of each new sample; lower = steadier but slower to settle
  if (build_->smoothedEstimate <= 0) {
    build_->smoothedEstimate = static_cast<float>(raw);  // seed on the first estimate
  } else if (consumed != build_->smoothedAtConsumed) {
    build_->smoothedEstimate += ALPHA * (static_cast<float>(raw) - build_->smoothedEstimate);
  }
  build_->smoothedAtConsumed = consumed;

  const uint64_t est = static_cast<uint64_t>(build_->smoothedEstimate + 0.5f);
  if (est <= pageCount) return pageCount;  // never fewer than the pages already available
  return est > 60000 ? 60000 : static_cast<uint16_t>(est);
}

// Write the LUTs and anchor map into the open tmp .bin, patch the header with the built
// page count and table offsets, stamp the version as the commit point, then swap the tmp
// file over filePath. A partial also gets a watermark trailer (bytesConsumed, totalBytes)
// after the visible-offset LUT so a later open can estimate the total page count. The
// parser must still be alive (anchors are read from it).
// On failure the tmp is removed and any pre-existing file at filePath is left intact.
bool Section::commitBuildFile(const bool asPartial, const uint32_t bytesConsumed, const uint32_t totalBytes) {
  const auto failCommit = [this]() {
    // Explicit close() required before remove (member variable, O_RDWR handle).
    file.close();
    Storage.remove(binTmpPath().c_str());
    return false;
  };

  const Anchors& anchors = build_->parser->getAnchors();
  const bb_section_sink sink = section_file_io::sinkOf(file);
  const bb_section_watermark watermark{bytesConsumed, totalBytes};
  const int result = bb_section_commit(
      &sink, static_cast<uint32_t>(file.position()), build_->lut.data(), build_->lut.size(), anchors.size(), anchorAt,
      const_cast<void*>(static_cast<const void*>(&anchors)), asPartial ? &watermark : nullptr);
  if (result == BB_SECTION_MISSING_PAGE) {
    LOG_ERR("SCT", "Failed to write LUT due to invalid page positions");
    return failCommit();
  }
  if (result != BB_SECTION_COMMITTED) {
    LOG_ERR("SCT", "Failed to write the section tables");
    return failCommit();
  }
  // Explicit close() required: member variable persists beyond function scope
  file.close();

  // Swap into place. A crash between remove and rename loses the old file but keeps a
  // fully-committed tmp; the next build just removes it and rebuilds.
  if (Storage.exists(filePath.c_str())) {
    Storage.remove(filePath.c_str());
  }
  if (!Storage.rename(binTmpPath().c_str(), filePath.c_str())) {
    LOG_ERR("SCT", "Failed to move built section into place");
    Storage.remove(binTmpPath().c_str());
    return false;
  }
  return true;
}

bool Section::finalizeBuild() {
  // Flush the trailing page (emits the last page via the completePageFn into the LUT).
  // A false return means layout dropped content (OOM); committing would persist a
  // section cache with holes in the text, so abandon the build instead.
  if (!build_->parser->finishParse()) {
    LOG_ERR("SCT", "Parse finalize failed; abandoning section build");
    abandonBuild();
    return false;
  }

  if (!build_->reusedHtml) {
    // Parse succeeded: promote the freshly unzipped HTML to the persistent cache so future
    // rebuilds skip zip inflation. If promotion fails, drop the temp -- the build still succeeded.
    if (!Storage.rename(build_->tmpHtmlPath.c_str(), build_->htmlPath.c_str())) {
      LOG_DBG("SCT", "Failed to promote HTML cache, removing temp");
      Storage.remove(build_->tmpHtmlPath.c_str());
    }
  }

  const bool committed = commitBuildFile(false, 0, 0);
  if (build_->cssParser) build_->cssParser->clear();
  build_.reset();
  if (!committed) {
    // commitBuildFile removed filePath before the failed swap, so nothing valid remains.
    partial_ = false;
    partialPageCount_ = 0;
    pageCount = 0;
    builtPageCount_ = 0;
    return false;
  }
  buildComplete_ = true;
  partial_ = false;
  partialPageCount_ = 0;
  pageCount = builtPageCount_;
  return true;
}

void Section::suspendBuild() {
  if (!build_) return;

  // Only worth persisting if this build produced pages a pre-existing partial doesn't
  // already cover; otherwise keep the older (bigger) partial and just drop the tmp.
  const bool worthKeeping = builtPageCount_ > 0 && (!partial_ || builtPageCount_ > partialPageCount_);

  bool committed = false;
  if (worthKeeping) {
    // Capture the parse watermark and commit before tearing the parser down (the anchor
    // map is read from it). The incomplete trailing page is intentionally not flushed:
    // only fully laid-out pages are persisted, and the rebuild re-derives the rest.
    const uint32_t consumed = static_cast<uint32_t>(build_->parser->parseBytesConsumed());
    committed = commitBuildFile(true, consumed, build_->totalBytes);
    if (committed) {
      partial_ = true;
      partialPageCount_ = builtPageCount_;
      partialBytesConsumed_ = consumed;
      partialTotalBytes_ = build_->totalBytes;
      LOG_INF("SCT", "Suspended build: %u pages persisted", builtPageCount_);
    }
  }

  if (build_->parser) build_->parser->abortParse();
  if (build_->cssParser) build_->cssParser->clear();
  if (!committed && file) {
    // Explicit close() required before remove (member variable, O_RDWR handle).
    file.close();
    Storage.remove(binTmpPath().c_str());
  }
  if (!build_->reusedHtml && Storage.exists(build_->tmpHtmlPath.c_str())) {
    Storage.remove(build_->tmpHtmlPath.c_str());
  }
  build_.reset();
  buildComplete_ = false;
  pageCount = partial_ ? partialPageCount_ : 0;
  builtPageCount_ = 0;
}

void Section::abandonBuild() {
  if (!build_) return;
  if (build_->parser) build_->parser->abortParse();
  if (build_->cssParser) build_->cssParser->clear();
  if (file) {
    // Explicit close() required before remove (member variable, O_RDWR handle).
    file.close();
    Storage.remove(binTmpPath().c_str());
  }
  // A parse error would recur against the same HTML, so drop any partial too -- resuming
  // from it would just re-enter the failing build every open.
  if (Storage.exists(filePath.c_str())) {
    Storage.remove(filePath.c_str());
  }
  if (!build_->reusedHtml && Storage.exists(build_->tmpHtmlPath.c_str())) {
    Storage.remove(build_->tmpHtmlPath.c_str());
  }
  build_.reset();
  buildComplete_ = false;
  partial_ = false;
  partialPageCount_ = 0;
  pageCount = 0;
  builtPageCount_ = 0;
}

std::unique_ptr<Page> Section::loadPageDuringBuild(const int page) {
  if (!build_ || page < 0 || page >= static_cast<int>(build_->lut.size()) || !file) {
    return nullptr;
  }
  const uint32_t pos = build_->lut[page].position;
  if (pos == 0) {
    return nullptr;
  }
  // The .bin is open O_RDWR for the build. Read the already-written page, then restore
  // the write cursor so the next onPageComplete keeps appending where it left off.
  const uint32_t writePos = file.position();
  file.seek(pos);
  auto p = Page::deserialize(file);
  file.seek(writePos);
  if (p) {
    p->visibleTextOffset = build_->lut[page].visible;
  }
  return p;
}

// Read a page from the committed file at filePath (finalized section or partial from a
// previous session). Uses a local handle so it is safe while a build holds the member
// `file` open on the tmp .bin.
std::unique_ptr<Page> Section::loadPageAt(const int page) const {
  HalFile f;
  if (!Storage.openFileForRead("SCT", filePath, f)) {
    return nullptr;
  }

  // The page's visible-codepoint start offset comes from the same open handle, so the reader can
  // persist progress without reopening the section file on every page turn (see
  // Page::visibleTextOffset). A malformed/old file leaves it at 0.
  const bb_section_file view = fileOf(f);
  uint32_t position = 0;
  uint32_t visibleTextOffset = 0;
  if (!bb_section_page(&view, static_cast<uint16_t>(page), &position, &visibleTextOffset)) {
    return nullptr;
  }
  f.seek(position);
  auto p = Page::deserialize(f);
  if (p) {
    p->visibleTextOffset = visibleTextOffset;
  }
  return p;
  // No f.close() needed -- DESTRUCTOR_CLOSES_FILE=1 handles it at scope exit
}

std::unique_ptr<Page> Section::loadPage(const int page) {
  if (page < 0) {
    return nullptr;
  }
  if (build_ && page < static_cast<int>(build_->lut.size())) {
    return loadPageDuringBuild(page);
  }
  // Not (yet) in the active build: serve from the file on disk -- a finalized section,
  // or a partial from a previous session whose pages the rebuild hasn't reached again.
  const int onDisk = partial_ ? partialPageCount_ : (build_ ? 0 : pageCount);
  if (page >= onDisk) {
    return nullptr;
  }
  return loadPageAt(page);
}

std::string Section::getTextFromSectionFile() {
  std::string fullText;
  auto p = loadPage(currentPage);
  if (p) {
    for (const auto& el : p->elements) {
      if (el->getTag() == TAG_PageLine) {
        const auto& line = static_cast<const PageLine&>(*el);
        if (line.getBlock()) {
          const auto& block = *line.getBlock();
          for (uint16_t i = 0; i < block.wordCount(); i++) {
            if (!fullText.empty()) fullText += " ";
            fullText += block.wordText(i);
          }
        }
      }
    }
  }
  return fullText;
}

std::optional<uint16_t> Section::getCachedPageCount() const {
  return lookUp<uint16_t>(filePath, bb_section_finished_page_count);
}

std::optional<uint16_t> Section::getPageForAnchor(const std::string& anchor) const {
  return lookUp<uint16_t>(filePath, [&anchor](const bb_section_file* view, uint16_t* out) {
    return bb_section_find_anchor(view, anchor.data(), anchor.size(), out);
  });
}

std::optional<uint16_t> Section::getPageForParagraphIndex(const uint16_t pIndex) const {
  return lookUp<uint16_t>(filePath, [pIndex](const bb_section_file* view, uint16_t* out) {
    return bb_section_page_for_paragraph(view, pIndex, out);
  });
}

std::optional<uint16_t> Section::getParagraphIndexForPage(const uint16_t page) const {
  return lookUp<uint16_t>(filePath, [page](const bb_section_file* view, uint16_t* out) {
    return bb_section_paragraph_for_page(view, page, out);
  });
}

std::optional<uint16_t> Section::getPageForListItemIndex(const uint16_t liIndex) const {
  return lookUp<uint16_t>(filePath, [liIndex](const bb_section_file* view, uint16_t* out) {
    return bb_section_page_for_list_item(view, liIndex, out);
  });
}

std::optional<uint32_t> Section::getVisibleTextOffsetForPage(const uint16_t page) const {
  if (build_ && page < build_->lut.size()) {
    return build_->lut[page].visible;
  }
  return lookUp<uint32_t>(filePath, [page](const bb_section_file* view, uint32_t* out) {
    return bb_section_visible_offset(view, page, out);
  });
}

std::optional<uint16_t> Section::getPageForVisibleTextOffset(const uint32_t offset,
                                                             const bool preferFirstAtOffset) const {
  // Resolve within the active build's known range. Later offsets may still be
  // covered by an on-disk partial that the resumed build has not reached yet.
  if (buildReachedVisibleTextOffset(offset)) {
    uint16_t page = 0;
    bb_section_page_for_visible_offset_in(build_->lut.data(), build_->lut.size(), offset, preferFirstAtOffset, &page);
    return page;
  }
  return lookUp<uint16_t>(filePath, [offset, preferFirstAtOffset](const bb_section_file* view, uint16_t* out) {
    return bb_section_page_for_visible_offset(view, offset, preferFirstAtOffset, out);
  });
}
