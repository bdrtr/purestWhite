#include "ChapterHtmlSlimParser.h"

#include <GfxRenderer.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <MemoryManager.h>

#include <cstring>

#include "Epub.h"
#include "Epub/Page.h"
#include "Epub/ParsedText.h"
#include "Epub/converters/ImageDecoderFactory.h"
#include "Epub/converters/ImageDimsProbe.h"
#include "Epub/converters/ImageToFramebufferDecoder.h"
#include "Epub/hyphenation/Hyphenator.h"

// Minimum file size (in bytes) to show indexing popup - smaller chapters don't benefit from it
constexpr size_t MIN_SIZE_FOR_POPUP = 10 * 1024;  // 10KB

static_assert(FOOTNOTE_NUMBER_LEN == BB_FOOTNOTE_NUMBER_LEN && FOOTNOTE_HREF_LEN == BB_FOOTNOTE_HREF_LEN,
              "bembeyaz-chapter keeps what FootnoteEntry holds");
static_assert(sizeof(CssBorderSide) == sizeof(bb_box_side) && offsetof(CssBorderSide, style) == 1 &&
                  sizeof(bb_box_segment) == 18 && offsetof(bb_box_segment, width) == 4 &&
                  offsetof(bb_box_segment, sides) == 8 && offsetof(bb_box_segment, shaded) == 16,
              "bb_box_side and bb_box_segment layout");
static_assert(offsetof(bb_chapter_settings, language) == 24 && sizeof(bb_chapter_settings) == 24 + 2 * sizeof(void*),
              "bb_chapter_settings layout");
static_assert(sizeof(bb_chapter_line) == 16 && offsetof(bb_chapter_line, base_height) == 8, "bb_chapter_line layout");

namespace {

ChapterHtmlSlimParser& parserOf(void* context) { return *static_cast<ChapterHtmlSlimParser*>(context); }

void reclaimMemory(const size_t bytes) { freeink::MemoryManager::instance().ensureFree(bytes); }

}  // namespace

ChapterHtmlSlimParser::ChapterHtmlSlimParser(
    std::shared_ptr<Epub> epub, const std::string& filepath, GfxRenderer& renderer, const int fontId,
    const float lineCompression, const bool extraParagraphSpacing, const uint8_t paragraphAlignment,
    const uint16_t viewportWidth, const uint16_t viewportHeight, const bool hyphenationEnabled,
    const bool focusReadingEnabled,
    const std::function<void(std::unique_ptr<Page>, uint16_t, uint16_t, uint32_t)>& completePageFn,
    const bool embeddedStyle, const std::string& contentBase, const std::string& imageBasePath,
    const uint8_t imageRendering, std::vector<std::string> tocAnchors, const std::function<void()>& popupFn,
    const CssParser* cssParser)
    : epub(std::move(epub)),
      filepath(filepath),
      renderer(renderer),
      completePageFn(completePageFn),
      popupFn(popupFn),
      cssParser(cssParser),
      imageBasePath(imageBasePath) {
  settings.font = fontId;
  settings.line_compression = lineCompression;
  settings.extra_paragraph_spacing = extraParagraphSpacing;
  settings.paragraph_alignment = paragraphAlignment;
  settings.viewport_width = viewportWidth;
  settings.viewport_height = viewportHeight;
  settings.hyphenation = hyphenationEnabled;
  settings.focus_reading = focusReadingEnabled;
  settings.embedded_style = embeddedStyle;
  settings.image_rendering = imageRendering;
  settings.paragraph_indent_spaces = 2;
  settings.character_spacing = 0;
  settings.word_spacing_percent = 100;
  settings.language = -1;
  settings.reclaim = reclaimMemory;
  chapter = bb_chapter_new(&settings);
  if (!chapter) {
    LOG_ERR("EHP", "OOM: chapter parser");
    return;
  }
  if (!bb_chapter_set_content_base(chapter, contentBase.data(), contentBase.size())) {
    LOG_ERR("EHP", "OOM: chapter parser");
    bb_chapter_free(chapter);
    chapter = nullptr;
    return;
  }
  for (const auto& anchor : tocAnchors) bb_chapter_add_toc_anchor(chapter, anchor.data(), anchor.size());
}

