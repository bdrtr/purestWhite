#include "ChapterHtmlSlimParser.h"

#include <FsHelpers.h>
#include <GfxRenderer.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <MemoryManager.h>
#include <Utf8.h>
#include <XmlParserUtils.h>
#include <bembeyaz/drop_cap.h>
#include <bembeyaz/html.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <iterator>
#include <new>

#include "Epub.h"
#include "Epub/Page.h"
#include "Epub/converters/ImageDecoderFactory.h"
#include "Epub/converters/ImageDimsProbe.h"
#include "Epub/converters/ImageToFramebufferDecoder.h"
#include "Epub/htmlEntities.h"

// Minimum file size (in bytes) to show indexing popup - smaller chapters don't benefit from it
constexpr size_t MIN_SIZE_FOR_POPUP = 10 * 1024;  // 10KB

// This number comes from PR #73
// If we have > 750 words buffered up, perform the layout and consume out all but the last line
// There should be enough here to build out 1-2 full pages and doing this will free up a lot of
// memory.
// Spotted when reading Intermezzo, there are some really long text blocks in there.
constexpr size_t TEXT_BLOCK_SOFT_FLUSH_WORDS = 750;

// When CSS is enabled, flush earlier to save RAM. 320 is still more than enough to build a CJK
// page at font size 14
constexpr size_t TEXT_BLOCK_SOFT_FLUSH_WORDS_WITH_CSS = 320;

// Reuse serializable PageLine/PageHorizontalRule elements for a small grid.
constexpr int16_t TABLE_CELL_HORIZONTAL_PADDING = 4;
constexpr uint8_t TABLE_ROW_SEPARATOR_THICKNESS = 1;

bool isWhitespace(const char c) { return c == ' ' || c == '\r' || c == '\n' || c == '\t'; }

// The host tests check the ruby text cleanup through this.
std::string trimAndNormalize(const std::string& str) {
  std::string out = str;
  out.resize(bb_html_collapse_space(out.data(), out.size()));
  return out;
}

uint16_t elementKind(const char* name) { return bb_html_element(name, strlen(name)); }

bool headerOrBlock(const uint16_t element) { return (element & (BB_HTML_HEADING_LEVEL | BB_HTML_BLOCK)) != 0; }

uint8_t headingLevel(const uint16_t element) { return static_cast<uint8_t>(element & BB_HTML_HEADING_LEVEL); }

const char* getAttribute(const char** atts, const char* attrName) {
  if (!atts) return nullptr;
  for (int i = 0; atts[i]; i += 2) {
    if (strcmp(atts[i], attrName) == 0) return atts[i + 1];
  }
  return nullptr;
}

uint16_t parseTableSpan(const char* value) { return value ? bb_html_table_span(value, strlen(value)) : 1; }

// Returns true if the HTML element is a purely inline, non-navigable wrapper.
// IDs on these elements are never meaningful navigation targets in epub content.
// Reading-system converters (Kobo KePub, Calibre, etc.) frequently inject thousands
// of such IDs for progress tracking or internal bookkeeping, and recording each one
// as a navigation anchor exhausts the heap on memory-constrained devices.
// Block-level, sectioning, and structural elements are always considered navigable.
bool isNonNavigableInlineElement(const uint16_t element) { return (element & BB_HTML_NAME) == BB_HTML_TAG_SPAN; }

bool isInternalEpubLink(const char* href) { return href && bb_html_internal_link(href, strlen(href)); }

// The host tests check the classification through these two.
bool isHeaderOrBlock(const char* name) { return headerOrBlock(elementKind(name)); }

bool isTableStructuralTag(const char* name) { return (elementKind(name) & BB_HTML_TABLE) != 0; }

EpdFontFamily::Style ChapterHtmlSlimParser::fontStyleForTextDecoration(const CssTextDecoration decoration) {
  EpdFontFamily::Style style = EpdFontFamily::REGULAR;
  if ((decoration & CssTextDecoration::Underline) != CssTextDecoration::None) {
    style = static_cast<EpdFontFamily::Style>(style | EpdFontFamily::UNDERLINE);
  }
  if ((decoration & CssTextDecoration::LineThrough) != CssTextDecoration::None) {
    style = static_cast<EpdFontFamily::Style>(style | EpdFontFamily::STRIKETHROUGH);
  }
  return style;
}

namespace {
const bb_css_style* cssOf(const CssStyle& style) { return reinterpret_cast<const bb_css_style*>(&style); }
}  // namespace

bool ChapterHtmlSlimParser::makeInlineEntry(const bb_css_entry_kind& kind, const CssStyle& cssStyle,
                                            StyleStackEntry& entry) const {
  return bb_css_make_inline_entry(&kind, cssOf(cssStyle), depth, &entry);
}

void ChapterHtmlSlimParser::pushInlineEntry(const StyleStackEntry& entry) {
  inlineStyleStack.push_back(entry);
  updateEffectiveInlineStyle();
}

void ChapterHtmlSlimParser::pushBlockTextStyleEntry(const CssStyle& cssStyle) {
  bb_css_entry_kind kind{};
  kind.kind = BB_CSS_ENTRY_BLOCK;
  kind.sized_ancestor = std::any_of(inlineStyleStack.begin(), inlineStyleStack.end(),
                                    [](const StyleStackEntry& entry) { return entry.has_font_scale; });
  kind.scale = blockStyleStack.empty() ? 1.0f : blockStyleStack.back().fontScale;
  StyleStackEntry entry;
  if (makeInlineEntry(kind, cssStyle, entry)) pushInlineEntry(entry);
}

void ChapterHtmlSlimParser::pushDecorationStyleEntry(const CssTextDecoration defaultDecoration,
                                                     const CssStyle& cssStyle) {
  bb_css_entry_kind kind{};
  kind.kind = BB_CSS_ENTRY_DECORATION;
  kind.decoration = static_cast<uint8_t>(defaultDecoration);
  StyleStackEntry entry;
  if (makeInlineEntry(kind, cssStyle, entry)) pushInlineEntry(entry);
}

// Update effective bold/italic/decorations based on block style and inline style stack
void ChapterHtmlSlimParser::updateEffectiveInlineStyle() {
  const BlockStyle* blockStyle = blockStyleStack.empty() ? nullptr : &blockStyleStack.back();
  bb_css_effective effective{};
  bb_css_fold_inline(cssOf(currentCssStyle), blockStyle && blockStyle->directionDefined,
                     blockStyle && blockStyle->isRtl, inlineStyleStack.data(), inlineStyleStack.size(), &effective);
  effectiveBold = effective.bold;
  effectiveItalic = effective.italic;
  effectiveTextDecoration = static_cast<CssTextDecoration>(effective.decoration);
  effectiveSmallCaps = effective.small_caps;
  effectivePreserveWhitespace = effective.preserve_whitespace;
  effectiveDirectionDefined = effective.direction_defined;
  effectiveDirection = static_cast<CssTextDirection>(effective.direction);
  effectiveTextAlignDefined = effective.text_align_defined;
  effectiveTextAlign = static_cast<CssTextAlign>(effective.text_align);
  effectiveSup = effective.sup;
  effectiveSub = effective.sub;

  // Keep flow direction in the active empty text block. Inline direction remains
  // available for CSS inheritance without replacing the paragraph's base direction.
  if (currentTextBlock && currentTextBlock->isEmpty()) {
    auto& style = currentTextBlock->getBlockStyle();
    style.directionDefined = effective.paragraph_direction_defined;
    style.isRtl = effective.paragraph_rtl;
  }
}

// The previous block is on its pages: the waiting id lands on the current page (a new one when a
// table-of-contents chapter starts there).
void ChapterHtmlSlimParser::flushPendingAnchor() {
  const bb_pages pages = pageCallbacks();
  if (!bb_anchors_place_pending(&anchors, &pager, &pages)) layoutOom = true;
}

// Each page gets the note references shown on it.
void ChapterHtmlSlimParser::addFootnotesToPage(const int32_t words) {
  if (!currentPage) return;
  bb_footnotes_take(
      &notes, words,
      [](void* page, const char* number, const char* href) { static_cast<Page*>(page)->addFootnote(number, href); },
      currentPage.get());
}

// The pager places content; the pages and their elements are made here. A page or box that cannot
// be made fails the build, since the pager counts it on the page.
bb_pages ChapterHtmlSlimParser::pageCallbacks() {
  return {
      this,
      [](void* context) {
        auto& self = *static_cast<ChapterHtmlSlimParser*>(context);
        self.currentPage = makeUniqueNoThrow<Page>();
        if (!self.currentPage) {
          LOG_ERR("EHP", "OOM: Page (%zu bytes)", sizeof(Page));
          self.layoutOom = true;
        }
        return self.currentPage != nullptr;
      },
      [](void* context, const uint8_t lines, const int16_t top) {
        auto& self = *static_cast<ChapterHtmlSlimParser*>(context);
        self.nextPage = makeUniqueNoThrow<Page>();
        if (!self.nextPage) {
          LOG_ERR("EHP", "OOM: Page (%zu bytes)", sizeof(Page));
          self.layoutOom = true;
          return false;
        }
        if (lines == 0) return true;
        auto& from = self.currentPage->elements;
        const auto first = from.end() - lines;
        for (auto it = first; it != from.end(); ++it) {
          (*it)->yPos = static_cast<int16_t>((*it)->yPos - top);
          self.nextPage->elements.push_back(std::move(*it));
        }
        from.erase(first, from.end());
        self.currentPage->moveNotesBelow(top, *self.nextPage);
        return true;
      },
      [](void* context, const uint32_t visibleOffset) {
        auto& self = *static_cast<ChapterHtmlSlimParser*>(context);
        self.completePageFn(std::move(self.currentPage), self.xpathParagraphIndex, self.xpathListItemIndex,
                            visibleOffset);
        self.currentPage = std::move(self.nextPage);
      },
      [](void* context, const bb_box_segment* segment) {
        auto& self = *static_cast<ChapterHtmlSlimParser*>(context);
        const auto side = [](const bb_box_side& s) {
          return CssBorderSide{s.width, static_cast<CssBorderStyle>(s.style)};
        };
        const CssBorderSide sides[4] = {side(segment->sides[0]), side(segment->sides[1]), side(segment->sides[2]),
                                        side(segment->sides[3])};
        auto element = makeUniqueNoThrow<PageBorderBox>(segment->width, segment->height, sides, segment->shaded,
                                                        segment->x, segment->y);
        if (!element) {
          LOG_ERR("EHP", "OOM: PageBorderBox");
          self.layoutOom = true;
          return;
        }
        self.currentPage->elements.push_back(std::move(element));
      },
  };
}

static_assert(FOOTNOTE_NUMBER_LEN == BB_FOOTNOTE_NUMBER_LEN && FOOTNOTE_HREF_LEN == BB_FOOTNOTE_HREF_LEN,
              "bembeyaz-chapter keeps what FootnoteEntry holds");
static_assert(sizeof(bb_pager_line) == 3 && offsetof(bb_pager_line, drop_cap) == 1 &&
                  offsetof(bb_pager_line, last_line) == 2,
              "bb_pager_line layout");
static_assert(sizeof(CssBorderSide) == sizeof(bb_box_side) && offsetof(CssBorderSide, style) == 1 &&
                  sizeof(bb_box_segment) == 18 && offsetof(bb_box_segment, width) == 4 &&
                  offsetof(bb_box_segment, sides) == 8 && offsetof(bb_box_segment, shaded) == 16,
              "bb_box_side and bb_box_segment layout");

void ChapterHtmlSlimParser::openBoxScope(BlockStyle& ownStyle, const CssStyle& cssStyle) {
  const bool shaded = cssStyle.defined.shaded && cssStyle.shaded;
  if (tableDepth > 0 || bb_pager_box_full(&pager) || (!cssStyle.hasVisibleBorder() && !shaded)) return;

  if (partWordBufferIndex > 0) flushPartWordBuffer();
  if (currentTextBlock && !currentTextBlock->isEmpty() && !listItemBulletOnly) {
    startNewTextBlock(blockStyleStack.back().withoutTop().withoutBottom());
  }

  const auto side = [](const CssBorderSide& s) { return bb_box_side{s.width, static_cast<uint8_t>(s.style)}; };
  const bb_box_side sides[4] = {side(cssStyle.borderTop), side(cssStyle.borderRight), side(cssStyle.borderBottom),
                                side(cssStyle.borderLeft)};
  const BlockStyle& parent = blockStyleStack.back();
  const auto left = static_cast<int16_t>(parent.leftInset() + ownStyle.marginLeft);
  const auto right = static_cast<int16_t>(viewportWidth - parent.rightInset() - ownStyle.marginRight);
  // The border sits between margin and padding, so it joins the padding.
  int16_t padding[4] = {ownStyle.paddingTop, ownStyle.paddingRight, ownStyle.paddingBottom, ownStyle.paddingLeft};
  if (!bb_pager_open_box(&pager, depth, sides, shaded, left, right, padding)) return;
  ownStyle.paddingTop = padding[0];
  ownStyle.paddingRight = padding[1];
  ownStyle.paddingBottom = padding[2];
  ownStyle.paddingLeft = padding[3];
}

