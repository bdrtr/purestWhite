#include <bembeyaz/css.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <random>

#include "CssStyle.h"
#include "OldImageSize.h"

namespace {

const bb_css_style* cssOf(const CssStyle& style) { return reinterpret_cast<const bb_css_style*>(&style); }

std::pair<int, int> newSize(const CssStyle& style, int16_t w, int16_t h, float em, int container, uint16_t page) {
  int width = -7;
  int height = -7;
  bb_css_image_size(cssOf(style), w, h, em, container, page, &width, &height);
  return {width, height};
}

std::pair<int, int> oldSize(const CssStyle& style, int16_t w, int16_t h, float em, int container, uint16_t page) {
  int width = -7;
  int height = -7;
  old_image_size::displaySize(style, w, h, em, container, page, width, height);
  return {width, height};
}

TEST(CssImageSize, MatchesTheOldParser) {
  std::mt19937 rng(20261024);
  const auto pick = [&](int lo, int hi) { return std::uniform_int_distribution<int>(lo, hi)(rng); };
  const auto length = [&]() {
    CssLength l;
    l.unit = static_cast<CssUnit>(pick(0, 4));
    // Whole, fractional, zero, negative and oversized values, all within int range once resolved.
    switch (pick(0, 4)) {
      case 0:
        l.value = static_cast<float>(pick(-20, 3000));
        break;
      case 1:
        l.value = std::uniform_real_distribution<float>(-5.0f, 400.0f)(rng);
        break;
      case 2:
        l.value = 0.0f;
        break;
      case 3:
        l.value = std::uniform_real_distribution<float>(0.0f, 2.0f)(rng);
        break;
      default:
        l.value = static_cast<float>(pick(90, 110));
    }
    return l;
  };
  const auto natural = [&]() -> int16_t {
    switch (pick(0, 5)) {
      case 0:
        return 0;
      case 1:
        return static_cast<int16_t>(pick(-32768, -1));
      case 2:
        return static_cast<int16_t>(pick(1, 20));
      default:
        return static_cast<int16_t>(pick(1, 32767));
    }
  };
  for (int i = 0; i < 200000; ++i) {
    CssStyle style;
    if (pick(0, 2) != 0) {
      style.imageWidth = length();
      style.defined.imageWidth = 1;
    }
    if (pick(0, 2) != 0) {
      style.imageHeight = length();
      style.defined.imageHeight = 1;
    }
    // A length can also be stored without its defined bit.
    if (pick(0, 9) == 0) style.imageWidth = length();
    const int16_t w = natural();
    const int16_t h = natural();
    const float em = static_cast<float>(pick(1, 60)) + (pick(0, 1) ? 0.5f : 0.0f);
    const int container = pick(1, 481);
    const auto page = static_cast<uint16_t>(pick(1, 800));
    ASSERT_EQ(newSize(style, w, h, em, container, page), oldSize(style, w, h, em, container, page))
        << "case " << i << " natural " << w << "x" << h << " em " << em << " container " << container << " page "
        << page;
  }
}

TEST(CssImageSize, WritesNothingWithoutOutputs) {
  CssStyle style;
  bb_css_image_size(cssOf(style), 10, 10, 10.0f, 100, 100, nullptr, nullptr);
  bb_css_image_size(nullptr, 1000, 500, 10.0f, 400, 800, nullptr, nullptr);
  int width = 0;
  int height = 0;
  bb_css_image_size(nullptr, 1000, 500, 10.0f, 400, 800, &width, &height);
  EXPECT_EQ(width, 400);
  EXPECT_EQ(height, 200);
}

}  // namespace