ChapterHtmlSlimParser::~ChapterHtmlSlimParser() {
  abortParse();
  bb_chapter_free(chapter);
}

void ChapterHtmlSlimParser::setTextSpacing(const int8_t character, const uint8_t wordPercent) {
  settings.character_spacing = character;
  settings.word_spacing_percent = wordPercent;
  bb_chapter_set_settings(chapter, &settings);
}

void ChapterHtmlSlimParser::setParagraphIndentSpaces(const uint8_t spaces) {
  settings.paragraph_indent_spaces = spaces;
  bb_chapter_set_settings(chapter, &settings);
}

// The Rust parser's side of the firmware. A page, box or line that cannot be made is reported, so
// the parser fails the build instead of losing text.
bb_chapter_host ChapterHtmlSlimParser::host() {
  bb_chapter_host h{};
  h.context = this;
  h.metrics = ParsedText::metricsOf(renderer);
  h.rules = cssParser ? cssParser->rules() : nullptr;
  h.new_page = [](void* c) {
    auto& self = parserOf(c);
    self.currentPage = makeUniqueNoThrow<Page>();
    if (!self.currentPage) LOG_ERR("EHP", "OOM: Page (%zu bytes)", sizeof(Page));
    return self.currentPage != nullptr;
  };
  h.split = [](void* c, const uint8_t count, const int16_t top) {
    auto& self = parserOf(c);
    self.nextPage = makeUniqueNoThrow<Page>();
    if (!self.nextPage) {
      LOG_ERR("EHP", "OOM: Page (%zu bytes)", sizeof(Page));
      return false;
    }
    if (count == 0) return true;
    auto& from = self.currentPage->elements;
    const auto first = from.end() - count;
    for (auto it = first; it != from.end(); ++it) {
      (*it)->yPos = static_cast<int16_t>((*it)->yPos - top);
      self.nextPage->elements.push_back(std::move(*it));
    }
    from.erase(first, from.end());
    self.currentPage->moveNotesBelow(top, *self.nextPage);
    return true;
  };
  h.emit = [](void* c, const uint32_t visibleOffset, const uint16_t paragraph, const uint16_t listItem) {
    auto& self = parserOf(c);
    self.completePageFn(std::move(self.currentPage), paragraph, listItem, visibleOffset);
    self.currentPage = std::move(self.nextPage);
  };
  h.add_box = [](void* c, const bb_box_segment* segment) {
    auto& self = parserOf(c);
    const auto side = [](const bb_box_side& s) { return CssBorderSide{s.width, static_cast<CssBorderStyle>(s.style)}; };
    const CssBorderSide sides[4] = {side(segment->sides[0]), side(segment->sides[1]), side(segment->sides[2]),
                                    side(segment->sides[3])};
    auto element = makeUniqueNoThrow<PageBorderBox>(segment->width, segment->height, sides, segment->shaded, segment->x,
                                                    segment->y);
    if (!element) {
      LOG_ERR("EHP", "OOM: PageBorderBox");
      return false;
    }
    self.currentPage->elements.push_back(std::move(element));
    return true;
  };
  h.make_line = [](void* c, const bb_line* line, const bb_css_block* block, const char* const* targets,
                   bb_chapter_line* out) {
    auto& self = parserOf(c);
    auto text = ParsedText::makeLine(*line, *reinterpret_cast<const BlockStyle*>(block), targets);
    if (!text) return false;
    const int sectionFont = self.settings.font;
    const int lineFont = text->blockFontId(sectionFont);
    KeptLine kept;
    kept.rubyShift = text->getRubyShift(self.renderer.getFontAscenderSize(lineFont));
    // Taller inline words push the shared baseline down.
    kept.baseHeight = self.renderer.getLineHeight(lineFont, self.settings.line_compression) +
                      text->extraAscent(self.renderer, sectionFont);
    out->words = text->wordCount();
    out->base_height = kept.baseHeight;
    out->ruby_shift = kept.rubyShift;
    kept.line = std::move(text);
    if (!self.freeLines.empty()) {
      out->handle = self.freeLines.back();
      self.freeLines.pop_back();
      self.lines[out->handle] = std::move(kept);
    } else {
      out->handle = static_cast<uint32_t>(self.lines.size());
      self.lines.push_back(std::move(kept));
    }
    return true;
  };
  h.set_line_left = [](void* c, const uint32_t handle, const int16_t x) {
    auto& line = parserOf(c).lines[handle].line;
    if (!line) return;
    auto style = line->getBlockStyle();
    style.marginLeft = x;
    line->setBlockStyle(style);
  };
  h.push_line = [](void* c, const uint32_t handle, const int16_t y) {
    auto& self = parserOf(c);
    KeptLine kept = std::move(self.lines[handle]);
    self.freeLines.push_back(handle);
    if (!kept.line || !self.currentPage) return false;
    // The block's left inset (margin and padding) is the line's x.
    const int16_t x = kept.line->getBlockStyle().leftInset();
    for (const auto& link : kept.line->takeLinkSpans()) {
      if (!self.currentPage->addLink(link.href, static_cast<int16_t>(x + link.x),
                                     static_cast<int16_t>(y + kept.rubyShift - link.topLift), link.width,
                                     static_cast<int16_t>(kept.baseHeight + link.topLift))) {
        LOG_DBG("EHP", "Dropped page link: %.48s", link.href);
      }
    }
    auto pageLine = makeUniqueNoThrow<PageLine>(std::move(kept.line), x, y);
    if (!pageLine) {
      LOG_ERR("EHP", "OOM: PageLine");
      return false;
    }
    self.currentPage->elements.push_back(std::move(pageLine));
    return true;
  };
  h.drop_line = [](void* c, const uint32_t handle) {
    auto& self = parserOf(c);
    self.lines[handle] = KeptLine{};
    self.freeLines.push_back(handle);
  };
  h.push_rule = [](void* c, const int16_t width, const uint8_t thickness, const int16_t x, const int16_t y) {
    auto& self = parserOf(c);
    auto rule = makeUniqueNoThrow<PageHorizontalRule>(width, thickness, x, y);
    if (!rule) {
      LOG_ERR("EHP", "OOM: PageHorizontalRule");
      return false;
    }
    self.currentPage->elements.push_back(std::move(rule));
    return true;
  };
  h.push_drop_cap = [](void* c, const int32_t font, const uint16_t scale, const bool bold, const char* text,
                       const int16_t x, const int16_t y) {
    auto& self = parserOf(c);
    auto cap =
        makeUniqueNoThrow<PageDropCap>(font, scale, bold ? EpdFontFamily::BOLD : EpdFontFamily::REGULAR, text, x, y);
    if (!cap) {
      LOG_ERR("EHP", "OOM: PageDropCap");
      return false;
    }
    self.currentPage->elements.push_back(std::move(cap));
    return true;
  };
  h.push_image = [](void* c, const int32_t width, const int32_t height, const int32_t x, const int16_t y) {
    auto& self = parserOf(c);
    auto image = makeUniqueNoThrow<ImageBlock>(self.imagePath, self.imageSource, width, height);
    if (!image) {
      LOG_ERR("EHP", "Failed to create ImageBlock");
      return false;
    }
    auto pageImage = makeUniqueNoThrow<PageImage>(std::move(image), x, y);
    if (!pageImage) {
      LOG_ERR("EHP", "Failed to create PageImage");
      return false;
    }
    self.currentPage->elements.push_back(std::move(pageImage));
    return true;
  };
  h.add_footnote = [](void* c, const char* number, const char* href) {
    auto& self = parserOf(c);
    if (self.currentPage) self.currentPage->addFootnote(number, href);
  };
  h.line_height = [](void* c, const int32_t font, const float compression) {
    return parserOf(c).renderer.getLineHeight(font, compression);
  };
  h.font_variant = [](void* c, const int32_t font, const float scale) {
    return static_cast<int32_t>(parserOf(c).renderer.resolveFontVariant(font, scale));
  };
  h.glyph = [](void* c, const int32_t font, const uint32_t cp, const bool bold, int32_t* advance, int* top) {
    return parserOf(c).renderer.getCodepointMetrics(font, cp, bold ? EpdFontFamily::BOLD : EpdFontFamily::REGULAR,
                                                    *advance, *top);
  };
  h.read = [](void* c, uint8_t* buf, const size_t len, bool* more) {
    auto& self = parserOf(c);
    const int n = self.parseFile_.read(buf, len);
    *more = self.parseFile_.available() > 0;
    // A read error is nothing read, which with more to come stops the parse.
    return n > 0 ? static_cast<size_t>(n) : 0;
  };
  h.table_open = [](void* c, const uint64_t offset) {
    auto& self = parserOf(c);
    if (!self.parsing_) return false;
    if (self.tableFile_.isOpen()) self.tableFile_.close();
    if (!Storage.openFileForRead("EHP", self.filepath, self.tableFile_)) return false;
    if (self.tableFile_.seek(static_cast<size_t>(offset))) return true;
    self.tableFile_.close();
    return false;
  };
  h.table_read = [](void* c, uint8_t* buf, const size_t len) -> size_t {
    const int count = parserOf(c).tableFile_.read(buf, len);
    return count > 0 ? static_cast<size_t>(count) : 0;
  };
  h.table_close = [](void* c) {
    auto& self = parserOf(c);
    if (self.tableFile_.isOpen()) self.tableFile_.close();
  };
  h.table_word = [](void* c, const char* word, const bool bold) {
    auto& self = parserOf(c);
    const auto style = bold ? EpdFontFamily::BOLD : EpdFontFamily::REGULAR;
    const int font = self.settings.font;
    if (self.renderer.isSdCardFont(font)) {
      self.renderer.ensureSdCardFontReady(font, word, static_cast<uint8_t>(1u << style));
    }
    return self.renderer.getTextAdvanceX(font, word, style);
  };
  h.image = [](void* c, const char* path, const size_t len, int16_t* width, int16_t* height) {
    auto& self = parserOf(c);
    const std::string resolvedPath(path, len);
    if (!ImageDecoderFactory::isFormatSupported(resolvedPath)) return 0;
    // A unique name for the cached image.
    std::string ext;
    const size_t extPos = resolvedPath.rfind('.');
    if (extPos != std::string::npos) ext = resolvedPath.substr(extPos);
    std::string cachedImagePath = self.imageBasePath + std::to_string(self.imageCounter++) + ext;

    // The dimensions from the entry's first bytes (an early-aborted inflate, a few KB); the image
    // itself is extracted at the page's first render (ImageBlock's lazy extractor), so first-open
    // of an image-heavy chapter does not stall for seconds per image.
    ImageDimensions dims = {0, 0};
    ImageDimsProbe headerProbe;
    self.epub->readItemContentsToStream(resolvedPath, headerProbe, 1024, /*allowEarlyStop=*/true);
    bool gotDimensions = headerProbe.getDimensions(dims);
    if (!gotDimensions) {
      // Retry with framebuffer scratch when the heap cannot fit the inflate window.
      GfxRenderer::FrameBufferLoan probeLoan(self.renderer);
      ImageDimsProbe retryProbe;
      self.epub->readItemContentsToStream(resolvedPath, retryProbe, 1024, /*allowEarlyStop=*/true);
      gotDimensions = retryProbe.getDimensions(dims);
    }
    if (!gotDimensions) {
      // No header in the stream (rare): extract the whole image and probe the file. That can take
      // seconds, so the indexing popup comes first (once per parser).
      if (self.popupFn && !self.imagePopupFired) {
        self.imagePopupFired = true;
        self.popupFn();
      }
      HalFile cachedImageFile;
      bool extractSuccess = false;
      if (Storage.openFileForWrite("EHP", cachedImagePath, cachedImageFile)) {
        {
          // Same 32 KB inflate window as the probe; the popup is already up.
          GfxRenderer::FrameBufferLoan extractLoan(self.renderer);
          extractSuccess = self.epub->readItemContentsToStream(resolvedPath, cachedImageFile, 4096);
        }
        cachedImageFile.flush();
        cachedImageFile.close();
      }
      if (extractSuccess) {
        // Retries absorb SD-card sync latency on slow cards.
        ImageToFramebufferDecoder* decoder = ImageDecoderFactory::getDecoder(cachedImagePath);
        for (int attempt = 0; attempt < 3 && !gotDimensions; attempt++) {
          if (attempt > 0) delay(50);
          gotDimensions = decoder && decoder->getDimensions(cachedImagePath, dims);
        }
      } else {
        LOG_ERR("EHP", "Failed to extract image");
      }
    }
    if (!gotDimensions) {
      LOG_ERR("EHP", "Failed to get image dimensions");
      Storage.remove(cachedImagePath.c_str());
      return 1;
    }
    *width = dims.width;
    *height = dims.height;
    self.imagePath = std::move(cachedImagePath);
    self.imageSource = resolvedPath;
    return 2;
  };
  return h;
}