void ChapterHtmlSlimParser::closeBoxScope() {
  // An element without content (a rule drawn with borders) gets its own height. Its spacing still
  // sits on the empty text block; the box takes it instead.
  int above = 0;
  int below = 0;
  if (bb_pager_box_empty_height(&pager) > 0 && currentTextBlock && currentTextBlock->isEmpty()) {
    BlockStyle& pending = currentTextBlock->getBlockStyle();
    above = pending.marginTop;
    below = pending.marginBottom;
    pending.marginTop = pending.paddingTop = pending.marginBottom = pending.paddingBottom = 0;
  }
  const bb_pages pages = pageCallbacks();
  bb_pager_close_box(&pager, &pages, above, below);
}

uint8_t ChapterHtmlSlimParser::dropCapLines(const CssStyle& style) {
  static_assert(MAX_DROP_CAP_BYTES == PageDropCap::MAX_TEXT_BYTES, "captured drop-cap text must fit a PageDropCap");
  return bb_css_drop_cap_lines(cssOf(style));
}

void ChapterHtmlSlimParser::armFirstLetterDropCap(const char* tagName, const std::string& classAttr,
                                                  const char* idAttr) {
  if (!cssParser || !cssParser->hasFirstLetterRules() || tableDepth > 0 || dropCap.length > 0) return;
  const auto elementDepth = static_cast<size_t>(depth);
  const CssStyle firstLetter = cssParser->resolveStyle(tagName, classAttr, idAttr, cssAncestors.data(),
                                                       elementDepth <= MAX_CSS_ANCESTORS ? elementDepth : 0, true);
  const uint8_t lines = dropCapLines(firstLetter);
  if (lines == 0) return;
  dropCap.firstLetterPending = true;
  dropCap.lines = lines;
  dropCap.bold = firstLetter.hasFontWeight() && firstLetter.fontWeight == CssFontWeight::Bold;
}

void ChapterHtmlSlimParser::cancelDropCapToWord() {
  const int length = std::min<int>(dropCap.length, MAX_WORD_SIZE - partWordBufferIndex);
  memcpy(partWordBuffer + partWordBufferIndex, dropCap.text, length);
  partWordBufferIndex += length;
  dropCap = DropCapState{};
}

void ChapterHtmlSlimParser::layoutCurrentBlock(const bool includeLastLine) {
  const int layoutFontId = prepareBlockFont(
      *currentTextBlock, inlineSize.valid ? inlineSize.scale : currentTextBlock->getBlockStyle().fontScale);
  const int horizontalInset = currentTextBlock->getBlockStyle().totalHorizontalInset();
  const uint16_t effectiveWidth =
      (horizontalInset < viewportWidth) ? static_cast<uint16_t>(viewportWidth - horizontalInset) : viewportWidth;
  bool topSpacingApplied = wordsExtractedInBlock != 0;
  const auto applyTopSpacing = [this, &topSpacingApplied]() {
    if (topSpacingApplied) return;
    const auto& style = currentTextBlock->getBlockStyle();
    if (style.marginTop > 0) bb_pager_advance(&pager, style.marginTop);
    if (style.paddingTop > 0) bb_pager_advance(&pager, style.paddingTop);
    topSpacingApplied = true;
  };
  const auto emitLine = [this, &applyTopSpacing](std::unique_ptr<TextBlock> textBlock, const uint32_t offset) {
    applyTopSpacing();
    addLineToPage(std::move(textBlock), offset);
  };
  layoutParagraph = currentTextBlock.get();
  if (dropCap.length > 0 && dropCap.spanDepth < 0 && !currentTextBlock->isEmpty()) {
    // Lines beside the letter must stay on its page.
    layoutParagraphHasDropCap = true;
    applyTopSpacing();
    layoutDropCapLines(layoutFontId, effectiveWidth, emitLine, includeLastLine);
  }
  currentTextBlock->layoutAndExtractLines(renderer, layoutFontId, effectiveWidth, emitLine, includeLastLine,
                                          characterSpacing, wordSpacingPercent);
  layoutParagraph = nullptr;
}

static_assert(sizeof(bb_drop_cap) == 16 && offsetof(bb_drop_cap, width) == 4 && offsetof(bb_drop_cap, baseline) == 8 &&
                  offsetof(bb_drop_cap, height) == 12,
              "bb_drop_cap layout");

void ChapterHtmlSlimParser::layoutDropCapLines(
    const int layoutFontId, const uint16_t effectiveWidth,
    const std::function<void(std::unique_ptr<TextBlock>, uint32_t)>& emitLine, const bool includeLastLine) {
  const uint8_t lines = dropCap.lines;
  char text[MAX_DROP_CAP_BYTES + 1];
  memcpy(text, dropCap.text, sizeof(text));
  const EpdFontFamily::Style capStyle = dropCap.bold ? EpdFontFamily::BOLD : EpdFontFamily::REGULAR;
  dropCap = DropCapState{};

  // Enlarging the family's largest built-in size keeps outlines smoothest.
  const int capFontId = fontIdForScale(3.0f);
  const int lineHeight = renderer.getLineHeight(layoutFontId, lineCompression);
  struct Fonts {
    const GfxRenderer& renderer;
    int bodyFontId;
    int capFontId;
    EpdFontFamily::Style capStyle;
  } fonts{renderer, layoutFontId, capFontId, capStyle};
  const auto metrics = [](void* context, const bool cap, const uint32_t cp, int32_t* advance, int* top) {
    const auto& f = *static_cast<const Fonts*>(context);
    return f.renderer.getCodepointMetrics(cap ? f.capFontId : f.bodyFontId, cp,
                                          cap ? f.capStyle : EpdFontFamily::REGULAR, *advance, *top);
  };
  bb_drop_cap cap{};
  bb_drop_cap_place(lines, text, strnlen(text, sizeof(text)), lineHeight, renderer.getFontAscenderSize(layoutFontId),
                    renderer.getSpaceWidth(layoutFontId, EpdFontFamily::REGULAR), effectiveWidth, metrics, &fonts,
                    &cap);
  const int capWidth = cap.width;
  const int spannedHeight = cap.height;

  // Keep every spanned line on the letter's page.
  const bb_pages pages = pageCallbacks();
  const int top = bb_pager_drop_cap_top(&pager, &pages, spannedHeight, visibleTextOffset);
  if (!currentPage) return;  // out of memory: the build fails
  BlockStyle& blockStyle = currentTextBlock->getBlockStyle();
  auto element = makeUniqueNoThrow<PageDropCap>(capFontId, cap.scale, capStyle, text, blockStyle.leftInset(),
                                                static_cast<int16_t>(top + static_cast<int>(cap.baseline)));
  if (element) {
    currentPage->elements.push_back(std::move(element));
    bb_pager_added(&pager);
    bb_pager_note(&pager, top, top + spannedHeight);
  } else {
    LOG_ERR("EHP", "OOM: PageDropCap");
  }

  // The spanned lines wrap beside the letter; a drop-cap paragraph has no first-line indent.
  currentTextBlock->suppressFirstLineIndent();
  blockStyle.textIndent = 0;
  blockStyle.textIndentDefined = true;
  blockStyle.marginLeft = static_cast<int16_t>(blockStyle.marginLeft + capWidth);
  currentTextBlock->layoutAndExtractLines(renderer, layoutFontId, static_cast<uint16_t>(effectiveWidth - capWidth),
                                          emitLine, includeLastLine, characterSpacing, wordSpacingPercent, lines);
  blockStyle.marginLeft = static_cast<int16_t>(blockStyle.marginLeft - capWidth);
  bb_pager_reach(&pager, top + spannedHeight);
}

void ChapterHtmlSlimParser::completeCurrentPage() {
  const bb_pages pages = pageCallbacks();
  bb_pager_complete_page(&pager, &pages);
}

// Honors a pending page-break-after and the current block's page-break-before,
// once, before the block (or image) places its first content.
void ChapterHtmlSlimParser::applyPendingPageBreak() {
  bool requested = pendingPageBreak;
  pendingPageBreak = false;
  if (currentTextBlock) {
    BlockStyle& blockStyle = currentTextBlock->getBlockStyle();
    requested = requested || blockStyle.pageBreakBefore;
    blockStyle.pageBreakBefore = false;
  }
  const bb_pages pages = pageCallbacks();
  bb_pager_page_break(&pager, &pages, requested);
}

void ChapterHtmlSlimParser::pushBlockStyle(const BlockStyle& accumulated) {
  // The break belongs to the element's first block only; blocks restarted from
  // the stack after a child closes must not break again.
  BlockStyle entry = accumulated;
  entry.pageBreakBefore = false;
  blockStyleStack.push_back(entry);
}

// flush the contents of partWordBuffer to currentTextBlock
void ChapterHtmlSlimParser::flushPartWordBuffer() {
  // Block creation failed (OOM): drop the buffered text; parseStep() is about
  // to fail the build via layoutOom.
  if (!currentTextBlock) {
    partWordBufferIndex = 0;
    nextWordContinues = false;
    return;
  }

  if (effectivePreserveWhitespace) {
    currentTextBlock->suppressFirstLineIndent();
    auto& style = currentTextBlock->getBlockStyle();
    if (style.alignment == CssTextAlign::Justify) style.alignment = CssTextAlign::Left;
  }

  // Determine font style from depth-based tracking and CSS effective style
  const bool isBold = boldUntilDepth < depth || effectiveBold;
  const bool isItalic = italicUntilDepth < depth || effectiveItalic;

  // Combine style flags using bitwise OR
  EpdFontFamily::Style fontStyle = EpdFontFamily::REGULAR;
  if (isBold) {
    fontStyle = static_cast<EpdFontFamily::Style>(fontStyle | EpdFontFamily::BOLD);
  }
  if (isItalic) {
    fontStyle = static_cast<EpdFontFamily::Style>(fontStyle | EpdFontFamily::ITALIC);
  }
  fontStyle = static_cast<EpdFontFamily::Style>(fontStyle | fontStyleForTextDecoration(effectiveTextDecoration));
  if (effectiveSmallCaps) {
    fontStyle = static_cast<EpdFontFamily::Style>(fontStyle | EpdFontFamily::SMALL_CAPS);
  }
  if (effectiveSup) {
    fontStyle = static_cast<EpdFontFamily::Style>(fontStyle | EpdFontFamily::SUP);
  } else if (effectiveSub) {
    fontStyle = static_cast<EpdFontFamily::Style>(fontStyle | EpdFontFamily::SUB);
  }

  // flush the buffer
  partWordBuffer[partWordBufferIndex] = '\0';
  const size_t wordBytes = static_cast<size_t>(partWordBufferIndex);
  if (insideTableCell && !tableRowStacked && tableRowTextBytes + wordBytes > MAX_GRID_TABLE_ROW_BYTES) {
    fallbackTableRowToStacked();
  }

  uint8_t linkId = 0;
  if (insideFootnoteLink) {
    if (!currentTextBlock->linkTargetMatches(currentFootnoteLinkId, bb_footnotes_href(&notes))) {
      currentFootnoteLinkId = currentTextBlock->addLinkTarget(bb_footnotes_href(&notes));
    }
    linkId = currentFootnoteLinkId;
  }
  // Inline font-size spans size their words apart from the block.
  const float inlineScale =
      bb_css_word_scale(inlineStyleStack.data(), inlineStyleStack.size(), currentTextBlock->getBlockStyle().fontScale);
  const uint8_t sizeSlot =
      inlineScale > 0.0f && (inlineSize.valid || inlineScale != currentTextBlock->getBlockStyle().fontScale)
          ? currentTextBlock->sizeSlotFor(std::clamp(inlineScale, 0.5f, 3.0f))
          : 0;
  currentTextBlock->addWord(partWordBuffer, fontStyle, false, nextWordContinues, partWordVisibleOffset, linkId,
                            sizeSlot, effectivePreserveWhitespace);
  if (insideTableCell && !tableRowStacked) {
    tableRowTextBytes += wordBytes;
    size_t rowWords = currentTextBlock->size();
    for (const auto& cell : tableRowCells) rowWords += cell->size();
    if (rowWords > MAX_GRID_TABLE_ROW_WORDS) {
      fallbackTableRowToStacked();
    }
  }
  partWordBufferIndex = 0;
  nextWordContinues = false;
  listItemBulletOnly = false;
}

void ChapterHtmlSlimParser::breakTextLine(const BlockStyle& style) {
  const bool hasText = currentTextBlock && !currentTextBlock->isEmpty();
  if (!hasText) bb_pager_blank_line(&pager, renderer.getLineHeight(fontId, lineCompression));
  startNewTextBlock(style, false);
  if (currentTextBlock && (hasText || effectivePreserveWhitespace)) currentTextBlock->suppressFirstLineIndent();
}

