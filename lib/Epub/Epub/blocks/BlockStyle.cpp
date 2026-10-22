#include "BlockStyle.h"

#include <bembeyaz/css.h>

#include <cstddef>

// Rust's bembeyaz_css::block::Block asserts the same layout.
static_assert(sizeof(BlockStyle) == 40 && alignof(BlockStyle) == 4 && offsetof(BlockStyle, marginTop) == 2 &&
                  offsetof(BlockStyle, textIndent) == 18 && offsetof(BlockStyle, textIndentDefined) == 20 &&
                  offsetof(BlockStyle, directionDefined) == 23 && offsetof(BlockStyle, fontScale) == 24 &&
                  offsetof(BlockStyle, fontScaleDefined) == 28 && offsetof(BlockStyle, fontId) == 32 &&
                  offsetof(BlockStyle, keepWithNext) == 38,
              "BlockStyle must match bb_css_block");
static_assert(static_cast<uint8_t>(BlockStyle::CombineAxis::Horizontal) == BB_CSS_HORIZONTAL &&
              static_cast<uint8_t>(BlockStyle::CombineAxis::Vertical) == BB_CSS_VERTICAL);

namespace {
const bb_css_block* rust(const BlockStyle& style) { return reinterpret_cast<const bb_css_block*>(&style); }
bb_css_block* rust(BlockStyle& style) { return reinterpret_cast<bb_css_block*>(&style); }
}  // namespace

BlockStyle BlockStyle::addBottom(const BlockStyle& source) const {
  BlockStyle result = *this;
  bb_css_block_add_bottom(rust(result), rust(source));
  return result;
}

BlockStyle BlockStyle::getCombinedBlockStyle(const BlockStyle& child, const CombineAxis axis) const {
  BlockStyle result = child;
  bb_css_block_combine(rust(*this), rust(result), static_cast<uint8_t>(axis));
  return result;
}

BlockStyle BlockStyle::fromCssStyle(const CssStyle& cssStyle, const float emSize, const CssTextAlign paragraphAlignment,
                                    const uint16_t viewportWidth) {
  BlockStyle blockStyle;
  bb_css_block_from_style(reinterpret_cast<const bb_css_style*>(&cssStyle), emSize,
                          static_cast<uint8_t>(paragraphAlignment), viewportWidth, rust(blockStyle));
  return blockStyle;
}
