// FontDecompressor's hot-group path over a compressed font (FontDecompressorFixture.h, from
// make_fixture.py): each glyph comes out packed from its group, and a group that cannot be
// decoded gives no bitmap.

#include <gtest/gtest.h>

#include <cstring>
#include <vector>

#include "FontDecompressor.h"
#include "FontDecompressorFixture.h"

namespace {

constexpr uint32_t kGlyphCount = sizeof(kGlyphs) / sizeof(kGlyphs[0]);

class FontDecompressorTest : public ::testing::Test {
 protected:
  void SetUp() override {
    data.assign(std::begin(kGroupData), std::end(kGroupData));
    std::memcpy(groups, kGroups, sizeof(groups));
    font.bitmap = data.data();
    font.glyph = kGlyphs;
    font.groups = groups;
    font.groupCount = sizeof(groups) / sizeof(groups[0]);
    ASSERT_TRUE(decompressor.init());
  }

  bool decodes(const uint32_t glyph) {
    const uint8_t* bitmap = decompressor.getBitmap(&font, &kGlyphs[glyph], glyph);
    return bitmap && std::memcmp(bitmap, kPacked[glyph], kGlyphs[glyph].dataLength) == 0;
  }

  std::vector<uint8_t> data;
  EpdFontGroup groups[sizeof(kGroups) / sizeof(kGroups[0])];
  EpdFontData font{};
  FontDecompressor decompressor;
};

TEST_F(FontDecompressorTest, EveryGlyphComesOutOfItsGroup) {
  // Hopping between groups decompresses each one again.
  for (const uint32_t glyph : {0u, 3u, 6u, 1u, 4u, 2u, 5u, 0u}) {
    EXPECT_TRUE(decodes(glyph)) << "glyph " << glyph;
  }
  EXPECT_EQ(decompressor.getStats().cacheMisses, 8u);
  for (uint32_t glyph = 0; glyph < kGlyphCount; ++glyph) EXPECT_TRUE(decodes(glyph)) << "glyph " << glyph;
}

TEST_F(FontDecompressorTest, ADamagedGroupGivesNoBitmap) {
  data[groups[1].compressedOffset] = 0x07;  // a final block of the reserved type 11
  EXPECT_FALSE(decodes(3));
  EXPECT_TRUE(decodes(0)) << "the other groups still decode";
  EXPECT_FALSE(decodes(4)) << "the failed group was not kept";
}

TEST_F(FontDecompressorTest, ACutGroupGivesNoBitmap) {
  groups[2].compressedSize -= 20;
  EXPECT_FALSE(decodes(6));
}

TEST_F(FontDecompressorTest, AGroupThatEndsShortGivesNoBitmap) {
  groups[2].uncompressedSize += 1;
  EXPECT_FALSE(decodes(6));
}

}  // namespace