// start a new text block if needed
void ChapterHtmlSlimParser::startNewTextBlock(const BlockStyle& blockStyle, const bool paragraphEnd) {
  nextWordContinues = false;  // New block = new paragraph, no continuation
  if (currentTextBlock) {
    // already have a text block running and it is empty - just reuse it
    if (currentTextBlock->isEmpty()) {
      if (paragraphEnd) currentTextBlock->resetFirstLineIndent();
      // The stack accumulates horizontal margins and text properties from ancestors.
      // Vertical margins are per-element and not inherited through the stack, but
      // container elements deposit their vertical margins on the empty block when they
      // open. Merge those into the new style so the first child in a container inherits
      // the container's vertical spacing.
      const auto style = currentTextBlock->getBlockStyle();
      currentTextBlock->setBlockStyle(style.getCombinedBlockStyle(blockStyle, BlockStyle::CombineAxis::Vertical));
      inlineSize = InlineSizeState{};

      flushPendingAnchor();
      return;
    }

    // <li> added a bullet as the first word, making the block non-empty. When a nested
    // block-level child (<p>, <div>, etc.) opens, reuse the block instead of flushing
    // the bullet to its own line. The bullet stays inline with the child's text.
    if (listItemBulletOnly) {
      const auto style = currentTextBlock->getBlockStyle();
      currentTextBlock->setBlockStyle(style.getCombinedBlockStyle(blockStyle, BlockStyle::CombineAxis::Vertical));
      listItemBulletOnly = false;
      flushPendingAnchor();
      return;
    }

    makePages(true, paragraphEnd);
  }
  // If the pending anchor is a TOC chapter boundary, force a page break after the previous
  // block is flushed so the chapter starts on a fresh page.
  flushPendingAnchor();
  dropCap.firstLetterPending = false;
  inlineSize = InlineSizeState{};
  currentTextBlock =
      makeUniqueNoThrow<ParsedText>(hyphenationEnabled, focusReadingEnabled, blockStyle, paragraphIndentSpaces);
  if (!currentTextBlock) {
    // Evict rebuildable caches and retry once before failing the build.
    freeink::MemoryManager::instance().ensureFree(4 * 1024);
    currentTextBlock =
        makeUniqueNoThrow<ParsedText>(hyphenationEnabled, focusReadingEnabled, blockStyle, paragraphIndentSpaces);
  }
  if (!currentTextBlock) {
    LOG_ERR("EHP", "OOM: ParsedText");
    layoutOom = true;  // parseStep() turns this into ParseStatus::Error
  }
  wordsExtractedInBlock = 0;
  listItemBulletOnly = false;
}

void ChapterHtmlSlimParser::emitHorizontalRule(const BlockStyle& blockStyle) {
  if (partWordBufferIndex > 0) {
    flushPartWordBuffer();
  }

  if (currentTextBlock) {
    const BlockStyle parentBlockStyle = currentTextBlock->getBlockStyle();
    startNewTextBlock(parentBlockStyle);
  }

  const int16_t lineHeight = static_cast<int16_t>(renderer.getLineHeight(fontId, lineCompression));
  const int16_t defaultVerticalSpacing = static_cast<int16_t>(lineHeight / 2);
  const int16_t topSpacing =
      static_cast<int16_t>((blockStyle.marginTop > 0 ? blockStyle.marginTop : defaultVerticalSpacing) +
                           (blockStyle.paddingTop > 0 ? blockStyle.paddingTop : 0));
  const int16_t bottomSpacing =
      static_cast<int16_t>((blockStyle.marginBottom > 0 ? blockStyle.marginBottom : defaultVerticalSpacing) +
                           (blockStyle.paddingBottom > 0 ? blockStyle.paddingBottom : 0));
  constexpr uint8_t ruleThickness = 2;
  const int16_t availableWidth =
      std::max<int16_t>(1, static_cast<int16_t>(viewportWidth - blockStyle.totalHorizontalInset()));
  const int16_t width = std::max<int16_t>(1, static_cast<int16_t>(availableWidth / 4));
  const int16_t xPos = static_cast<int16_t>(blockStyle.leftInset() + ((availableWidth - width) / 2));
  const int16_t totalHeight = static_cast<int16_t>(topSpacing + ruleThickness + bottomSpacing);

  const bb_pages pages = pageCallbacks();
  int16_t ruleY = 0;
  if (!bb_pager_rule_top(&pager, &pages, topSpacing, totalHeight, visibleTextOffset, &ruleY)) {
    LOG_ERR("EHP", "Failed to create page for horizontal rule");
    return;
  }

  auto pageRule = makeUniqueNoThrow<PageHorizontalRule>(width, ruleThickness, xPos, ruleY);
  if (!pageRule) {
    LOG_ERR("EHP", "Failed to create PageHorizontalRule");
    return;
  }
  currentPage->elements.push_back(std::move(pageRule));
  bb_pager_rule_added(&pager, ruleThickness, bottomSpacing, visibleTextOffset);

  if (!bb_anchors_record_pending(&anchors, bb_pager_completed(&pager))) layoutOom = true;
}

void ChapterHtmlSlimParser::fallbackTableRowToStacked() {
  if (tableRowStacked) {
    return;
  }

  auto activeCell = std::move(currentTextBlock);
  tableRowStacked = true;

  for (auto& cell : tableRowCells) {
    currentTextBlock = std::move(cell);
    wordsExtractedInBlock = 0;
    if (currentTextBlock && !currentTextBlock->isEmpty()) {
      makePages();
    }
  }
  tableRowCells.clear();
  currentTextBlock = std::move(activeCell);
  wordsExtractedInBlock = 0;
}

void ChapterHtmlSlimParser::closeTableCell() {
  if (!insideTableCell) {
    return;
  }
  insideTableCell = false;
  const int bottomPadding = blockStyleStack.back().paddingBottom;
  blockStyleStack.pop_back();

  if (!currentTextBlock) {
    return;
  }

  // Latch before the cell leaves currentTextBlock: parseStep()'s dropped-word
  // check only inspects currentTextBlock, so a cell parsed and moved (or reset
  // while empty) within one XML buffer would otherwise lose its OOM flag.
  if (currentTextBlock->hadDroppedWords()) {
    layoutOom = true;
  }

  if (!tableRowStacked && tableRowCells.size() >= MAX_GRID_TABLE_COLUMNS) {
    fallbackTableRowToStacked();
  }

  if (tableRowStacked) {
    wordsExtractedInBlock = 0;
    if (!currentTextBlock->isEmpty()) {
      makePages();
    } else {
      bb_pager_advance(&pager, bottomPadding);
    }
    currentTextBlock.reset();
    return;
  }

  tableRowCells.push_back(std::move(currentTextBlock));
}

void ChapterHtmlSlimParser::addTableRowSeparator() {
  int16_t separatorY = 0;
  if (!bb_pager_row_separator_top(&pager, viewportWidth, &separatorY)) {
    return;
  }

  auto separator = makeUniqueNoThrow<PageHorizontalRule>(viewportWidth, TABLE_ROW_SEPARATOR_THICKNESS, 0, separatorY);
  if (!separator) {
    LOG_ERR("EHP", "OOM: table row separator");
    return;
  }
  if (currentPage->elements.capacity() == currentPage->elements.size()) {
    currentPage->elements.reserve(currentPage->elements.size() + 1);
  }
  currentPage->elements.push_back(std::move(separator));
  bb_pager_row_separator_added(&pager);
}

// Streams a table's raw markup from its opening tag and measures each column's
// longest word and longest unwrapped line, so every row can share one set of
// widths. Returns false when the table has fewer than two or more than four
// columns, or does not end within the scan limit.
bool ChapterHtmlSlimParser::measureTableColumns(HalFile& file, TableColumnMeasure& out) const {
  // Checked against Measure in rust/crates/bembeyaz-ffi/src/table.rs too.
  static_assert(sizeof(bb_table_measure) == 18 && offsetof(bb_table_measure, columns) == 16 &&
                    sizeof(bb_table_measure::min_width) / sizeof(uint16_t) == MAX_GRID_TABLE_COLUMNS,
                "bb_table_measure layout");
  struct Context {
    HalFile& file;
    const GfxRenderer& renderer;
    int fontId;
    bool sdFont;
  } context{file, renderer, fontId, renderer.isSdCardFont(fontId)};
  const auto read = [](void* raw, uint8_t* buf, const size_t len) -> size_t {
    const int count = static_cast<Context*>(raw)->file.read(buf, len);
    return count > 0 ? static_cast<size_t>(count) : 0;
  };
  const auto advance = [](void* raw, const char* word, const bool bold) -> int {
    const auto* self = static_cast<Context*>(raw);
    const auto style = bold ? EpdFontFamily::BOLD : EpdFontFamily::REGULAR;
    if (self->sdFont) self->renderer.ensureSdCardFontReady(self->fontId, word, static_cast<uint8_t>(1u << style));
    return self->renderer.getTextAdvanceX(self->fontId, word, style);
  };
  return bb_table_measure_columns(read, advance, &context, renderer.getSpaceWidth(fontId, EpdFontFamily::REGULAR),
                                  &out);
}

// Distributes the viewport across columns the way CSS auto table layout does.
void ChapterHtmlSlimParser::planTableColumns(const TableColumnMeasure& measure) {
  tableColumnCount =
      bb_table_plan_columns(&measure, viewportWidth, TABLE_CELL_HORIZONTAL_PADDING, tableColumnWidths.data())
          ? measure.columns
          : 0;
}

// The layouts bembeyaz-ffi checks too.
static_assert(sizeof(bb_table_cell) == 8 && offsetof(bb_table_cell, padding_right) == 6 && sizeof(bb_table_row) == 28 &&
                  offsetof(bb_table_row, x) == 8 && offsetof(bb_table_row, frame_width) == 16 &&
                  offsetof(bb_table_row, top_padding) == 24 && offsetof(bb_table_row, bottom_padding) == 26,
              "bb_table_cell and bb_table_row layout");