bool ChapterHtmlSlimParser::beginParse() {
  if (!chapter) return false;
  currentPage.reset();
  nextPage.reset();
  lines.clear();
  freeLines.clear();
  settings.language = Hyphenator::language();
  bb_chapter_set_settings(chapter, &settings);
  const bb_chapter_host h = host();
  if (!bb_chapter_begin(chapter, &h)) {
    LOG_ERR("EHP", "Couldn't allocate memory for parser");
    return false;
  }
  if (!Storage.openFileForRead("EHP", filepath, parseFile_)) {
    bb_chapter_abort(chapter);
    return false;
  }
  parsing_ = true;
  // Big chapters show the indexing popup.
  if (popupFn && parseFile_.size() >= MIN_SIZE_FOR_POPUP) popupFn();
  return true;
}

ChapterHtmlSlimParser::ParseStatus ChapterHtmlSlimParser::parseStep() {
  const bb_chapter_host h = host();
  switch (bb_chapter_step(chapter, &h)) {
    case BB_CHAPTER_MORE:
      return ParseStatus::More;
    case BB_CHAPTER_DONE:
      return ParseStatus::Done;
    default:
      LOG_ERR("EHP", "Chapter parse failed (malformed XHTML, a read error or out of memory)");
      return ParseStatus::Error;
  }
}

