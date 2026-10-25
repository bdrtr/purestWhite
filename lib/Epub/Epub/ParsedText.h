#pragma once

#include <EpdFontFamily.h>
#include <bembeyaz/layout.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <vector>

#include "blocks/BlockStyle.h"
#include "blocks/TextBlock.h"

class GfxRenderer;

// One paragraph's words and their layout into lines: rust/crates/bembeyaz-layout. The block
// style, link targets and inline size slots stay here; the words and layout are Rust's.
class ParsedText {
  bb_paragraph* paragraph;
  BlockStyle blockStyle;
  std::vector<std::string> linkTargets;
  // Words sized by an inline font-size span: a slot per word into sizeSlotScales (absolute
  // multiples of the reader's body size). Slot 0 follows the block's own size; the caller
  // sets each slot's font before layout.
  static constexpr uint8_t MAX_SIZE_SLOTS = TextBlock::MAX_WORD_FONTS + 1;
  float sizeSlotScales[MAX_SIZE_SLOTS] = {1.0f};
  int32_t sizeSlotFontIds[MAX_SIZE_SLOTS] = {};
  uint8_t sizeSlotCount = 1;
  int linesAfterCurrent = -1;

  struct LayoutContext;
  static void reclaim(size_t bytes);
  static bool emitLine(void* context, const bb_line* line);

 public:
  explicit ParsedText(const bool hyphenationEnabled = false, const bool focusReadingEnabled = false,
                      const BlockStyle& blockStyle = BlockStyle(), const uint8_t paragraphIndentSpaces = 2)
      : paragraph(bb_paragraph_new(hyphenationEnabled, focusReadingEnabled, paragraphIndentSpaces, reclaim)),
        blockStyle(blockStyle) {}
  ~ParsedText() { bb_paragraph_free(paragraph); }
  ParsedText(const ParsedText&) = delete;
  ParsedText& operator=(const ParsedText&) = delete;

  void addWord(std::string word, EpdFontFamily::Style fontStyle, bool underline = false, bool attachToPrevious = false,
               uint32_t visibleTextOffset = 0, uint8_t linkId = 0, uint8_t sizeSlot = 0, bool noSpaceBefore = false);
  uint8_t addLinkTarget(const char* href);
  bool linkTargetMatches(uint8_t linkId, const char* href) const;
  void setRubyForWordAt(size_t index, const std::string& ruby) {
    bb_paragraph_set_ruby(paragraph, index, ruby.data(), ruby.size());
  }
  void setRubyGroupAt(size_t startIndex, size_t count, const std::string& ruby) {
    bb_paragraph_set_ruby_group(paragraph, startIndex, count, ruby.data(), ruby.size());
  }
  // The word's renderer style: stored flags removed, small caps restored from its size slot.
  EpdFontFamily::Style getWordStyleAt(size_t index) const {
    return static_cast<EpdFontFamily::Style>(bb_paragraph_word_style(paragraph, index));
  }
  std::string getRubyTextAt(size_t index) const {
    size_t len = 0;
    const char* ruby = bb_paragraph_ruby(paragraph, index, &len);
    return std::string(ruby, len);
  }
  void suppressFirstLineIndent() { bb_paragraph_suppress_first_line_indent(paragraph); }
  void resetFirstLineIndent() { bb_paragraph_reset_first_line_indent(paragraph); }
  void setBlockStyle(const BlockStyle& blockStyle) { this->blockStyle = blockStyle; }
  BlockStyle& getBlockStyle() { return blockStyle; }
  size_t size() const { return bb_paragraph_len(paragraph); }
  bool isEmpty() const { return size() == 0; }
  // True once a word or a line was dropped for lack of memory (or the paragraph could not be
  // created). Callers must treat the block as incomplete and fail the section build.
  bool hadDroppedWords() const { return bb_paragraph_dropped_words(paragraph); }
  // While a line is being handed to layoutAndExtractLines' callback: how many lines of the
  // paragraph follow it, or -1 when the rest of the paragraph is not laid out yet.
  int linesAfterCurrentLine() const { return linesAfterCurrent; }
  // Slot for words at an absolute size; reuses a matching slot, or the nearest once all are taken.
  uint8_t sizeSlotFor(float scale);
  uint8_t sizeSlotsInUse() const { return sizeSlotCount; }
  float sizeSlotScale(uint8_t slot) const { return sizeSlotScales[slot]; }
  void setSizeSlotFontId(uint8_t slot, int32_t fontId) { sizeSlotFontIds[slot] = fontId; }
  // The renderer's glyph metrics as layout reads them.
  static bb_metrics metricsOf(const GfxRenderer& renderer);
  // A laid-out line as a TextBlock styled block; targets holds each link span's target. NULL when
  // out of memory.
  static std::unique_ptr<TextBlock> makeLine(const bb_line& line, const BlockStyle& block, const char* const* targets);
  // Lays the words out and drops the ones used. processLine may call linesAfterCurrentLine() and
  // nothing else of this paragraph: the layout is still running.
  void layoutAndExtractLines(const GfxRenderer& renderer, int fontId, uint16_t viewportWidth,
                             const std::function<void(std::unique_ptr<TextBlock>, uint32_t)>& processLine,
                             bool includeLastLine = true, int8_t characterSpacing = 0, uint8_t wordSpacingPercent = 100,
                             size_t maxLines = SIZE_MAX);
};