void ChapterHtmlSlimParser::finishTableRow() {
  closeTableCell();

  if (tableRowCells.empty()) {
    if (tableRowStacked) {
      addTableRowSeparator();
    }
    tableRowStacked = false;
    return;
  }

  const int16_t lineHeight =
      std::max<int16_t>(1, static_cast<int16_t>(renderer.getLineHeight(fontId) * lineCompression));
  const size_t columnCount = tableRowCells.size();
  // closeTableCell stacks a row before its fifth cell, so the cells fit; the check keeps the
  // array and the count in step.
  bb_table_cell cells[MAX_GRID_TABLE_COLUMNS] = {};
  for (size_t column = 0; column < std::min(columnCount, MAX_GRID_TABLE_COLUMNS); ++column) {
    const auto& style = tableRowCells[column]->getBlockStyle();
    cells[column] = {style.paddingTop, style.paddingBottom, style.paddingLeft, style.paddingRight};
  }
  bb_table_row grid{};
  const uint16_t* planned = tableColumnCount == columnCount ? tableColumnWidths.data() : nullptr;
  if (columnCount > MAX_GRID_TABLE_COLUMNS ||
      !bb_table_plan_row(cells, columnCount, planned, viewportWidth, lineHeight, tableRowRtl, &grid)) {
    fallbackTableRowToStacked();
    addTableRowSeparator();
    tableRowStacked = false;
    return;
  }
  const uint16_t* columnWidths = grid.width;
  const int maxTopPadding = grid.top_padding;
  const int maxBottomPadding = grid.bottom_padding;
  for (auto& lines : tableCellLines) {
    lines.clear();
  }
  tableLineVisibleOffsets.clear();
  size_t maxLineCount = 0;

  for (size_t column = 0; column < columnCount; ++column) {
    auto& lines = tableCellLines[column];
    // Reserve from this cell's size, rather than the full row budget for every column.
    const size_t estimatedLines = tableRowCells[column]->size() * 2;
    lines.reserve(estimatedLines);
    tableLineVisibleOffsets.reserve(estimatedLines);
    const int cellFontId = prepareBlockFont(*tableRowCells[column], tableRowCells[column]->getBlockStyle().fontScale);
    tableRowCells[column]->layoutAndExtractLines(
        renderer, cellFontId,
        static_cast<uint16_t>(columnWidths[column] - cells[column].padding_left - cells[column].padding_right),
        [this, &lines](std::unique_ptr<TextBlock> line, const uint32_t offset) {
          const size_t lineIndex = lines.size();
          lines.push_back(std::move(line));
          if (tableLineVisibleOffsets.size() <= lineIndex) {
            tableLineVisibleOffsets.resize(lineIndex + 1, UINT32_MAX);
          }
          tableLineVisibleOffsets[lineIndex] = std::min(tableLineVisibleOffsets[lineIndex], offset);
        },
        true, characterSpacing, wordSpacingPercent);
    maxLineCount = std::max(maxLineCount, lines.size());
  }
  // Cell layout itself can drop lines (out of memory for a line or its TextBlock);
  // latch that before the cells are destroyed.
  for (const auto& cell : tableRowCells) {
    if (cell && cell->hadDroppedWords()) layoutOom = true;
  }
  tableRowCells.clear();
  const auto clearLayoutLines = [this]() {
    for (auto& lines : tableCellLines) {
      lines.clear();
    }
    tableLineVisibleOffsets.clear();
  };

  // Bordered tables frame every cell. Boxes overlap their neighbours by one pixel so
  // shared edges draw as a single line; a row split by a page break is framed per page.
  CssBorderSide cellBorder = tableBorder;
  cellBorder.width = std::min<uint8_t>(cellBorder.width, 3);
  const bool bordered = cellBorder.visible();
  const int16_t cellGap = bordered ? static_cast<int16_t>(cellBorder.width + 2) : 0;
  int sliceTop = -1;
  const auto frameCells = [&](const int top, const int bottom) {
    const CssBorderSide sides[4] = {cellBorder, cellBorder, cellBorder, cellBorder};
    const int height = std::min(bottom + 1, static_cast<int>(viewportHeight)) - top;
    if (height <= 0) return;
    for (size_t column = 0; column < columnCount; ++column) {
      auto box = makeUniqueNoThrow<PageBorderBox>(grid.frame_width[column], static_cast<uint16_t>(height), sides, false,
                                                  grid.x[column], static_cast<int16_t>(top));
      if (!box) {
        LOG_ERR("EHP", "OOM: table cell border");
        return;
      }
      currentPage->elements.push_back(std::move(box));
      bb_pager_added(&pager);
    }
  };
  const bb_pages pages = pageCallbacks();

  for (size_t lineIndex = 0; lineIndex < maxLineCount; ++lineIndex) {
    const uint32_t lineVisibleOffset =
        lineIndex < tableLineVisibleOffsets.size() ? tableLineVisibleOffsets[lineIndex] : visibleTextOffset;
    int16_t rowLineHeight = lineHeight;
    for (size_t column = 0; column < columnCount; ++column) {
      if (lineIndex < tableCellLines[column].size()) {
        const TextBlock& line = *tableCellLines[column][lineIndex];
        const int lineFontId = line.blockFontId(fontId);
        const int height = renderer.getLineHeight(lineFontId, lineCompression) + line.extraAscent(renderer, fontId) +
                           line.getRubyShift(renderer.getFontAscenderSize(lineFontId));
        rowLineHeight = std::max<int16_t>(rowLineHeight, static_cast<int16_t>(height));
      }
    }

    const int topGap = sliceTop < 0 ? maxTopPadding + cellGap : 0;
    const int bottomGap = maxBottomPadding + cellGap;
    const bool pageFull = bb_pager_row_overflows(&pager, topGap + bottomGap, rowLineHeight);
    if (pageFull) {
      if (bordered && sliceTop >= 0) frameCells(sliceTop, bb_pager_y(&pager) + bottomGap);
      sliceTop = -1;
      bb_pager_break_table_row(&pager, &pages, lineVisibleOffset);
    } else {
      bb_pager_ensure_page(&pager, &pages);
    }
    if (!currentPage) {
      LOG_ERR("EHP", "OOM: page for table row");
      clearLayoutLines();
      return;
    }

    if (sliceTop < 0) {
      sliceTop = bb_pager_y(&pager);
      bb_pager_set_y(&pager, static_cast<int16_t>(sliceTop + cellGap + maxTopPadding));
    }
    const int16_t rowY = bb_pager_y(&pager);
    const size_t requiredCapacity = currentPage->elements.size() + columnCount;
    if (currentPage->elements.capacity() < requiredCapacity) {
      const size_t linesThatFit = std::max<size_t>(1, static_cast<size_t>((viewportHeight - rowY) / rowLineHeight));
      const size_t linesToReserve = std::min(maxLineCount - lineIndex, linesThatFit);
      currentPage->elements.reserve(currentPage->elements.size() + linesToReserve * columnCount + 1);
    }
    for (size_t column = 0; column < columnCount; ++column) {
      if (lineIndex >= tableCellLines[column].size()) {
        continue;
      }

      auto& line = tableCellLines[column][lineIndex];
      auto style = line->getBlockStyle();
      style.marginLeft = grid.x[column];
      line->setBlockStyle(style);

      bb_pager_set_y(&pager, static_cast<int16_t>(rowY - maxTopPadding + cells[column].padding_top));
      addLineToPage(std::move(line), lineVisibleOffset);
    }
    bb_pager_set_y(&pager, static_cast<int16_t>(rowY + rowLineHeight));
  }

  bb_pager_set_y(&pager, static_cast<int16_t>(std::min<int>(bb_pager_y(&pager) + maxBottomPadding, viewportHeight)));
  bb_pager_clear_keep_with_next(&pager);
  if (bordered && sliceTop >= 0 && currentPage) {
    bb_pager_set_y(&pager, static_cast<int16_t>(std::min<int>(bb_pager_y(&pager) + cellGap, viewportHeight)));
    frameCells(sliceTop, bb_pager_y(&pager));
  } else {
    addTableRowSeparator();
  }
  tableRowStacked = false;
  clearLayoutLines();
}

// The layout bembeyaz-ffi checks too.
static_assert(sizeof(bb_html_list) == 8 && offsetof(bb_html_list, counter) == 4, "bb_html_list layout");