void ChapterHtmlSlimParser::abortParse() {
  bb_chapter_abort(chapter);
  parsing_ = false;
  if (parseFile_.isOpen()) parseFile_.close();
  if (tableFile_.isOpen()) tableFile_.close();
}

bool ChapterHtmlSlimParser::finishParse() {
  parsing_ = false;
  if (parseFile_.isOpen()) parseFile_.close();
  if (tableFile_.isOpen()) tableFile_.close();
  const bb_chapter_host h = host();
  if (!bb_chapter_finish(chapter, &h)) {
    LOG_ERR("EHP", "Text layout dropped content (OOM); failing section build");
    return false;
  }
  currentPage.reset();
  return true;
}

bool ChapterHtmlSlimParser::parseAndBuildPages() {
  if (!beginParse()) return false;
  for (;;) {
    const ParseStatus status = parseStep();
    if (status == ParseStatus::Error) {
      abortParse();
      return false;
    }
    if (status == ParseStatus::Done) break;
  }
  return finishParse();
}

size_t ChapterHtmlSlimParser::anchorCount() const { return bb_anchors_len(bb_chapter_anchors(chapter)); }

std::string_view ChapterHtmlSlimParser::anchorAt(const size_t i, uint16_t& page) const {
  size_t len = 0;
  const char* name = bb_anchors_get(bb_chapter_anchors(chapter), i, &len, &page);
  return name ? std::string_view(name, len) : std::string_view();
}

std::optional<uint16_t> ChapterHtmlSlimParser::findAnchor(const std::string_view name) const {
  uint16_t page = 0;
  if (!bb_anchors_find(bb_chapter_anchors(chapter), name.data(), name.size(), &page)) return std::nullopt;
  return page;
}

void ChapterHtmlSlimParser::startElement(const char* name, const char** atts) {
  const bb_chapter_host h = host();
  bb_chapter_start_element(chapter, &h, name, atts);
}

void ChapterHtmlSlimParser::characterData(const char* text, const size_t len) {
  const bb_chapter_host h = host();
  bb_chapter_text(chapter, &h, text, len);
}

void ChapterHtmlSlimParser::endElement(const char* name) {
  const bb_chapter_host h = host();
  bb_chapter_end_element(chapter, &h, name);
}
