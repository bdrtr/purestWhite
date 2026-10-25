#pragma once

#include <HalStorage.h>
#include <bembeyaz/chapter_parser.h>

#include <cstddef>
#include <cstdint>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include "Epub/blocks/TextBlock.h"
#include "Epub/css/CssParser.h"

class Page;
class GfxRenderer;
class Epub;

// Lays a chapter's XHTML out onto pages: rust/crates/bembeyaz-chapter reads it and decides the
// layout; this class keeps the chapter's file, the pages, their elements and the laid-out lines.
class ChapterHtmlSlimParser {
  std::shared_ptr<Epub> epub;
  const std::string& filepath;
  GfxRenderer& renderer;
  std::function<void(std::unique_ptr<Page>, uint16_t, uint16_t, uint32_t)> completePageFn;
  std::function<void()> popupFn;
  bool imagePopupFired = false;  // popupFn fired for the first unsized image (single-shot)
  const CssParser* cssParser;
  std::string imageBasePath;
  int imageCounter = 0;
  bb_chapter_settings settings{};
  bb_chapter* chapter = nullptr;

  // The page being filled, and the next one a page break set aside with the lines it carries.
  std::unique_ptr<Page> currentPage;
  std::unique_ptr<Page> nextPage;
  // Laid-out lines kept until they are placed, by handle; a placed or dropped one is NULL.
  struct KeptLine {
    std::unique_ptr<TextBlock> line;
    int baseHeight = 0;
    int rubyShift = 0;
  };
  std::vector<KeptLine> lines;
  std::vector<uint32_t> freeLines;
  // The image last sized, for push_image.
  std::string imagePath;
  std::string imageSource;

  HalFile parseFile_;
  HalFile tableFile_;
  bool parsing_ = false;

  bb_chapter_host host();

  // For the host tests, which reach them with `#define private public`: the document's events one
  // at a time, and the page being filled.
  void startElement(const char* name, const char** atts);
  void characterData(const char* text, size_t len);
  void endElement(const char* name);
  const bb_pager* pager() const { return bb_chapter_pager(chapter); }
  Page* page() const { return currentPage.get(); }
  // Takes the page being filled away; the next content starts a new one.
  std::unique_ptr<Page> takePage() {
    bb_chapter_restart_pages(chapter);
    nextPage.reset();
    return std::move(currentPage);
  }

 public:
  explicit ChapterHtmlSlimParser(
      std::shared_ptr<Epub> epub, const std::string& filepath, GfxRenderer& renderer, int fontId, float lineCompression,
      bool extraParagraphSpacing, uint8_t paragraphAlignment, uint16_t viewportWidth, uint16_t viewportHeight,
      bool hyphenationEnabled, bool focusReadingEnabled,
      const std::function<void(std::unique_ptr<Page>, uint16_t, uint16_t, uint32_t)>& completePageFn,
      bool embeddedStyle, const std::string& contentBase, const std::string& imageBasePath, uint8_t imageRendering = 0,
      std::vector<std::string> tocAnchors = {}, const std::function<void()>& popupFn = nullptr,
      const CssParser* cssParser = nullptr);
  ~ChapterHtmlSlimParser();
  ChapterHtmlSlimParser(const ChapterHtmlSlimParser&) = delete;
  ChapterHtmlSlimParser& operator=(const ChapterHtmlSlimParser&) = delete;

  void setTextSpacing(int8_t character, uint8_t wordPercent);
  void setParagraphIndentSpaces(uint8_t spaces);

  // One-shot parse: builds every page before returning (begin + step* + finish).
  bool parseAndBuildPages();

  // Resumable parse, for the incremental section builder. Drive as:
  //   if (!beginParse()) fail;
  //   loop: switch (parseStep()) { More: keep going / yield; Done: finishParse(); Error: abortParse(); }
  // Pages are emitted via completePageFn as they complete during parseStep(), so
  // the caller can stop once enough pages are built and resume on a later tick.
  enum class ParseStatus { More, Done, Error };
  bool beginParse();
  ParseStatus parseStep();
  bool finishParse();  // flush the trailing page and tear down; returns true
  void abortParse();   // tear down without flushing (error / abandon)

  // The anchors placed so far, in order: the i-th's id and page.
  size_t anchorCount() const;
  std::string_view anchorAt(size_t i, uint16_t& page) const;
  std::optional<uint16_t> findAnchor(std::string_view name) const;

  // Byte progress of the in-flight parse, used to estimate a still-building section's total page
  // count (a giant single-spine book never fully lays out, so its real count is unknown). Valid
  // between beginParse() and finishParse()/abortParse().
  size_t parseBytesConsumed() { return parseFile_ ? parseFile_.position() : 0; }
  size_t parseTotalBytes() { return parseFile_ ? parseFile_.size() : 0; }
};