void ChapterHtmlSlimParser::startElement(void* userData, const char* name, const char** atts) {
  auto* self = static_cast<ChapterHtmlSlimParser*>(userData);
  const uint16_t element = elementKind(name);
  const uint16_t named = element & BB_HTML_NAME;
  if (strcasecmp(name, "body") == 0) {
    // Case-insensitive to match ParagraphStreamer's tag matching (ProgressMapper). A case
    // mismatch here would leave visibleTextOffset at 0 for the whole section, so every page
    // would record offset 0 while the sync resolver still counts a non-zero offset.
    self->insideBody = true;
  }
  if (self->insideBody && (self->nonVisibleTextDepth > 0 || (element & BB_HTML_HIDDEN))) {
    self->nonVisibleTextDepth++;
  }

  // Middle of skip
  if (self->skipUntilDepth < self->depth) {
    self->depth += 1;
    return;
  }

  // rust/crates/bembeyaz-kosync counts <p> and <li> the same way for KOReader positions.
  if (named == BB_HTML_TAG_P) {
    self->xpathParagraphIndex++;
  }
  if (named == BB_HTML_TAG_LI) {
    self->xpathListItemIndex++;
  }

  // Extract class, style, id, dir and hidden attributes for CSS/RTL processing
  std::string classAttr;
  std::string styleAttr;
  std::string dirAttr;
  const char* idAttr = "";
  bool hasHiddenAttr = false;
  if (atts != nullptr) {
    for (int i = 0; atts[i]; i += 2) {
      if (strcmp(atts[i], "class") == 0) {
        classAttr = atts[i + 1];
      } else if (strcmp(atts[i], "style") == 0) {
        styleAttr = atts[i + 1];
      } else if (strcmp(atts[i], "id") == 0) {
        // The id waits until the previous block is on its pages (startNewTextBlock). A displaced
        // one is placed first: consecutive <aside id="fn1">…</aside><aside id="fn2"> have no
        // block between them. Ids on spans are never link targets, and converters put one on
        // every span, so those are dropped unless the table of contents names them.
        idAttr = atts[i + 1];
        const bb_pages pages = self->pageCallbacks();
        if (!bb_anchors_open(&self->anchors, &self->pager, &pages, idAttr, strlen(idAttr),
                             !isNonNavigableInlineElement(element))) {
          self->layoutOom = true;
        }
      } else if (strcmp(atts[i], "dir") == 0) {
        dirAttr = atts[i + 1];
      } else if (strcmp(atts[i], "hidden") == 0) {
        hasHiddenAttr = true;
      }
    }
  }

  auto centeredBlockStyle = BlockStyle();
  centeredBlockStyle.textAlignDefined = true;
  centeredBlockStyle.alignment = CssTextAlign::Center;

  // Compute CSS style for this element early so display:none can short-circuit
  // before tag-specific branches emit any content or metadata.
  CssStyle cssStyle;
  if (self->cssParser) {
    const auto elementDepth = static_cast<size_t>(self->depth);
    cssStyle = self->cssParser->resolveStyle(name, classAttr, idAttr, self->cssAncestors.data(),
                                             elementDepth <= MAX_CSS_ANCESTORS ? elementDepth : 0);
    if (elementDepth < MAX_CSS_ANCESTORS) {
      self->cssAncestors[elementDepth] = CssParser::makeAncestor(name, classAttr, idAttr);
    }
    if (!styleAttr.empty()) {
      CssStyle inlineStyle = CssParser::parseInlineStyle(styleAttr);
      cssStyle.applyOver(inlineStyle);
    }
  }

  if (named == BB_HTML_TAG_PRE && !cssStyle.defined.whiteSpace) {
    cssStyle.preserveWhitespace = true;
    cssStyle.defined.whiteSpace = 1;
  }

  // HTML hidden attribute overrides CSS display.
  if (hasHiddenAttr) {
    cssStyle.display = CssDisplay::None;
    cssStyle.defined.display = 1;
  }

  // HTML dir attribute overrides CSS direction (case-insensitive per HTML spec)
  if (!dirAttr.empty()) {
    if (strcasecmp(dirAttr.c_str(), "rtl") == 0) {
      cssStyle.direction = CssTextDirection::Rtl;
      cssStyle.defined.direction = 1;
    } else if (strcasecmp(dirAttr.c_str(), "ltr") == 0) {
      cssStyle.direction = CssTextDirection::Ltr;
      cssStyle.defined.direction = 1;
    }
  }

  // Direction is inherited in HTML/CSS. If this element does not define one, carry
  // the currently active inherited direction into its computed style.
  if (!cssStyle.hasDirection() && self->effectiveDirectionDefined) {
    cssStyle.direction = self->effectiveDirection;
    cssStyle.defined.direction = 1;
  }

  // Skip elements with display:none before all fast paths (tables, links, etc.).
  if (cssStyle.hasDisplay() && cssStyle.display == CssDisplay::None) {
    self->skipUntilDepth = self->depth;
    self->depth += 1;
    return;
  }

  // Buffer one simple row; oversized rows fall back to full-width flow.
  if (named == BB_HTML_TAG_TABLE) {
    // Flatten nested content without allocating a recursive row buffer.
    if (self->tableDepth > 0) {
      if (self->tableDepth == 1 && self->insideTableCell && self->partWordBufferIndex > 0) {
        self->flushPartWordBuffer();
      }
      self->nextWordContinues = false;
      self->tableDepth += 1;
      self->depth += 1;
      return;
    }

    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
    }
    if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
      self->makePages();
      self->currentTextBlock.reset();
    }
    self->flushPendingAnchor();
    self->inlineSize = InlineSizeState{};
    self->dropCap.firstLetterPending = false;
    self->pushBlockTextStyleEntry(cssStyle);
    self->tableDepth = 1;
    self->insideTableCell = false;
    self->tableRowStacked = false;
    self->tableRowRtl = cssStyle.hasDirection() && cssStyle.direction == CssTextDirection::Rtl;
    self->tableRowsSpannedRemaining = 0;
    self->tableRowTextBytes = 0;
    self->tableRowCells.clear();
    self->tableRowCells.reserve(MAX_GRID_TABLE_COLUMNS);
    self->tableBorder = CssBorderSide{};
    self->tableColumnCount = 0;
    if (self->xmlParser_) {
      // Rows stream in one at a time, so plan shared column widths from the raw markup first.
      HalFile tableFile;
      TableColumnMeasure measure{};
      if (Storage.openFileForRead("EHP", self->filepath, tableFile) &&
          tableFile.seek(static_cast<size_t>(self->xmlParser_.currentByteIndex())) &&
          self->measureTableColumns(tableFile, measure)) {
        self->planTableColumns(measure);
      }
    }
    for (const CssBorderSide* side :
         {&cssStyle.borderTop, &cssStyle.borderRight, &cssStyle.borderBottom, &cssStyle.borderLeft}) {
      if (!self->tableBorder.visible() && side->visible()) self->tableBorder = *side;
    }
    const char* borderAttr = getAttribute(atts, "border");
    if (!self->tableBorder.visible() && borderAttr && atoi(borderAttr) > 0) {
      self->tableBorder = CssBorderSide{1, CssBorderStyle::Solid};
    }
    self->depth += 1;
    return;
  }

  if (self->tableDepth == 1 && named == BB_HTML_TAG_TR) {
    self->finishTableRow();
    if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
      // Text before the first row is typically a <caption>.
      self->makePages();
    }
    self->currentTextBlock.reset();
    self->tableRowTextBytes = 0;
    self->tableRowStacked = self->tableRowsSpannedRemaining > 0;
    self->tableRowRtl = cssStyle.hasDirection() && cssStyle.direction == CssTextDirection::Rtl;
    if (self->tableRowsSpannedRemaining != UINT16_MAX && self->tableRowsSpannedRemaining > 0) {
      self->tableRowsSpannedRemaining--;
    }
    self->pushBlockTextStyleEntry(cssStyle);
    self->depth += 1;
    return;
  }

  if (self->tableDepth == 1 && (named == BB_HTML_TAG_TD || named == BB_HTML_TAG_TH)) {
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
    }
    self->closeTableCell();
    if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
      self->makePages();
    }
    self->currentTextBlock.reset();

    const uint16_t columnSpan = parseTableSpan(getAttribute(atts, "colspan"));
    const uint16_t rowSpan = parseTableSpan(getAttribute(atts, "rowspan"));
    if (columnSpan > 1 || rowSpan > 1) {
      self->fallbackTableRowToStacked();
    }
    if (rowSpan > 1) {
      const uint16_t remaining = rowSpan == UINT16_MAX ? UINT16_MAX : static_cast<uint16_t>(rowSpan - 1);
      self->tableRowsSpannedRemaining = std::max(self->tableRowsSpannedRemaining, remaining);
    }

    auto tableCellBlockStyle = BlockStyle();
    self->applyBlockFontScale(tableCellBlockStyle, cssStyle, headingLevel(element));
    const float emSize = static_cast<float>(self->renderer.getFontAscenderSize(self->fontId));
    // Bound author padding so even a stacked cell leaves room for content.
    const auto padding = [&](const CssLength& length, const int limit) {
      return static_cast<int16_t>(std::clamp<int>(length.toPixelsInt16(emSize, self->viewportWidth), 0, limit));
    };
    tableCellBlockStyle.paddingLeft = cssStyle.hasPaddingLeft() ? padding(cssStyle.paddingLeft, self->viewportWidth / 4)
                                                                : TABLE_CELL_HORIZONTAL_PADDING;
    tableCellBlockStyle.paddingRight = cssStyle.hasPaddingRight()
                                           ? padding(cssStyle.paddingRight, self->viewportWidth / 4)
                                           : TABLE_CELL_HORIZONTAL_PADDING;
    tableCellBlockStyle.paddingTop = padding(cssStyle.paddingTop, self->viewportHeight / 4);
    tableCellBlockStyle.paddingBottom = padding(cssStyle.paddingBottom, self->viewportHeight / 4);
    tableCellBlockStyle.textAlignDefined = true;
    tableCellBlockStyle.alignment =
        cssStyle.hasTextAlign()
            ? cssStyle.textAlign
            : (self->effectiveTextAlignDefined
                   ? self->effectiveTextAlign
                   : (cssStyle.hasDirection() && cssStyle.direction == CssTextDirection::Rtl ? CssTextAlign::Right
                                                                                             : CssTextAlign::Left));
    if (cssStyle.hasDirection()) {
      tableCellBlockStyle.directionDefined = true;
      tableCellBlockStyle.isRtl = cssStyle.direction == CssTextDirection::Rtl;
    }

    self->currentTextBlock = makeUniqueNoThrow<ParsedText>(self->hyphenationEnabled, self->focusReadingEnabled,
                                                           tableCellBlockStyle, self->paragraphIndentSpaces);
    if (!self->currentTextBlock) {
      LOG_ERR("EHP", "OOM: table cell");
      self->skipUntilDepth = self->depth;
      self->depth += 1;
      return;
    }
    for (const CssBorderSide* side :
         {&cssStyle.borderTop, &cssStyle.borderRight, &cssStyle.borderBottom, &cssStyle.borderLeft}) {
      if (!self->tableBorder.visible() && side->visible()) self->tableBorder = *side;
    }
    self->pushBlockStyle(tableCellBlockStyle);
    self->insideTableCell = true;
    self->wordsExtractedInBlock = 0;
    self->flushPendingAnchor();
    self->pushBlockTextStyleEntry(cssStyle);

    if (named == BB_HTML_TAG_TH && (!cssStyle.hasFontWeight() || cssStyle.fontWeight == CssFontWeight::Bold)) {
      self->boldUntilDepth = std::min(self->boldUntilDepth, self->depth);
    }

    self->depth += 1;
    return;
  }

  if (self->tableDepth >= 1 && self->insideTableCell &&
      (headerOrBlock(element) || named == BB_HTML_TAG_HR || (element & BB_HTML_IMAGE))) {
    // ponytail: rich cells use full-width flow; keep a grid only for plain inline content.
    if (self->partWordBufferIndex > 0) self->flushPartWordBuffer();
    self->fallbackTableRowToStacked();
  }

  if (element & BB_HTML_IMAGE) {
    std::string src;
    std::string alt;
    if (atts != nullptr) {
      for (int i = 0; atts[i]; i += 2) {
        if (strcmp(atts[i], "src") == 0) {
          src = atts[i + 1];
        } else if (src.empty() && (strcmp(atts[i], "href") == 0 || strcmp(atts[i], "xlink:href") == 0)) {
          src = atts[i + 1];
        } else if (strcmp(atts[i], "alt") == 0) {
          alt = atts[i + 1];
        }
      }

      const size_t fragmentPos = src.find('#');
      if (fragmentPos != std::string::npos) {
        src.resize(fragmentPos);
      }

      // imageRendering: 0=display, 1=placeholder (alt text only), 2=suppress entirely
      if (self->imageRendering == 2) {
        self->skipUntilDepth = self->depth;
        self->depth += 1;
        return;
      }

      if (!src.empty() && self->imageRendering != 1) {
        LOG_DBG("EHP", "Found image: src=%s", src.c_str());

        {
          // Resolve the image path relative to the HTML file
          std::string resolvedPath = FsHelpers::normalisePath(FsHelpers::decodeUriEscapes(self->contentBase + src));

          if (ImageDecoderFactory::isFormatSupported(resolvedPath)) {
            // Create a unique filename for the cached image
            std::string ext;
            size_t extPos = resolvedPath.rfind('.');
            if (extPos != std::string::npos) {
              ext = resolvedPath.substr(extPos);
            }
            std::string cachedImagePath = self->imageBasePath + std::to_string(self->imageCounter++) + ext;

            {
              // Probe the dimensions from the entry's first bytes (early-aborted
              // inflate, a few KB) instead of extracting the whole image now —
              // extraction is deferred to the first render of the page (see
              // ImageBlock's lazy extractor). This is what keeps first-open of an
              // image-heavy chapter from stalling for seconds per image.
              ImageDimensions dims = {0, 0};
              ImageDimsProbe headerProbe;
              self->epub->readItemContentsToStream(resolvedPath, headerProbe, 1024, /*allowEarlyStop=*/true);
              bool gotDimensions = headerProbe.getDimensions(dims);

              if (!gotDimensions) {
                // Retry with framebuffer scratch when the heap cannot fit the inflate window.
                GfxRenderer::FrameBufferLoan probeLoan(self->renderer);
                ImageDimsProbe retryProbe;
                self->epub->readItemContentsToStream(resolvedPath, retryProbe, 1024, /*allowEarlyStop=*/true);
                gotDimensions = retryProbe.getDimensions(dims);
              }

              if (!gotDimensions) {
                // No header within the stream (rare) — fall back to extracting the
                // whole image and probing the file. That can take seconds, so
                // surface the indexing popup first (single-shot per parser).
                if (self->popupFn && !self->imagePopupFired) {
                  self->imagePopupFired = true;
                  self->popupFn();
                }
                HalFile cachedImageFile;
                bool extractSuccess = false;
                if (Storage.openFileForWrite("EHP", cachedImagePath, cachedImageFile)) {
                  {
                    // Same 32 KB inflate window as the probe; the popup is already up.
                    GfxRenderer::FrameBufferLoan extractLoan(self->renderer);
                    extractSuccess = self->epub->readItemContentsToStream(resolvedPath, cachedImageFile, 4096);
                  }
                  cachedImageFile.flush();
                  cachedImageFile.close();
                }
                if (extractSuccess) {
                  // Retry to absorb SD-card sync latency on slow cards, and to close
                  // the silent-drop bug where a single getDimensions failure was fatal.
                  ImageToFramebufferDecoder* decoder = ImageDecoderFactory::getDecoder(cachedImagePath);
                  for (int attempt = 0; attempt < 3 && !gotDimensions; attempt++) {
                    if (attempt > 0) {
                      delay(50);  // Give a slow SD card time to finish syncing before retrying
                    }
                    gotDimensions = decoder && decoder->getDimensions(cachedImagePath, dims);
                  }
                } else {
                  LOG_ERR("EHP", "Failed to extract image");
                }
              }

              if (gotDimensions) {
                LOG_DBG("EHP", "Image dimensions: %dx%d", dims.width, dims.height);

                // Compute effective container width for percentage-based image sizes.
                // If the image is inside a block with horizontal margins/padding (e.g.
                // <div style="margin: 1em 40%">), percentage widths like width:100%
                // should resolve against the container width, not the full viewport.
                int containerWidth = self->viewportWidth;
                if (self->currentTextBlock) {
                  const int inset = self->currentTextBlock->getBlockStyle().totalHorizontalInset();
                  if (inset > 0 && inset < self->viewportWidth) {
                    containerWidth = self->viewportWidth - inset;
                  }
                }
                int displayWidth = 0;
                int displayHeight = 0;
                bb_css_image_size(cssOf(cssStyle), dims.width, dims.height,
                                  static_cast<float>(self->renderer.getFontAscenderSize(self->fontId)), containerWidth,
                                  self->viewportHeight, &displayWidth, &displayHeight);
                LOG_DBG("EHP", "Display size: %dx%d", displayWidth, displayHeight);

                // Flush any pending text block so it appears before the image
                if (self->partWordBufferIndex > 0) {
                  self->flushPartWordBuffer();
                }
                if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
                  const BlockStyle parentBlockStyle = self->currentTextBlock->getBlockStyle();
                  self->startNewTextBlock(parentBlockStyle);
                }

                // Apply vertical margins from the container to the image.
                // Top margin lives on the empty text block (deposited via vertical merge
                // in startNewTextBlock). Bottom margin was stripped by withoutBottom() for
                // deferred application at element close, so read it from the stack.
                int16_t imageMarginTop = 0;
                int16_t imageMarginBottom = 0;
                if (self->currentTextBlock && self->currentTextBlock->isEmpty()) {
                  const auto& bs = self->currentTextBlock->getBlockStyle();
                  imageMarginTop = bs.topInset();
                  if (self->blockStyleStack.size() > 1) {
                    imageMarginBottom = self->blockStyleStack.back().bottomInset();
                  }
                }

                self->applyPendingPageBreak();
                const bb_pages pages = self->pageCallbacks();
                int16_t imageY = 0;
                if (!bb_pager_image_top(&self->pager, &pages, imageMarginTop, displayHeight, imageMarginBottom,
                                        self->visibleTextOffset, &imageY) ||
                    !self->currentPage) {
                  LOG_ERR("EHP", "Failed to create page for image");
                  return;
                }

                auto imageBlock =
                    makeUniqueNoThrow<ImageBlock>(cachedImagePath, resolvedPath, displayWidth, displayHeight);
                if (!imageBlock) {
                  LOG_ERR("EHP", "Failed to create ImageBlock");
                  return;
                }
                int xPos = (self->viewportWidth - displayWidth) / 2;
                auto pageImage = makeUniqueNoThrow<PageImage>(std::move(imageBlock), xPos, imageY);
                if (!pageImage) {
                  LOG_ERR("EHP", "Failed to create PageImage");
                  return;
                }
                self->currentPage->elements.push_back(std::move(pageImage));
                bb_pager_image_added(&self->pager, displayHeight, imageMarginBottom, self->visibleTextOffset);

                // The image consumed the empty block's accumulated vertical spacing.
                // Reset the block so the Vertical merge in startNewTextBlock doesn't
                // re-apply the same margins to the next text paragraph.
                if (self->currentTextBlock && self->currentTextBlock->isEmpty()) {
                  BlockStyle resetStyle;
                  resetStyle.alignment = (self->paragraphAlignment == static_cast<uint8_t>(CssTextAlign::None))
                                             ? CssTextAlign::Justify
                                             : static_cast<CssTextAlign>(self->paragraphAlignment);
                  self->currentTextBlock->setBlockStyle(resetStyle);
                }

                self->depth += 1;
                return;
              } else {
                LOG_ERR("EHP", "Failed to get image dimensions");
                Storage.remove(cachedImagePath.c_str());
              }
            }
          }  // isFormatSupported
        }
      }

      // Fallback to alt text if image processing fails
      if (!alt.empty()) {
        alt = "[Image: " + alt + "]";
        self->startNewTextBlock(self->blockStyleStack.back()
                                    .getCombinedBlockStyle(centeredBlockStyle, BlockStyle::CombineAxis::Horizontal)
                                    .withoutBottom());
        self->italicUntilDepth = std::min(self->italicUntilDepth, self->depth);
        self->depth += 1;
        self->syntheticCharacterData = true;
        self->characterData(userData, alt.c_str(), alt.length());
        self->syntheticCharacterData = false;
        // Skip any child content (skip until parent as we pre-advanced depth above)
        self->skipUntilDepth = self->depth - 1;
        return;
      }

      // No alt text, skip
      self->skipUntilDepth = self->depth;
      self->depth += 1;
      return;
    }
  }

  // Ruby tag handling
  if (named == BB_HTML_TAG_RUBY) {
    // <ruby> is an inline element: a base that follows text with no whitespace between them
    // continues the same visual word, exactly like <b>/<i> handling in endElement().
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
      self->nextWordContinues = true;
    }
    self->inRuby = true;
    self->rubyStartWordIndex = self->currentTextBlock ? static_cast<int>(self->currentTextBlock->size()) : 0;
    self->rubyTextBuffer.clear();
    self->depth += 1;
    return;
  }
  if (named == BB_HTML_TAG_RT) {
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
    }
    self->collectingRubyText = true;
    self->depth += 1;
    return;
  }

  if (element & BB_HTML_HIDDEN) {
    // start skip
    self->skipUntilDepth = self->depth;
    self->depth += 1;
    return;
  }

  // Skip blocks with role="doc-pagebreak" and epub:type="pagebreak"
  if (atts != nullptr) {
    for (int i = 0; atts[i]; i += 2) {
      if (strcmp(atts[i], "role") == 0 && strcmp(atts[i + 1], "doc-pagebreak") == 0 ||
          strcmp(atts[i], "epub:type") == 0 && strcmp(atts[i + 1], "pagebreak") == 0) {
        self->skipUntilDepth = self->depth;
        self->depth += 1;
        return;
      }
    }
  }

  // Detect internal <a href="..."> links (footnotes, cross-references)
  // Note: <aside epub:type="footnote"> elements are rendered as normal content
  // without special handling. Links pointing to them are collected as footnotes.
  if (named == BB_HTML_TAG_A) {
    const char* href = getAttribute(atts, "href");

    bool isInternalLink = isInternalEpubLink(href);

    // Special case: javascript:void(0) links with data attributes
    // Example: <a href="javascript:void(0)"
    // data-xyz="{&quot;name&quot;:&quot;OPS/ch2.xhtml&quot;,&quot;frag&quot;:&quot;id46&quot;}">
    if (href && strncmp(href, "javascript:", 11) == 0) {
      isInternalLink = false;
      // TODO: Parse data-* attributes to extract actual href
    }

    if (isInternalLink) {
      // Footnote indices are block-relative, so linked rows use ordinary flow.
      if (self->tableDepth >= 1 && self->insideTableCell && !self->tableRowStacked) {
        self->fallbackTableRowToStacked();
      }

      // Flush buffer before style change
      if (self->partWordBufferIndex > 0) {
        self->flushPartWordBuffer();
        self->nextWordContinues = true;
      }
      self->insideFootnoteLink = true;
      self->footnoteLinkDepth = self->depth;
      self->currentFootnoteLinkId = self->currentTextBlock ? self->currentTextBlock->addLinkTarget(href) : 0;
      // The block keeps the target only when it fits.
      const char* target = self->currentFootnoteLinkId != 0 ? href : "";
      if (!bb_footnotes_open(&self->notes, target, strlen(target))) self->layoutOom = true;

      // Apply underline style to visually indicate the link.
      bb_css_entry_kind kind{};
      kind.kind = BB_CSS_ENTRY_LINK;
      StyleStackEntry entry;
      if (self->makeInlineEntry(kind, cssStyle, entry)) self->pushInlineEntry(entry);

      // Skip CSS resolution — we already handled styling for this <a> tag
      self->depth += 1;
      return;
    }
  }

  const float emSize = static_cast<float>(self->renderer.getFontAscenderSize(self->fontId));
  const auto userAlignmentBlockStyle = BlockStyle::fromCssStyle(
      cssStyle, emSize, static_cast<CssTextAlign>(self->paragraphAlignment), self->viewportWidth);

  if (named == BB_HTML_TAG_HR) {
    auto hrBlockStyle = BlockStyle::fromCssStyle(cssStyle, emSize, CssTextAlign::Left, self->viewportWidth);
    if (!self->embeddedStyle) {
      hrBlockStyle.marginLeft = 0;
      hrBlockStyle.marginRight = 0;
      hrBlockStyle.marginTop = 0;
      hrBlockStyle.marginBottom = 0;
      hrBlockStyle.paddingLeft = 0;
      hrBlockStyle.paddingRight = 0;
      hrBlockStyle.paddingTop = 0;
      hrBlockStyle.paddingBottom = 0;
      hrBlockStyle.textIndentDefined = false;
      hrBlockStyle.textIndent = 0;
    }
    self->emitHorizontalRule(hrBlockStyle);
    self->depth += 1;
    return;
  }

  if (element & BB_HTML_HEADING_LEVEL) {
    // Text running straight into a block ("text<h3>…") is still in the part-word
    // buffer; flush it into the block it belongs to before that block is replaced.
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
    }
    self->currentCssStyle = cssStyle;
    auto headerBlockStyle = BlockStyle::fromCssStyle(cssStyle, emSize, CssTextAlign::Center, self->viewportWidth);
    headerBlockStyle.textAlignDefined = true;
    if (self->embeddedStyle && cssStyle.hasTextAlign()) {
      headerBlockStyle.alignment = cssStyle.textAlign;
    }
    self->applyBlockFontScale(headerBlockStyle, cssStyle, headingLevel(element));
    headerBlockStyle.keepWithNext = true;
    self->openBoxScope(headerBlockStyle, cssStyle);
    const auto accumulated =
        self->blockStyleStack.back().getCombinedBlockStyle(headerBlockStyle, BlockStyle::CombineAxis::Horizontal);
    self->pushBlockStyle(accumulated);
    self->startNewTextBlock(accumulated.withoutBottom());
    self->pushBlockTextStyleEntry(cssStyle);
    self->armFirstLetterDropCap(name, classAttr, idAttr);
    self->boldUntilDepth = std::min(self->boldUntilDepth, self->depth);
    self->updateEffectiveInlineStyle();
  } else if (element & BB_HTML_BLOCK) {
    if (named == BB_HTML_TAG_BR) {
      if (self->partWordBufferIndex > 0) {
        // flush word preceding <br/> to currentTextBlock before calling startNewTextBlock
        self->flushPartWordBuffer();
      }
      self->breakTextLine(self->blockStyleStack.back().withoutTop().withoutBottom());
    } else {
      // Flush pending text before the new block, as the header and <br/> branches do.
      if (self->partWordBufferIndex > 0) {
        self->flushPartWordBuffer();
      }
      self->currentCssStyle = cssStyle;
      auto blockStyle = userAlignmentBlockStyle;
      self->applyBlockFontScale(blockStyle, cssStyle, headingLevel(element));
      self->openBoxScope(blockStyle, cssStyle);
      const auto accumulated =
          self->blockStyleStack.back().getCombinedBlockStyle(blockStyle, BlockStyle::CombineAxis::Horizontal);
      self->pushBlockStyle(accumulated);
      self->startNewTextBlock(accumulated.withoutBottom());
      if (!self->currentTextBlock) {
        // OOM: layoutOom is latched; bail before the <li> marker path below
        // dereferences the missing block. parseStep() fails the build.
        return;
      }
      self->pushBlockTextStyleEntry(cssStyle);
      self->updateEffectiveInlineStyle();
      if (named != BB_HTML_TAG_LI) self->armFirstLetterDropCap(name, classAttr, idAttr);

      if (named == BB_HTML_TAG_LI) {
        // Innermost open <ul>/<ol> (if any) decides whether this item gets a bullet,
        // a number, or no marker at all (list-style-type: none). A malformed <li>
        // with no enclosing list falls back to the plain bullet. No marker leaves the block
        // empty, so it behaves like a normal paragraph.
        char marker[BB_HTML_MAX_MARKER + 1];
        const size_t markerLength =
            bb_html_list_marker(self->listStack.empty() ? nullptr : &self->listStack.back().list, marker);
        marker[markerLength] = '\0';
        if (markerLength > 0) {
          self->currentTextBlock->addWord(marker, EpdFontFamily::REGULAR, false, false, self->visibleTextOffset);
          self->listItemBulletOnly = true;
        }
      } else if (named == BB_HTML_TAG_UL || named == BB_HTML_TAG_OL) {
        // <ul>/<ol> container accumulation: pushing the block-style stack here includes
        // the container's own margin/padding in the inset children combine with, so a
        // hanging text-indent on <li><p> children keeps the first line on the page.
        ChapterHtmlSlimParser::ListContext ctx;
        ctx.list.ordered = named == BB_HTML_TAG_OL;
        ctx.list.unmarked = cssStyle.hasListStyleType() && cssStyle.listStyleType == CssListStyleType::None;
        ctx.depth = self->depth;
        self->listStack.push_back(ctx);
      }
    }
  } else if (element & BB_HTML_UNDERLINE) {
    // Flush buffer before style change so preceding text gets current style
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
      self->nextWordContinues = true;
    }
    self->pushDecorationStyleEntry(CssTextDecoration::Underline, cssStyle);
  } else if (element & BB_HTML_LINE_THROUGH) {
    // Flush buffer before style change so preceding text gets current style
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
      self->nextWordContinues = true;
    }
    self->pushDecorationStyleEntry(CssTextDecoration::LineThrough, cssStyle);
  } else if (element & BB_HTML_BOLD) {
    // Flush buffer before style change so preceding text gets current style
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
      self->nextWordContinues = true;
    }
    self->boldUntilDepth = std::min(self->boldUntilDepth, self->depth);
    // Push inline style entry for bold tag
    bb_css_entry_kind kind{};
    kind.kind = BB_CSS_ENTRY_BOLD;
    StyleStackEntry entry;
    if (self->makeInlineEntry(kind, cssStyle, entry)) self->pushInlineEntry(entry);
  } else if (element & BB_HTML_ITALIC) {
    // Flush buffer before style change so preceding text gets current style
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
      self->nextWordContinues = true;
    }
    self->italicUntilDepth = std::min(self->italicUntilDepth, self->depth);
    // Push inline style entry for italic tag
    bb_css_entry_kind kind{};
    kind.kind = BB_CSS_ENTRY_ITALIC;
    StyleStackEntry entry;
    if (self->makeInlineEntry(kind, cssStyle, entry)) self->pushInlineEntry(entry);
  } else if (named == BB_HTML_TAG_SUP || named == BB_HTML_TAG_SUB) {
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
      self->nextWordContinues = true;
    }
    bb_css_entry_kind kind{};
    kind.kind = named == BB_HTML_TAG_SUP ? BB_CSS_ENTRY_SUP : BB_CSS_ENTRY_SUB;
    StyleStackEntry entry;
    if (self->makeInlineEntry(kind, cssStyle, entry)) self->pushInlineEntry(entry);
  } else if (named == BB_HTML_TAG_SPAN || !headerOrBlock(element)) {
    if (cssStyle.hasFontSize() && self->tableDepth == 0 && self->partWordBufferIndex == 0 && self->currentTextBlock &&
        self->currentTextBlock->isEmpty() && (!self->inlineSize.valid || self->inlineSize.open)) {
      const float base =
          self->inlineSize.valid ? self->inlineSize.scale : self->currentTextBlock->getBlockStyle().fontScale;
      self->inlineSize.scale = bb_css_span_scale(cssOf(cssStyle), base);
      if (!self->inlineSize.valid) self->inlineSize.depth = self->depth;
      self->inlineSize.valid = true;
      self->inlineSize.open = true;
    }
    // A floated or oversized span opening a paragraph is a hand-made drop cap.
    if (self->tableDepth == 0 && self->dropCap.length == 0 && self->dropCap.spanDepth < 0 &&
        self->partWordBufferIndex == 0 && self->currentTextBlock && self->currentTextBlock->isEmpty()) {
      const uint8_t lines = dropCapLines(cssStyle);
      if (lines > 0) {
        self->dropCap.spanDepth = self->depth;
        self->dropCap.lines = lines;
        self->dropCap.firstLetterPending = false;
        self->dropCap.bold =
            self->effectiveBold || (cssStyle.hasFontWeight() && cssStyle.fontWeight == CssFontWeight::Bold);
      }
    }
    // Handle span and other inline elements for CSS styling.
    bb_css_entry_kind kind{};
    kind.kind = BB_CSS_ENTRY_INLINE;
    kind.sets_paragraph = named == BB_HTML_TAG_HTML || named == BB_HTML_TAG_BODY;
    kind.in_table = self->tableDepth >= 1;
    kind.scale = bb_css_tag_scale(name, strlen(name));
    StyleStackEntry entry;
    if (self->makeInlineEntry(kind, cssStyle, entry)) {
      // Flush buffer before style change so preceding text gets current style
      if (self->partWordBufferIndex > 0) {
        self->flushPartWordBuffer();
        self->nextWordContinues = true;
      }
      self->pushInlineEntry(entry);
    }
  }

  // Unprocessed tag, just increasing depth and continue forward
  self->depth += 1;
}

