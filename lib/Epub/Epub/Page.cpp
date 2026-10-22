#include "Page.h"

#include <GfxRenderer.h>
#include <Logging.h>
#include <Memory.h>
#include <Utf8.h>
#include <bembeyaz/page.h>

#include <cstddef>

#include "SectionFileIo.h"

namespace {

template <typename Predicate>
void renderFilteredPageElements(const std::vector<std::unique_ptr<PageElement>>& elements, GfxRenderer& renderer,
                                const int fontId, const int xOffset, const int yOffset, Predicate&& predicate) {
  for (const auto& element : elements) {
    if (predicate(*element)) {
      element->render(renderer, fontId, xOffset, yOffset);
    }
  }
}

bool isBorderBox(const PageElement& element) { return element.getTag() == TAG_PageBorderBox; }

void renderBoxBackgrounds(const std::vector<std::unique_ptr<PageElement>>& elements, GfxRenderer& renderer,
                          const int xOffset, const int yOffset) {
  for (const auto& element : elements) {
    if (isBorderBox(*element)) {
      static_cast<const PageBorderBox&>(*element).renderBackground(renderer, xOffset, yOffset);
    }
  }
}

// Draws one border edge as a strip along a box side.
void drawBorderEdge(const GfxRenderer& renderer, const int x, const int y, const int length, const CssBorderSide& side,
                    const bool horizontal) {
  const int thickness = side.width;
  const auto strip = [&](const int offset, const int along, const int span, const int depth) {
    if (horizontal) {
      renderer.fillRect(x + along, y + offset, span, depth, true);
    } else {
      renderer.fillRect(x + offset, y + along, depth, span, true);
    }
  };
  switch (side.style) {
    case CssBorderStyle::Double:
      if (thickness >= 3) {
        const int line = std::max(1, thickness / 3);
        strip(0, 0, length, line);
        strip(thickness - line, 0, length, line);
        return;
      }
      break;
    case CssBorderStyle::Dotted:
    case CssBorderStyle::Dashed: {
      const int dash = side.style == CssBorderStyle::Dotted ? thickness : std::max(3, thickness * 3);
      const int gap = side.style == CssBorderStyle::Dotted ? std::max(2, thickness) : std::max(2, thickness * 2);
      for (int along = 0; along < length; along += dash + gap)
        strip(0, along, std::min(dash, length - along), thickness);
      return;
    }
    default:
      break;
  }
  strip(0, 0, length, thickness);
}

}  // namespace

void PageLine::render(GfxRenderer& renderer, const int fontId, const int xOffset, const int yOffset) {
  block->render(renderer, fontId, xPos + xOffset, yPos + yOffset);
}

void PageImage::render(GfxRenderer& renderer, const int fontId, const int xOffset, const int yOffset) {
  // Images don't use fontId or text rendering
  imageBlock->render(renderer, xPos + xOffset, yPos + yOffset);
}

void PageImage::renderPlaceholder(GfxRenderer& renderer, const int xOffset, const int yOffset) const {
  imageBlock->renderPlaceholder(renderer, xPos + xOffset, yPos + yOffset);
}

void PageHorizontalRule::render(GfxRenderer& renderer, const int fontId, const int xOffset, const int yOffset) {
  (void)fontId;
  if (width == 0 || thickness == 0) {
    return;
  }

  renderer.drawLine(xPos + xOffset, yPos + yOffset, xPos + xOffset + width - 1, yPos + yOffset, thickness, true);
}

PageDropCap::PageDropCap(const int32_t fontId, const uint16_t scale256, const EpdFontFamily::Style style,
                         const char* utf8, const int16_t xPos, const int16_t yPos)
    : PageElement(xPos, yPos), fontId(fontId), scale256(scale256), style(style) {
  strncpy(text, utf8, MAX_TEXT_BYTES);
}

