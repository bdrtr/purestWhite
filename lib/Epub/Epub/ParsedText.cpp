#include "ParsedText.h"

#include <GfxRenderer.h>
#include <Logging.h>
#include <Memory.h>
#include <MemoryManager.h>

#include <cmath>
#include <cstring>

#include "hyphenation/Hyphenator.h"

// layout.h and bembeyaz-layout spell these values out.
static_assert(TextBlock::MAX_WORD_FONTS == 3, "bb_line::word_fonts and bb_layout_params::slot_fonts");
static_assert(TextBlock::DISCRETIONARY_HYPHEN_FLAG == 0x80 && TextBlock::SMALL_CAPS_SLOT_FLAG == 0x80);
static_assert(EpdFontFamily::BOLD == 1 && EpdFontFamily::UNDERLINE == 4 && EpdFontFamily::SUP == 16 &&
              EpdFontFamily::RUBY_CONTINUE == 64 && EpdFontFamily::SMALL_CAPS == 128);
static_assert(static_cast<int>(CssTextAlign::Justify) == 0 && static_cast<int>(CssTextAlign::Left) == 1 &&
              static_cast<int>(CssTextAlign::Center) == 2 && static_cast<int>(CssTextAlign::Right) == 3 &&
              static_cast<int>(CssTextAlign::None) == 4);
static_assert(sizeof(bb_link_span) == 8, "bembeyaz_layout::LinkSpan");

namespace {

const GfxRenderer& rendererOf(void* context) { return *static_cast<const GfxRenderer*>(context); }

EpdFontFamily::Style styleOf(const uint8_t style) { return static_cast<EpdFontFamily::Style>(style); }

}  // namespace

struct ParsedText::LayoutContext {
  ParsedText* text;
  const std::function<void(std::unique_ptr<TextBlock>, uint32_t)>* processLine;
};

void ParsedText::reclaim(const size_t bytes) { freeink::MemoryManager::instance().ensureFree(bytes); }

void ParsedText::addWord(std::string word, const EpdFontFamily::Style fontStyle, const bool underline,
                         const bool attachToPrevious, const uint32_t visibleTextOffset, const uint8_t linkId,
                         const uint8_t sizeSlot, const bool noSpaceBefore) {
  const bool wasDropped = hadDroppedWords();
  bb_paragraph_add_word(paragraph, word.data(), word.size(), fontStyle, underline, attachToPrevious, visibleTextOffset,
                        linkId, sizeSlot, noSpaceBefore);
  if (!wasDropped && hadDroppedWords()) {
    LOG_ERR("PTX", "OOM: dropping paragraph text");
  }
}

uint8_t ParsedText::addLinkTarget(const char* href) {
  if (!href || href[0] == '\0' || strnlen(href, FOOTNOTE_HREF_LEN) >= FOOTNOTE_HREF_LEN ||
      linkTargets.size() >= UINT8_MAX) {
    return 0;
  }
  linkTargets.emplace_back(href);
  return static_cast<uint8_t>(linkTargets.size());
}

bool ParsedText::linkTargetMatches(const uint8_t linkId, const char* href) const {
  return linkId > 0 && linkId <= linkTargets.size() && href && linkTargets[linkId - 1] == href;
}

uint8_t ParsedText::sizeSlotFor(const float scale) {
  uint8_t nearest = 1;
  for (uint8_t slot = 1; slot < sizeSlotCount; ++slot) {
    if (std::fabs(sizeSlotScales[slot] - scale) < 0.01f) return slot;
    if (std::fabs(sizeSlotScales[slot] - scale) < std::fabs(sizeSlotScales[nearest] - scale)) nearest = slot;
  }
  if (sizeSlotCount < MAX_SIZE_SLOTS) {
    sizeSlotScales[sizeSlotCount] = scale;
    return sizeSlotCount++;
  }
  return nearest;
}