void ChapterHtmlSlimParser::characterData(void* userData, const char* s, const int len) {
  auto* self = static_cast<ChapterHtmlSlimParser*>(userData);
  const bool countVisibleOffsets = self->insideBody && self->nonVisibleTextDepth == 0 && !self->syntheticCharacterData;
  const uint32_t callbackVisibleOffset = self->visibleTextOffset;
  if (countVisibleOffsets) {
    const unsigned char* ptr = reinterpret_cast<const unsigned char*>(s);
    const unsigned char* end = ptr + len;
    while (ptr < end) {
      utf8NextCodepoint(&ptr);
      self->visibleTextOffset++;
    }
  }

  // Nested content needs an enclosing bounded cell collector.
  if (self->tableDepth > 1 && !self->insideTableCell) {
    return;
  }

  // Middle of skip
  if (self->skipUntilDepth < self->depth) {
    return;
  }

  // Collect ruby text instead of normal word processing.
  if (self->collectingRubyText) {
    self->rubyTextBuffer.append(s, len);
    return;
  }

  if (self->tableDepth == 1 && !self->insideTableCell) {
    bool onlyWhitespace = true;
    for (int i = 0; i < len; ++i) {
      if (!isWhitespace(s[i])) {
        onlyWhitespace = false;
        break;
      }
    }
    if (onlyWhitespace) {
      return;
    }
  }

  // Recreate flow storage for valid text (for example a caption) after a row.
  if (!self->currentTextBlock) {
    const BlockStyle flowStyle =
        self->blockStyleStack.empty() ? BlockStyle() : self->blockStyleStack.back().withoutBottom();
    self->currentTextBlock = makeUniqueNoThrow<ParsedText>(self->hyphenationEnabled, self->focusReadingEnabled,
                                                           flowStyle, self->paragraphIndentSpaces);
    if (!self->currentTextBlock) {
      LOG_ERR("EHP", "OOM: text block for character data");
      return;
    }
    self->wordsExtractedInBlock = 0;
  }

  // A note reference's text is its number ("[1]" is 1).
  if (self->insideFootnoteLink) {
    bb_footnotes_text(&self->notes, s, static_cast<size_t>(std::max(len, 0)));
  }

  if (self->inlineSize.valid && !self->inlineSize.open &&
      std::any_of(s, s + len, [](const char c) { return !isWhitespace(c); })) {
    self->inlineSize.valid = false;
  }

  const bb_html_words words{
      self->partWordBuffer,
      &self->partWordBufferIndex,
      &self->partWordVisibleOffset,
      &self->nextWordContinues,
      self,
      [](void* context) { static_cast<ChapterHtmlSlimParser*>(context)->flushPartWordBuffer(); },
      [](void* context) {
        auto& block = static_cast<ChapterHtmlSlimParser*>(context)->currentTextBlock;
        if (block) block->suppressFirstLineIndent();
      },
      [](void* context) {
        auto& parser = *static_cast<ChapterHtmlSlimParser*>(context);
        const BlockStyle style = parser.currentTextBlock->getBlockStyle().withoutTop().withoutBottom();
        parser.breakTextLine(style);
        return parser.currentTextBlock != nullptr;
      },
      self->initialLetter(),
      [](void* context) { return static_cast<ChapterHtmlSlimParser*>(context)->currentTextBlock->isEmpty(); }};
  if (bb_html_scan_words(s, static_cast<size_t>(std::max(len, 0)), countVisibleOffsets, callbackVisibleOffset,
                         self->effectivePreserveWhitespace, self->insideFootnoteLink,
                         &words) == BB_HTML_WORDS_STOPPED) {
    return;
  }

  // Block creation failed (OOM): nothing to soft-flush.
  if (!self->currentTextBlock) {
    return;
  }

  // Keep token growth bounded: CSS-heavy spans can fragment text into many tiny
  // words, so flush earlier when embedded CSS is active. We still keep the
  // "exclude last line" behavior to preserve paragraph flow across chunks.
  const size_t blockWordCount = self->currentTextBlock->size();
  const size_t softFlushThreshold =
      self->embeddedStyle ? TEXT_BLOCK_SOFT_FLUSH_WORDS_WITH_CSS : TEXT_BLOCK_SOFT_FLUSH_WORDS;
  if (blockWordCount > softFlushThreshold && !self->inRuby) {
    LOG_DBG("EHP", "Text block soft flush (%u words)", static_cast<unsigned>(blockWordCount));
    self->makePages(/*includeLastLine=*/false);
  }
}