void PageDropCap::render(GfxRenderer& renderer, const int sectionFontId, const int xOffset, const int yOffset) {
  // A sized variant that can no longer be loaded draws from the section font instead.
  const int drawFontId = renderer.ensureFontLoaded(fontId) ? fontId : sectionFontId;
  int x = xPos + xOffset;
  const auto* cursor = reinterpret_cast<const unsigned char*>(text);
  while (const uint32_t cp = utf8NextCodepoint(&cursor)) {
    x += renderer.drawScaledCodepoint(drawFontId, cp, style, x, yPos + yOffset, scale256);
  }
}

void PageBorderBox::renderBackground(GfxRenderer& renderer, const int xOffset, const int yOffset) const {
  if (shaded) renderer.fillRectDither(xPos + xOffset, yPos + yOffset, width, height, Color::LightGray);
}

void PageBorderBox::render(GfxRenderer& renderer, const int, const int xOffset, const int yOffset) {
  const int x = xPos + xOffset;
  const int y = yPos + yOffset;
  const CssBorderSide& top = sides[0];
  const CssBorderSide& right = sides[1];
  const CssBorderSide& bottom = sides[2];
  const CssBorderSide& left = sides[3];
  if (top.visible()) drawBorderEdge(renderer, x, y, width, top, true);
  if (bottom.visible()) drawBorderEdge(renderer, x, y + height - bottom.width, width, bottom, true);
  if (left.visible()) drawBorderEdge(renderer, x, y, height, left, false);
  if (right.visible()) drawBorderEdge(renderer, x + width - right.width, y, height, right, false);
}

// Backgrounds precede every border, including borders inside shaded containers.
void Page::render(GfxRenderer& renderer, const int fontId, const int xOffset, const int yOffset) const {
  renderBoxBackgrounds(elements, renderer, xOffset, yOffset);
  renderFilteredPageElements(elements, renderer, fontId, xOffset, yOffset, isBorderBox);
  renderFilteredPageElements(elements, renderer, fontId, xOffset, yOffset,
                             [](const PageElement& element) { return !isBorderBox(element); });
}

void Page::renderImages(GfxRenderer& renderer, const int fontId, const int xOffset, const int yOffset) const {
  renderFilteredPageElements(elements, renderer, fontId, xOffset, yOffset,
                             [](const PageElement& element) { return element.getTag() == TAG_PageImage; });
}

void Page::renderWithImagePlaceholders(GfxRenderer& renderer, const int fontId, const int xOffset,
                                       const int yOffset) const {
  renderBoxBackgrounds(elements, renderer, xOffset, yOffset);
  renderFilteredPageElements(elements, renderer, fontId, xOffset, yOffset, isBorderBox);
  for (const auto& element : elements) {
    if (isBorderBox(*element)) continue;
    if (element->getTag() == TAG_PageImage) {
      static_cast<const PageImage&>(*element).renderPlaceholder(renderer, xOffset, yOffset);
    } else {
      element->render(renderer, fontId, xOffset, yOffset);
    }
  }
}

namespace {
// Footnotes and links are read into, and written from, the page's own vectors.
static_assert(sizeof(FootnoteEntry) == sizeof(bb_page_footnote) &&
                  offsetof(FootnoteEntry, href) == offsetof(bb_page_footnote, href),
              "FootnoteEntry is a bb_page_footnote");
static_assert(sizeof(PageLink) == sizeof(bb_page_link) && offsetof(PageLink, x) == offsetof(bb_page_link, x) &&
                  offsetof(PageLink, y) == offsetof(bb_page_link, y) &&
                  offsetof(PageLink, width) == offsetof(bb_page_link, width) &&
                  offsetof(PageLink, height) == offsetof(bb_page_link, height),
              "PageLink is a bb_page_link");
static_assert(Page::MAX_FOOTNOTES_PER_PAGE == BB_PAGE_MAX_FOOTNOTES && Page::MAX_LINKS_PER_PAGE == BB_PAGE_MAX_LINKS &&
                  PageDropCap::MAX_TEXT_BYTES == BB_PAGE_MAX_DROP_CAP &&
                  TextBlock::MAX_WORD_FONTS == BB_PAGE_WORD_FONTS,
              "bb_page limits");

// Makes a page's elements from bembeyaz-section's read.
struct PageBuilder {
  Page& page;
  TextBlock::Decoder line;
  int16_t lineX = 0;
  int16_t lineY = 0;
  std::string imagePath;
  std::string imageSource;

