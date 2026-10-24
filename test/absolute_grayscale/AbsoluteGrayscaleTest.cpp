#include <gtest/gtest.h>

#include "lib/GfxRenderer/BitmapHelpers.h"

namespace {
thread_local int rowAllocationToFail = -1;
}

// Fail one row allocation without changing production allocator APIs.
void* operator new[](std::size_t size, const std::nothrow_t&) noexcept {
  if (rowAllocationToFail >= 0 && rowAllocationToFail-- == 0) return nullptr;
  return ::operator new[](size);
}

TEST(AbsoluteGrayscale, DitherersReportScratchAllocationFailure) {
  for (const uint8_t method : {BB_DITHER_ATKINSON, BB_DITHER_ATKINSON_1BIT, BB_DITHER_FLOYD_STEINBERG}) {
    rowAllocationToFail = 0;
    Ditherer failed(method, 8);
    rowAllocationToFail = -1;
    EXPECT_FALSE(failed.isValid()) << static_cast<int>(method);
    Ditherer ditherer(method, 8);
    EXPECT_TRUE(ditherer.isValid()) << static_cast<int>(method);
  }
  // Methods that carry no error need no rows.
  rowAllocationToFail = 0;
  Ditherer threshold(BB_DITHER_THRESHOLD, 8);
  rowAllocationToFail = -1;
  EXPECT_TRUE(threshold.isValid());
}

TEST(AbsoluteGrayscale, FullPlanesIncludeBlackWhiteAndBothGrayLevels) {
  uint8_t planes[2] = {0xff, 0xff};
  for (unsigned p = 0; p < 2; ++p) {
    for (unsigned x = 0; x < 8; ++x) {
      const auto pixel = grayPlanePixel(x % 4, p == 1, true);
      ASSERT_TRUE(pixel.write);
      if (pixel.black)
        planes[p] &= ~(0x80 >> x);
      else
        planes[p] |= 0x80 >> x;
    }
  }
  EXPECT_EQ(planes[0], 0x55);  // black/dark/light/white = 0/1/0/1
  EXPECT_EQ(planes[1], 0x33);  // black/dark/light/white = 0/0/1/1
}

TEST(AbsoluteGrayscale, OverlayStillLeavesBlackAndWhiteToTheBase) {
  uint8_t planes[2] = {0, 0};
  for (unsigned p = 0; p < 2; ++p) {
    for (unsigned x = 0; x < 8; ++x) {
      const auto pixel = grayPlanePixel(x % 4, p == 1, false);
      if (x % 4 == 0 || x % 4 == 3) EXPECT_FALSE(pixel.write);
      if (pixel.write && !pixel.black) planes[p] |= 0x80 >> x;
    }
  }
  EXPECT_EQ(planes[0], 0x44);
  EXPECT_EQ(planes[1], 0x66);
}

namespace {
// The 2-bit level the first pixel of a fresh ditherer gets for gray.
uint8_t firstLevel(const uint8_t method, const uint8_t gray, const bool originalThresholds) {
  Ditherer ditherer(method, 1, originalThresholds);
  uint8_t out = 0;
  ditherer.pixels(&gray, 1, 0, 0, &out, 1);
  return out >> 6;
}
}  // namespace

TEST(AbsoluteGrayscale, ImageQuantizersRetainFourEvenLevels) {
  for (uint8_t level = 0; level < 4; ++level) {
    EXPECT_EQ(firstLevel(BB_DITHER_ATKINSON, level * 85, true), level);
    EXPECT_EQ(firstLevel(BB_DITHER_FLOYD_STEINBERG, level * 85, true), level);
  }
  EXPECT_EQ(firstLevel(BB_DITHER_ATKINSON, 85, false), 2);
}

TEST(AbsoluteGrayscale, TransparentPassesRetainBackgroundWithoutClearingBetweenPlanes) {
  // Alternating B/W background, with four opaque pixels and four transparent ones.
  uint8_t frame = 0xaa;
  uint8_t planes[2] = {};
  for (unsigned plane = 0; plane < 2; ++plane) {
    for (unsigned x = 0; x < 4; ++x) {
      const auto pixel = grayPlanePixel(x, plane == 1, true);
      if (pixel.black)
        frame &= ~(0x80 >> x);
      else
        frame |= 0x80 >> x;
    }
    planes[plane] = frame;
  }
  EXPECT_EQ(planes[0], 0x5a);
  EXPECT_EQ(planes[1], 0x3a);
  EXPECT_EQ(planes[0] & 0x0f, 0x0a);
  EXPECT_EQ(planes[1] & 0x0f, 0x0a);
}
