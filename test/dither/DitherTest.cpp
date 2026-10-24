// bembeyaz-dither (through BitmapHelpers.h's Ditherer) against the C++ ditherers and quantizers it
// replaced (OldBitmapHelpers.h), row by row as Bitmap and the cover converters drive them: random
// widths, gray rows and methods, the rows handed over in random pieces.
#include <gtest/gtest.h>

#include <cstdint>
#include <vector>

#include "OldBitmapHelpers.h"
#include "lib/GfxRenderer/BitmapHelpers.h"

namespace {
uint32_t seed = 3;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}
int range(int lo, int hi) { return lo + static_cast<int>(rnd() % static_cast<uint32_t>(hi - lo + 1)); }

// One old row the way the callers built it: the level of each pixel, packed most significant first.
template <typename Level>
std::vector<uint8_t> oldRow(const std::vector<uint8_t>& gray, const int bits, Level level) {
  std::vector<uint8_t> out((gray.size() * bits + 7) / 8, 0);
  const int perByte = 8 / bits;
  for (size_t x = 0; x < gray.size(); x++) {
    const uint8_t v = level(gray[x], static_cast<int>(x));
    out[x / perByte] |= static_cast<uint8_t>(v << (8 - bits - (x % perByte) * bits));
  }
  return out;
}
}  // namespace

TEST(Dither, MatchesTheOldDitherersAndQuantizers) {
  unsigned diffs = 0;
  for (int c = 0; c < 4000; ++c) {
    const uint8_t method = static_cast<uint8_t>(range(0, 4));
    const bool original = rnd() % 2;
    const int width = range(1, 90);
    const int rows = range(1, 12);
    // Smooth gradients and noise, both.
    std::vector<std::vector<uint8_t>> gray(rows, std::vector<uint8_t>(width));
    for (int y = 0; y < rows; y++) {
      for (int x = 0; x < width; x++) {
        gray[y][x] = rnd() % 3 == 0 ? static_cast<uint8_t>(rnd()) : static_cast<uint8_t>((x * 7 + y * 13 + c) & 0xFF);
      }
    }
    const int bits = method >= BB_DITHER_ATKINSON_1BIT ? 1 : 2;

    old::AtkinsonDitherer atkinson(width, original);
    old::FloydSteinbergDitherer floyd(width, original);
    old::Atkinson1BitDitherer atkinson1(width);
    Ditherer ditherer(method, width, original);
    ASSERT_TRUE(ditherer.isValid());
    for (int y = 0; y < rows; y++) {
      std::vector<uint8_t> expected;
      switch (method) {
        case BB_DITHER_THRESHOLD:
          expected = oldRow(gray[y], 2, [&](uint8_t g, int x) { return old::quantize(old::adjustPixel(g), x, y); });
          break;
        case BB_DITHER_NATIVE:
          expected = oldRow(gray[y], 2, [](uint8_t g, int) { return static_cast<uint8_t>(old::adjustPixel(g) >> 6); });
          break;
        case BB_DITHER_ATKINSON:
          expected =
              oldRow(gray[y], 2, [&](uint8_t g, int x) { return atkinson.processPixel(old::adjustPixel(g), x); });
          atkinson.nextRow();
          break;
        case BB_DITHER_FLOYD_STEINBERG:
          expected = oldRow(gray[y], 2, [&](uint8_t g, int x) { return floyd.processPixel(old::adjustPixel(g), x); });
          floyd.nextRow();
          break;
        default:
          expected = oldRow(gray[y], 1, [&](uint8_t g, int x) { return atkinson1.processPixel(g, x); });
          atkinson1.nextRow();
          break;
      }
      // The new row in random pieces, over a row already full of other bits.
      const uint8_t before = static_cast<uint8_t>(rnd());
      std::vector<uint8_t> actual(expected.size(), before);
      int x = 0;
      while (x < width) {
        const int count = std::min(width - x, range(1, 40));
        ditherer.pixels(gray[y].data() + x, count, x, y, actual.data(), actual.size());
        x += count;
      }
      // Bits past the last pixel are the caller's: left as they were.
      const int tailBits = static_cast<int>(actual.size()) * 8 - width * bits;
      if (tailBits > 0) {
        const uint8_t tailMask = static_cast<uint8_t>(0xFF >> (8 - tailBits));
        EXPECT_EQ(actual.back() & tailMask, before & tailMask) << "case " << c;
        actual.back() &= static_cast<uint8_t>(~tailMask);
      }
      ditherer.nextRow();
      if (actual != expected) {
        ADD_FAILURE() << "case " << c << " method " << static_cast<int>(method) << " original " << original << " width "
                      << width << " row " << y;
        if (++diffs == 5) return;
        break;
      }
    }
  }
}

// Rewinding a Bitmap resets its ditherer: the same rows come out again.
TEST(Dither, ResetStartsOver) {
  const std::vector<uint8_t> gray = {10, 60, 100, 140, 200, 250, 30, 90};
  for (const uint8_t method : {BB_DITHER_ATKINSON, BB_DITHER_FLOYD_STEINBERG, BB_DITHER_ATKINSON_1BIT}) {
    Ditherer ditherer(method, 8);
    std::vector<uint8_t> first(2), again(2);
    for (int y = 0; y < 3; y++) {
      ditherer.pixels(gray.data(), 8, 0, y, first.data(), first.size());
      ditherer.nextRow();
    }
    ditherer.reset();
    ditherer.pixels(gray.data(), 8, 0, 0, again.data(), again.size());
    Ditherer fresh(method, 8);
    std::vector<uint8_t> expected(2);
    fresh.pixels(gray.data(), 8, 0, 0, expected.data(), expected.size());
    EXPECT_EQ(again, expected) << static_cast<int>(method);
  }
}