  template <typename Element, typename... Args>
  bool add(Args&&... args) {
    auto element = makeUniqueNoThrow<Element>(std::forward<Args>(args)...);
    if (!element) {
      LOG_ERR("PGE", "Deserialization failed: could not allocate an element");
      return false;
    }
    page.elements.push_back(std::move(element));
    return true;
  }
};

PageBuilder& builderOf(void* context) { return *static_cast<PageBuilder*>(context); }

bb_page_builder pageBuilder(PageBuilder& page) {
  bb_page_builder b{};
  b.context = &page;
  b.begin = [](void* c, const uint16_t count) {
    // One allocation for the element vector instead of a grow per doubling. The count comes off the
    // SD card, so it is capped: a real page holds a few dozen elements, and a failed vector
    // allocation would abort (the firmware builds without exceptions).
    static constexpr uint16_t RESERVE_CAP = 256;
    builderOf(c).page.elements.reserve(std::min(count, RESERVE_CAP));
  };
  b.line = [](void* c, const int16_t x, const int16_t y, const bb_page_text* text, const size_t size, uint8_t** arena) {
    auto& p = builderOf(c);
    p.lineX = x;
    p.lineY = y;
    return p.line.begin(*text, size, arena);
  };
  b.ruby = [](void* c, const uint16_t word, const size_t len) { return builderOf(c).line.ruby(word, len); };
  b.word_fonts = [](void* c, const size_t size) { return builderOf(c).line.wordFonts(size); };
  b.line_end = [](void* c, const bb_page_block_style* style) {
    auto& p = builderOf(c);
    auto block = p.line.finish(*style);
    return block && p.add<PageLine>(std::move(block), p.lineX, p.lineY);
  };
  b.image_text = [](void* c, const bool source, const size_t len) {
    auto& text = source ? builderOf(c).imageSource : builderOf(c).imagePath;
    text.resize(len);
    return text.data();
  };
  b.image = [](void* c, const int16_t x, const int16_t y, const int16_t width, const int16_t height) {
    auto& p = builderOf(c);
    auto block = makeUniqueNoThrow<ImageBlock>(std::move(p.imagePath), std::move(p.imageSource), width, height);
    p.imagePath = {};
    p.imageSource = {};
    if (!block) {
      LOG_ERR("PGE", "Deserialization failed: could not allocate ImageBlock");
      return false;
    }
    return p.add<PageImage>(std::move(block), x, y);
  };
  b.rule = [](void* c, const int16_t x, const int16_t y, const uint16_t width, const uint8_t thickness) {
    return builderOf(c).add<PageHorizontalRule>(width, thickness, x, y);
  };
  b.drop_cap = [](void* c, const int16_t x, const int16_t y, const int32_t fontId, const uint16_t scale256,
                  const uint8_t style, const char* text, const size_t len) {
    char terminated[PageDropCap::MAX_TEXT_BYTES + 1] = {};
    memcpy(terminated, text, std::min(len, PageDropCap::MAX_TEXT_BYTES));
    return builderOf(c).add<PageDropCap>(fontId, scale256, static_cast<EpdFontFamily::Style>(style), terminated, x, y);
  };
  b.border_box = [](void* c, const int16_t x, const int16_t y, const uint16_t width, const uint16_t height,
                    const bb_page_side* stored, const bool shaded) {
    CssBorderSide sides[4];
    for (int i = 0; i < 4; ++i) sides[i] = {stored[i].width, static_cast<CssBorderStyle>(stored[i].style)};
    return builderOf(c).add<PageBorderBox>(width, height, sides, shaded, x, y);
  };
  b.footnotes = [](void* c, const size_t count) {
    auto& footnotes = builderOf(c).page.footnotes;
    footnotes.resize(count);
    return reinterpret_cast<bb_page_footnote*>(footnotes.data());
  };
  b.links = [](void* c, const size_t count) {
    auto& links = builderOf(c).page.links;
    links.resize(count);
    return reinterpret_cast<bb_page_link*>(links.data());
  };
  return b;
}
}  // namespace