// Builds the line's TextBlock and hands it on; false drops the line (out of memory).
bool ParsedText::emitLine(void* context, const bb_line* line) {
  const auto& layout = *static_cast<LayoutContext*>(context);
  ParsedText& self = *layout.text;
  self.blockStyle.isRtl = line->is_rtl;

  const size_t count = line->count;
  std::vector<std::string> words;
  std::vector<EpdFontFamily::Style> styles;
  std::vector<std::string> ruby;
  std::vector<TextBlock::SourceRange> ranges;
  words.reserve(count);
  styles.reserve(count);
  ruby.reserve(count);
  ranges.reserve(count);
  for (size_t i = 0; i < count; ++i) {
    words.emplace_back(line->text + line->word_starts[i]);
    styles.push_back(styleOf(line->styles[i]));
    ruby.emplace_back(line->ruby_text + line->ruby_starts[i]);
    ranges.push_back({line->ranges[2 * i], line->ranges[2 * i + 1]});
  }
  const std::vector<int16_t> xpos(line->xpos, line->xpos + count);
  std::vector<uint8_t> focus;
  std::vector<uint16_t> suffixX;
  if (line->focus) {
    focus.assign(line->focus, line->focus + count);
    suffixX.assign(line->suffix_x, line->suffix_x + count);
  }
  std::vector<TextBlock::LinkSpan> links(line->link_count);
  for (size_t i = 0; i < line->link_count; ++i) {
    const bb_link_span& source = line->links[i];
    auto& span = links[i];
    strncpy(span.href, self.linkTargets[source.link - 1].c_str(), sizeof(span.href) - 1);
    span.href[sizeof(span.href) - 1] = '\0';
    span.x = source.x;
    span.width = source.width;
    span.topLift = source.top_lift;
  }

  auto block = makeUniqueNoThrow<TextBlock>(words, xpos, styles, focus, suffixX, self.blockStyle, std::move(ruby),
                                            std::move(links), ranges, line->paragraph_start);
  int32_t fonts[TextBlock::MAX_WORD_FONTS];
  std::copy(line->word_fonts, line->word_fonts + TextBlock::MAX_WORD_FONTS, fonts);
  if (!block || !block->valid() || (line->word_slots && !block->setWordFonts(line->word_slots, fonts))) {
    LOG_ERR("PTX", "Dropping line: TextBlock or arena allocation failed");
    return false;
  }
  self.linesAfterCurrent = line->lines_after;
  (*layout.processLine)(std::move(block), line->visible_offset);
  return true;
}

void ParsedText::layoutAndExtractLines(const GfxRenderer& renderer, const int fontId, const uint16_t viewportWidth,
                                       const std::function<void(std::unique_ptr<TextBlock>, uint32_t)>& processLine,
                                       const bool includeLastLine, const int8_t characterSpacing,
                                       const uint8_t wordSpacingPercent, const size_t maxLines) {
  if (isEmpty()) return;
  // Stamped here rather than at construction: the parser replaces blockStyle as CSS resolves.
  blockStyle.characterSpacing = characterSpacing;

  const bb_metrics metrics = {
      const_cast<GfxRenderer*>(&renderer),
      [](void* c, const int font, const char* text, const uint8_t style, const int8_t spacing) {
        return rendererOf(c).getTextAdvanceX(font, text, styleOf(style), spacing);
      },
      [](void* c, const int font, const uint32_t left, const uint32_t right, const uint8_t style,
         const int8_t spacing) { return rendererOf(c).getKerning(font, left, right, styleOf(style), spacing); },
      [](void* c, const int font, const uint32_t left, const uint32_t right, const uint8_t style) {
        return rendererOf(c).getSpaceAdvance(font, left, right, styleOf(style));
      },
      [](void* c, const int font, const uint8_t style) { return rendererOf(c).getSpaceWidth(font, styleOf(style)); },
      [](void* c, const int font) { return rendererOf(c).getFontAscenderSize(font); },
      [](void* c, const int font) { return rendererOf(c).isSdCardFont(font); },
      [](void* c, const int font, const char* const* segments, const size_t* lens, const size_t count, const bool space,
         const bool hyphen, const uint8_t mask, const bool smallCaps) {
        rendererOf(c).ensureSdCardFontReady(font, segments, lens, count, space, hyphen, mask, smallCaps);
      },
  };
  bb_block_style style = {static_cast<uint8_t>(blockStyle.alignment),
                          blockStyle.textAlignDefined,
                          blockStyle.directionDefined,
                          blockStyle.isRtl,
                          blockStyle.textIndentDefined,
                          blockStyle.textIndent};
  bb_layout_params params = {};
  params.font = fontId;
  std::copy(sizeSlotFontIds, sizeSlotFontIds + MAX_SIZE_SLOTS, params.slot_fonts);
  params.slot_count = sizeSlotCount;
  params.width = viewportWidth;
  params.include_last_line = includeLastLine;
  params.character_spacing = characterSpacing;
  params.word_spacing = wordSpacingPercent;
  params.max_lines = maxLines;
  params.language = Hyphenator::language();
  params.link_count = linkTargets.size();

  LayoutContext context = {this, &processLine};
  bb_paragraph_layout(paragraph, &metrics, &style, &params, emitLine, &context);
  blockStyle.isRtl = style.is_rtl;
  linesAfterCurrent = -1;
}