// An entity the document does not declare (HTML entities like &nbsp;): expand the HTML ones, keep
// the others as written.
void ChapterHtmlSlimParser::skippedEntity(void* userData, const char* name) {
  htmlEntityText(name, userData, characterData);
}

void ChapterHtmlSlimParser::endElement(void* userData, const char* name) {
  auto* self = static_cast<ChapterHtmlSlimParser*>(userData);
  const uint16_t element = elementKind(name);
  const uint16_t named = element & BB_HTML_NAME;
  if (self->nonVisibleTextDepth > 0) {
    self->nonVisibleTextDepth--;
  }

  // Ruby text: </rt> distributes ruby to base words, </ruby> resets ruby state
  if (named == BB_HTML_TAG_RT) {
    self->collectingRubyText = false;
    if (self->inRuby && self->currentTextBlock) {
      const int currentWordCount = static_cast<int>(self->currentTextBlock->size());
      const int baseWordCount = currentWordCount - self->rubyStartWordIndex;
      self->rubyTextBuffer.resize(bb_html_collapse_space(self->rubyTextBuffer.data(), self->rubyTextBuffer.size()));
      const std::string& cleanRuby = self->rubyTextBuffer;
      if (!cleanRuby.empty()) {
        if (baseWordCount > 0) {
          self->currentTextBlock->setRubyGroupAt(self->rubyStartWordIndex, baseWordCount, cleanRuby);
          self->rubyStartWordIndex = currentWordCount;
        } else if (self->rubyStartWordIndex > 0) {
          int leaderIdx = self->rubyStartWordIndex - 1;
          while (leaderIdx >= 0 &&
                 (self->currentTextBlock->getWordStyleAt(leaderIdx) & EpdFontFamily::RUBY_CONTINUE) != 0) {
            leaderIdx--;
          }
          if (leaderIdx >= 0) {
            std::string prevRuby = self->currentTextBlock->getRubyTextAt(leaderIdx);
            self->currentTextBlock->setRubyForWordAt(leaderIdx, prevRuby + cleanRuby);
          }
        }
      }
    }
    self->rubyTextBuffer.clear();
    // Inline close: the next base (e.g. 字 in <ruby>漢<rt>かん</rt>字<rt>じ</rt></ruby>) joins the
    // preceding one with no space. Whitespace in the source resets this in characterData().
    if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
      self->nextWordContinues = true;
    }
    self->depth -= 1;
    return;
  }
  if (named == BB_HTML_TAG_RUBY && self->inRuby) {
    self->inRuby = false;
    self->rubyStartWordIndex = -1;
    self->rubyTextBuffer.clear();
    // Inline close: text following </ruby> joins the annotated base with no space.
    if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
      self->nextWordContinues = true;
    }
    self->depth -= 1;
    return;
  }
  // Check if any style state will change after we decrement depth
  // If so, we MUST flush the partWordBuffer with the CURRENT style first
  // Note: depth hasn't been decremented yet, so we check against (depth - 1)
  const bool willPopStyleStack =
      !self->inlineStyleStack.empty() && self->inlineStyleStack.back().depth == self->depth - 1;
  const bool willClearBold = self->boldUntilDepth == self->depth - 1;
  const bool willClearItalic = self->italicUntilDepth == self->depth - 1;

  const bool styleWillChange = willPopStyleStack || willClearBold || willClearItalic;
  const bool headerOrBlockTag = headerOrBlock(element);
  const bool tableStructuralTag = (element & BB_HTML_TABLE) != 0;
  const bool insideSkippedSubtree = self->depth - 1 >= self->skipUntilDepth;

  if (!insideSkippedSubtree && self->tableDepth > 1 && named == BB_HTML_TAG_TABLE) {
    if (self->partWordBufferIndex > 0) {
      self->flushPartWordBuffer();
    }
    self->nextWordContinues = false;
    self->tableDepth -= 1;
    self->depth -= 1;
    LOG_DBG("EHP", "nested table flattened into enclosing cell");
    return;
  }

  // Flush buffer with current style BEFORE any style changes
  if (self->partWordBufferIndex > 0) {
    // Flush if style will change OR if we're closing a block/structural element
    const bool isInlineTag = !headerOrBlockTag && !tableStructuralTag && !(element & BB_HTML_IMAGE) && self->depth != 1;
    const bool shouldFlush =
        styleWillChange || headerOrBlockTag || tableStructuralTag || self->depth == 1 ||
        (element & (BB_HTML_BOLD | BB_HTML_ITALIC | BB_HTML_UNDERLINE | BB_HTML_LINE_THROUGH | BB_HTML_IMAGE));

    if (shouldFlush) {
      self->flushPartWordBuffer();
      // If closing an inline element, the next word fragment continues the same visual word
      if (isInlineTag) {
        self->nextWordContinues = true;
      }
    }
  }

  // A captured letter without body text is ordinary block content.
  if (headerOrBlockTag && named != BB_HTML_TAG_BR && !insideSkippedSubtree && self->currentTextBlock &&
      self->currentTextBlock->isEmpty() && self->dropCap.length > 0) {
    self->cancelDropCapToWord();
    self->flushPartWordBuffer();
  }

  self->depth -= 1;

  // The captured drop-cap letter waits for its block's layout.
  if (self->dropCap.spanDepth == self->depth) {
    self->dropCap.spanDepth = -1;
  }
  if (self->inlineSize.valid && self->inlineSize.depth == self->depth) {
    self->inlineSize.open = false;
  }

  // Closing a footnote link — create entry from collected text and href
  if (self->insideFootnoteLink && self->depth == self->footnoteLinkDepth) {
    const int wordIndex =
        self->wordsExtractedInBlock + (self->currentTextBlock ? static_cast<int>(self->currentTextBlock->size()) : 0);
    if (!bb_footnotes_close(&self->notes, wordIndex)) self->layoutOom = true;
    self->insideFootnoteLink = false;
    self->currentFootnoteLinkId = 0;
  }

  // Leaving skip
  if (self->skipUntilDepth == self->depth) {
    self->skipUntilDepth = INT_MAX;
  }

  if (!insideSkippedSubtree && self->tableDepth == 1 && (named == BB_HTML_TAG_TD || named == BB_HTML_TAG_TH)) {
    self->closeTableCell();
    self->nextWordContinues = false;
  }

  if (!insideSkippedSubtree && self->tableDepth == 1 && (named == BB_HTML_TAG_TR)) {
    self->finishTableRow();
    self->nextWordContinues = false;
  }

  if (!insideSkippedSubtree && self->tableDepth == 1 && named == BB_HTML_TAG_TABLE) {
    self->finishTableRow();
    if (self->currentTextBlock && !self->currentTextBlock->isEmpty()) {
      self->makePages();
    }
    self->currentTextBlock.reset();
    self->tableDepth = 0;
    self->insideTableCell = false;
    self->tableRowStacked = false;
    self->tableRowsSpannedRemaining = 0;
    self->tableRowTextBytes = 0;
    self->tableRowCells.clear();
    self->nextWordContinues = false;

    const BlockStyle flowStyle =
        self->blockStyleStack.empty() ? BlockStyle() : self->blockStyleStack.back().withoutBottom();
    self->currentTextBlock = makeUniqueNoThrow<ParsedText>(self->hyphenationEnabled, self->focusReadingEnabled,
                                                           flowStyle, self->paragraphIndentSpaces);
    if (!self->currentTextBlock) {
      LOG_ERR("EHP", "OOM: text block after table");
    }
    self->wordsExtractedInBlock = 0;
  }

  // Leaving bold tag
  if (self->boldUntilDepth == self->depth) {
    self->boldUntilDepth = INT_MAX;
  }

  // Leaving italic tag
  if (self->italicUntilDepth == self->depth) {
    self->italicUntilDepth = INT_MAX;
  }

  // Pop from inline style stack if we pushed an entry at this depth
  // This handles all inline elements: b, i, u, span, etc.
  if (!self->inlineStyleStack.empty() && self->inlineStyleStack.back().depth == self->depth) {
    self->inlineStyleStack.pop_back();
    self->updateEffectiveInlineStyle();
  }

  // Clear block style when leaving header or block elements
  if (headerOrBlockTag && !insideSkippedSubtree) {
    self->currentCssStyle.reset();
    self->updateEffectiveInlineStyle();

    // br is self-closing and not a container — it doesn't push/pop the stack.
    if (named != BB_HTML_TAG_BR && self->blockStyleStack.size() > 1) {
      // Apply closing element's bottom margin to the current text block so
      // container spacing appears after the element's content (on the last child),
      // not on the first child via the empty-block merge in startNewTextBlock.
      if (self->currentTextBlock) {
        const auto style = self->currentTextBlock->getBlockStyle();
        self->currentTextBlock->setBlockStyle(style.addBottom(self->blockStyleStack.back()));
      }
      const bool breakAfter = self->blockStyleStack.back().pageBreakAfter;
      self->blockStyleStack.pop_back();
      // Start a new text block with the parent style to prevent subsequent bare text
      // from inheriting the closed block style (e.g. alignment or margins).
      // Vertical margins and paddings are stripped
      self->startNewTextBlock(self->blockStyleStack.back().withoutTop().withoutBottom());
      self->updateEffectiveInlineStyle();
      if (bb_pager_box_innermost_is(&self->pager, self->depth)) self->closeBoxScope();
      // Set after the closing element's own text was laid out above.
      if (breakAfter) self->pendingPageBreak = true;
    }

    // </li> closes: if the bullet never got inline text (empty <li> or <li> with only
    // block children that were flushed), clear the flag so the next sibling doesn't
    // merge into this block.
    if (named == BB_HTML_TAG_LI) {
      self->listItemBulletOnly = false;
    }
  }

  // </ul> or </ol> closes: pop its list context so a following sibling list at the
  // same nesting level starts its own counter/style instead of inheriting this one's.
  // Guarded by depth (not just tag name + non-empty stack): a display:none <ul>/<ol>
  // returns early in startElement without ever pushing a context (see the
  // hasDisplay()/CssDisplay::None branch above), so its closing tag must not pop
  // the *parent* list's context out from under still-unprocessed siblings.
  if ((named == BB_HTML_TAG_UL || named == BB_HTML_TAG_OL) && !self->listStack.empty() &&
      self->listStack.back().depth == self->depth) {
    self->listStack.pop_back();
  }
  if (named == BB_HTML_TAG_BODY) {
    self->insideBody = false;
  }
  if (named == BB_HTML_TAG_HTML) {
    self->htmlEnded_ = true;
  }
}