bool Page::serialize(HalFile& file) const {
  const bb_section_sink sink = section_file_io::sinkOf(file);
  auto at = static_cast<uint32_t>(file.position());
  bool ok = bb_page_write_count(&sink, &at, static_cast<uint16_t>(elements.size()));
  for (const auto& element : elements) {
    if (!ok) break;
    const int16_t x = element->xPos;
    const int16_t y = element->yPos;
    switch (element->getTag()) {
      case TAG_PageLine: {
        const TextBlock& block = *static_cast<const PageLine&>(*element).getBlock();
        if (!block.valid()) {
          LOG_ERR("PGE", "Serialization failed: invalid text block");
          return false;
        }
        const bb_page_line line = block.encoderView();
        ok = bb_page_write_line(&sink, &at, x, y, &line);
        break;
      }
      case TAG_PageImage: {
        const ImageBlock& image = static_cast<const PageImage&>(*element).getImageBlock();
        ok = bb_page_write_image(&sink, &at, x, y, image.getImagePath().data(), image.getImagePath().size(),
                                 image.getSrcPath().data(), image.getSrcPath().size(), image.getWidth(),
                                 image.getHeight());
        break;
      }
      case TAG_PageHorizontalRule: {
        const auto& rule = static_cast<const PageHorizontalRule&>(*element);
        ok = bb_page_write_rule(&sink, &at, x, y, rule.getWidth(), rule.getThickness());
        break;
      }
      case TAG_PageDropCap: {
        const auto& dropCap = static_cast<const PageDropCap&>(*element);
        ok = bb_page_write_drop_cap(&sink, &at, x, y, dropCap.getFontId(), dropCap.getScale256(),
                                    static_cast<uint8_t>(dropCap.getStyle()), dropCap.getText(),
                                    strnlen(dropCap.getText(), PageDropCap::MAX_TEXT_BYTES));
        break;
      }
      case TAG_PageBorderBox: {
        const auto& box = static_cast<const PageBorderBox&>(*element);
        bb_page_side sides[4];
        for (int i = 0; i < 4; ++i) {
          sides[i] = {box.getSides()[i].width, static_cast<uint8_t>(box.getSides()[i].style)};
        }
        ok = bb_page_write_border_box(&sink, &at, x, y, box.getWidth(), box.getHeight(), sides, box.isShaded());
        break;
      }
    }
  }
  ok = ok && bb_page_write_notes(&sink, &at, reinterpret_cast<const bb_page_footnote*>(footnotes.data()),
                                 footnotes.size(), reinterpret_cast<const bb_page_link*>(links.data()), links.size());
  if (!ok) {
    LOG_ERR("PGE", "Serialization failed: write");
  }
  return ok;
}

std::unique_ptr<Page> Page::deserialize(HalFile& file) {
  auto page = makeUniqueNoThrow<Page>();
  if (!page) {
    LOG_ERR("PGE", "Deserialization failed: could not allocate Page");
    return nullptr;
  }
  PageBuilder target{*page};
  const bb_page_builder builder = pageBuilder(target);
  const bb_section_file view = section_file_io::fileOf(file);
  uint32_t end = 0;
  const int result = bb_page_read(&view, static_cast<uint32_t>(file.position()), &builder, &end);
  if (result != BB_PAGE_READ) {
    LOG_ERR("PGE", "Deserialization failed: %s", bb_page_error(result));
    return nullptr;
  }
  // The read reads ahead; the file is left just past the page.
  file.seek(end);
  return page;
}
