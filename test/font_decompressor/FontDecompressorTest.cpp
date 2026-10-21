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

// Glyph i is codepoint 'A' + i.
constexpr EpdUnicodeInterval kIntervals[] = {{'A', 'A' + kGlyphCount - 1, 0}};
// The same groups, as a frequency-grouped font names them.
constexpr uint16_t kGlyphToGroup[] = {0, 0, 0, 1, 1, 1, 2};
// A + B -> D.
const EpdLigaturePair kLigatures[] = {{'A' << 16 | 'B', 'D'}};

class FontDecompressorPrewarmTest : public FontDecompressorTest {
 protected:
  void SetUp() override {
    FontDecompressorTest::SetUp();
    font.intervals = kIntervals;
    font.intervalCount = 1;
  }

  // A prewarmed glyph comes out of the page buffer: no group is decompressed for it.
  bool resident(const uint32_t glyph) {
    const uint32_t misses = decompressor.getStats().cacheMisses;
    return decodes(glyph) && decompressor.getStats().cacheMisses == misses;
  }
};

TEST_F(FontDecompressorPrewarmTest, APagesGlyphsComeFromThePageBuffer) {
  EXPECT_EQ(0, decompressor.prewarmCache(&font, "GACAz"));  // z is not in the font
  for (const uint32_t glyph : {0u, 2u, 6u}) EXPECT_TRUE(resident(glyph)) << "glyph " << glyph;
  EXPECT_FALSE(resident(1));  // not on the page: decompressed on demand
  EXPECT_TRUE(decodes(1));
}

TEST_F(FontDecompressorPrewarmTest, FrequencyGroupedFontsPrewarmToo) {
  font.glyphToGroup = kGlyphToGroup;
  EXPECT_EQ(0, decompressor.prewarmCache(&font, "FEDB"));
  for (const uint32_t glyph : {1u, 3u, 4u, 5u}) EXPECT_TRUE(resident(glyph)) << "glyph " << glyph;
  for (uint32_t glyph = 0; glyph < kGlyphCount; ++glyph) EXPECT_TRUE(decodes(glyph)) << "glyph " << glyph;
}

TEST_F(FontDecompressorPrewarmTest, ALigatureOutputJoinsAPageWithBothItsParts) {
  font.ligaturePairs = kLigatures;
  font.ligaturePairCount = 1;
  EXPECT_EQ(0, decompressor.prewarmCache(&font, "AB"));
  EXPECT_TRUE(resident(3));
  decompressor.clearCache();
  EXPECT_EQ(0, decompressor.prewarmCache(&font, "A"));
  EXPECT_FALSE(resident(3));
}

TEST_F(FontDecompressorPrewarmTest, EightFontsCanBePrewarmedAtOnce) {
  for (int slot = 0; slot < FontDecompressor::MAX_PAGE_SLOTS; ++slot) {
    EXPECT_EQ(0, decompressor.prewarmCache(&font, "A")) << slot;
  }
  EXPECT_EQ(-1, decompressor.prewarmCache(&font, "B"));
  decompressor.clearCache();
  EXPECT_EQ(0, decompressor.prewarmCache(&font, "B"));
}

TEST_F(FontDecompressorPrewarmTest, AGroupThatFailsLeavesItsGlyphsOutOfThePage) {
  data[groups[1].compressedOffset] = 0x07;  // a final block of the reserved type 11
  EXPECT_EQ(1, decompressor.prewarmCache(&font, "ADG"));
  EXPECT_TRUE(resident(0));
  EXPECT_TRUE(resident(6));
  EXPECT_FALSE(decodes(3));
}

TEST_F(FontDecompressorTest, AGlyphOutsideEveryGroupGivesNoBitmap) {
  font.groupCount = 2;  // the last group is gone
  EXPECT_EQ(nullptr, decompressor.getBitmap(&font, &kGlyphs[6], 6));
  EXPECT_TRUE(decodes(5));
}

TEST_F(FontDecompressorTest, AnUncompressedFontIsReadInPlace) {
  font.groups = nullptr;
  font.groupCount = 0;
  EXPECT_EQ(&font.bitmap[kGlyphs[4].dataOffset], decompressor.getBitmap(&font, &kGlyphs[4], 4));
}

}  // namespace