ChapterHtmlSlimParser::~ChapterHtmlSlimParser() {
  abortParse();
  bb_anchors_free(&anchors);
  bb_footnotes_free(&notes);
}

std::string_view ChapterHtmlSlimParser::anchorAt(const size_t i, uint16_t& page) const {
  size_t len = 0;
  const char* name = bb_anchors_get(&anchors, i, &len, &page);
  return name ? std::string_view(name, len) : std::string_view();
}

std::optional<uint16_t> ChapterHtmlSlimParser::findAnchor(const std::string_view name) const {
  uint16_t page = 0;
  if (!bb_anchors_find(&anchors, name.data(), name.size(), &page)) return std::nullopt;
  return page;
}

bool ChapterHtmlSlimParser::beginParse() {
  htmlEnded_ = false;
  // Initialize block style stack with a root entry representing "no ancestor block elements".
  // The user's paragraph alignment is set as the default so child elements without explicit
  // text-align inherit it correctly through getCombinedBlockStyle.
  BlockStyle rootBlockStyle;
  rootBlockStyle.alignment = (this->paragraphAlignment == static_cast<uint8_t>(CssTextAlign::None))
                                 ? CssTextAlign::Justify
                                 : static_cast<CssTextAlign>(this->paragraphAlignment);
  blockStyleStack.clear();
  blockStyleStack.reserve(8);
  blockStyleStack.push_back(rootBlockStyle);
  bb_pager_init(&pager, viewportHeight);
  currentPage.reset();
  nextPage.reset();
  dropCap = DropCapState{};

  listStack.clear();
  listStack.reserve(4);
  tableDepth = 0;
  insideTableCell = false;
  tableRowStacked = false;
  tableRowsSpannedRemaining = 0;
  tableRowTextBytes = 0;
  tableRowCells.clear();
  for (auto& lines : tableCellLines) {
    lines.clear();
  }
  tableLineVisibleOffsets.clear();

  auto paragraphAlignmentBlockStyle = BlockStyle();
  paragraphAlignmentBlockStyle.textAlignDefined = true;
  const auto align = rootBlockStyle.alignment;
  paragraphAlignmentBlockStyle.alignment = align;
  startNewTextBlock(paragraphAlignmentBlockStyle);

  if (!xmlParser_.create()) {
    LOG_ERR("EHP", "Couldn't allocate memory for parser");
    return false;
  }

  // HTML entities (like &nbsp;), with or without an external DTD (which the parser does not read)
  xmlParser_.skipUndefinedEntities();
  xmlParser_.setSkippedEntityHandler(skippedEntity);

  if (!Storage.openFileForRead("EHP", filepath, parseFile_)) {
    xmlParser_.destroy();
    return false;
  }

  // Get file size to decide whether to show indexing popup.
  if (popupFn && parseFile_.size() >= MIN_SIZE_FOR_POPUP) {
    popupFn();
  }

  xmlParser_.setUserData(this);
  xmlParser_.setElementHandler(startElement, endElement);
  xmlParser_.setCharacterDataHandler(characterData);

  parseStartTime_ = millis();
  return true;
}

ChapterHtmlSlimParser::ParseStatus ChapterHtmlSlimParser::parseStep() {
  // Layout OOM latched during the previous buffer's callbacks: fail the build
  // instead of emitting pages with silently missing text.
  if (layoutOom || (currentTextBlock && currentTextBlock->hadDroppedWords())) {
    LOG_ERR("EHP", "Text layout dropped content (OOM); failing section build");
    return ParseStatus::Error;
  }

  const size_t len = parseFile_.read(parseBuffer_, PARSE_BUFFER_SIZE);

  if (len == 0 && parseFile_.available() > 0) {
    LOG_ERR("EHP", "File read error");
    return ParseStatus::Error;
  }

  const int done = parseFile_.available() == 0;

  if (!xmlParser_.parse(parseBuffer_, len, done)) {
    if (htmlEnded_) {
      LOG_DBG("EHP", "Ignoring trailing data after </html>: %s", xmlParser_.errorString());
      return ParseStatus::Done;
    }
    LOG_ERR("EHP", "Parse error at byte %llu:\n%s", static_cast<unsigned long long>(xmlParser_.errorOffset()),
            xmlParser_.errorString());
    return ParseStatus::Error;
  }

  return done ? ParseStatus::Done : ParseStatus::More;
}

void ChapterHtmlSlimParser::abortParse() {
  xmlParser_.destroy();
  // Only close the file if it was successfully opened in beginParse()
  if (parseFile_.isOpen()) {
    parseFile_.close();
  }
}

bool ChapterHtmlSlimParser::finishParse() {
  // Same check as parseStep(): drops in the final buffer would otherwise slip
  // through because Done is returned before the next step's check runs.
  if (layoutOom || (currentTextBlock && currentTextBlock->hadDroppedWords())) {
    LOG_ERR("EHP", "Text layout dropped content (OOM); failing section build");
    return false;
  }

  if (xmlParser_) {
    LOG_DBG("EHP", "Time to parse and build pages: %lu ms", millis() - parseStartTime_);
    xmlParser_.destroy();
  }
  parseFile_.close();

  // Process last page if there is still text
  if (currentTextBlock) {
    makePages();
    // Re-check: makePages() latches layoutOom for lines dropped DURING this
    // final layout, which the entry check above cannot have seen.
    if (layoutOom) {
      LOG_ERR("EHP", "Text layout dropped content (OOM); failing section build");
      return false;
    }
    if (!bb_anchors_record_pending(&anchors, bb_pager_completed(&pager))) {
      LOG_ERR("EHP", "OOM: anchor");
      return false;
    }
    const bb_pages pages = pageCallbacks();
    bb_pager_finish(&pager, &pages, visibleTextOffset);
    currentPage.reset();
    currentTextBlock.reset();
  }

  return true;
}

bool ChapterHtmlSlimParser::parseAndBuildPages() {
  if (!beginParse()) {
    return false;
  }
  for (;;) {
    const ParseStatus status = parseStep();
    if (status == ParseStatus::Error) {
      abortParse();
      return false;
    }
    if (status == ParseStatus::Done) {
      break;
    }
  }
  return finishParse();
}

void ChapterHtmlSlimParser::applyBlockFontScale(BlockStyle& blockStyle, const CssStyle& cssStyle,
                                                const uint8_t headingLevel) const {
  blockStyle.fontScale = bb_css_block_scale(cssOf(cssStyle), headingLevel,
                                            blockStyleStack.empty() ? 1.0f : blockStyleStack.back().fontScale,
                                            inlineStyleStack.data(), inlineStyleStack.size());
  blockStyle.fontScaleDefined = true;
}

int ChapterHtmlSlimParser::fontIdForScale(const float scale) const {
  const int variant = renderer.resolveFontVariant(fontId, scale);
  return variant != 0 ? variant : fontId;
}

int ChapterHtmlSlimParser::prepareBlockFont(ParsedText& text, const float scale) {
  BlockStyle& blockStyle = text.getBlockStyle();
  const int layoutFontId = fontIdForScale(scale);
  for (uint8_t slot = 1; slot < text.sizeSlotsInUse(); ++slot) {
    text.setSizeSlotFontId(slot, fontIdForScale(text.sizeSlotScale(slot)));
  }
  blockStyle.fontId = layoutFontId == fontId ? 0 : layoutFontId;
  return layoutFontId;
}

void ChapterHtmlSlimParser::addLineToPage(std::unique_ptr<TextBlock> line, const uint32_t visibleOffset) {
  const int lineFontId = line->blockFontId(fontId);
  const int rubyShift = line->getRubyShift(renderer.getFontAscenderSize(lineFontId));
  // Taller inline words push the shared baseline down.
  const int baseLineHeight = renderer.getLineHeight(lineFontId, lineCompression) + line->extraAscent(renderer, fontId);
  const int lineHeight = baseLineHeight + rubyShift;

  const bb_pager_line end{layoutParagraph != nullptr, layoutParagraphHasDropCap,
                          layoutParagraph && layoutParagraph->linesAfterCurrentLine() == 0};
  const bb_pages pages = pageCallbacks();
  const int16_t y = bb_pager_line_top(&pager, &pages, lineHeight, visibleOffset, &end);
  if (!currentPage) return;  // out of memory: the build fails

  // Track cumulative words to assign footnotes to the page containing their anchor
  wordsExtractedInBlock += line->wordCount();
  addFootnotesToPage(wordsExtractedInBlock);

  // Apply horizontal left inset (margin + padding) as x position offset
  const int16_t xOffset = line->getBlockStyle().leftInset();
  for (const auto& link : line->takeLinkSpans()) {
    if (!currentPage->addLink(link.href, static_cast<int16_t>(xOffset + link.x),
                              static_cast<int16_t>(y + rubyShift - link.topLift), link.width,
                              static_cast<int16_t>(baseLineHeight + link.topLift))) {
      LOG_DBG("EHP", "Dropped page link: %.48s", link.href);
    }
  }
  auto pageLine = makeUniqueNoThrow<PageLine>(std::move(line), xOffset, y);
  if (!pageLine) {
    LOG_ERR("EHP", "OOM: PageLine");
    return;
  }
  currentPage->elements.push_back(std::move(pageLine));
  bb_pager_line_added(&pager, lineHeight, visibleOffset, layoutParagraph != nullptr);
}

void ChapterHtmlSlimParser::makePages(const bool includeLastLine, const bool paragraphEnd) {
  if (!currentTextBlock) {
    LOG_ERR("EHP", "!! No text block to make pages for !!");
    return;
  }

  // Latch before layout: startNewTextBlock() replaces the block right after
  // this returns, which would otherwise lose its dropped-words flag before
  // parseStep()/finishParse() get to check it.
  if (currentTextBlock->hadDroppedWords()) {
    layoutOom = true;
  }

  const bb_pages pages = pageCallbacks();
  bb_pager_ensure_page(&pager, &pages);
  if (!currentTextBlock->isEmpty()) applyPendingPageBreak();

  const int lineHeight = renderer.getLineHeight(fontId, lineCompression);

  const BlockStyle& blockStyle = currentTextBlock->getBlockStyle();

  layoutCurrentBlock(includeLastLine);
  if (includeLastLine) {
    // A heading's lines on this page wait to move with what follows it.
    bb_pager_end_block(&pager, blockStyle.keepWithNext);
    layoutParagraphHasDropCap = false;
  }

  // Latch again after layout: it can drop a whole line (out of memory for the
  // line or its TextBlock) during the call above, after the pre-layout latch ran, and the
  // block is replaced right after this returns.
  if (currentTextBlock->hadDroppedWords()) {
    layoutOom = true;
  }

  // Trailing spacing and footnotes only apply when the block is finalized
  if (includeLastLine) {
    // Fallback: transfer any remaining pending footnotes to current page.
    // Normally addLineToPage handles this via word-index tracking, but this catches
    // edge cases where a footnote's word index equals the exact block size.
    addFootnotesToPage(INT32_MAX);

    if (!paragraphEnd) return;

    // Apply bottom spacing after the paragraph (stored in pixels)
    if (blockStyle.marginBottom > 0) {
      bb_pager_advance(&pager, blockStyle.marginBottom);
    }
    if (blockStyle.paddingBottom > 0) {
      bb_pager_advance(&pager, blockStyle.paddingBottom);
    }

    // Extra paragraph spacing if enabled (default behavior)
    if (extraParagraphSpacing) {
      bb_pager_advance(&pager, lineHeight / 2);
    }
  }
}
