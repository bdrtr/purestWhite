// Pins SdCardFont::load: the .cpfont header, the style table of contents and the interval tables
// it validates before anything reads glyphs.
#include <HalStorage.h>
#include <SdCardFont.h>
#include <gtest/gtest.h>

#include <cstring>
#include <vector>

namespace {

struct Interval {
  uint32_t first;
  uint32_t last;
  uint32_t offset;
};

struct Style {
  uint8_t id = 0;
  std::vector<Interval> intervals = {{'A', 'Z', 0}};
  uint32_t glyphs = 26;
  uint16_t kernLeft = 0;
  uint16_t kernRight = 0;
  uint8_t kernLeftClasses = 0;
  uint8_t kernRightClasses = 0;
  uint8_t ligatures = 0;
};

void put(std::vector<uint8_t>& v, const size_t at, const uint64_t value, const int bytes) {
  if (v.size() < at + bytes) v.resize(at + bytes, 0);
  for (int i = 0; i < bytes; i++) v[at + i] = static_cast<uint8_t>(value >> (8 * i));
}

// A font with the given styles, each style's intervals stored after the table of contents.
std::vector<uint8_t> makeFont(const std::vector<Style>& styles, const bool twoBit = false,
                              const uint16_t version = CPFONT_VERSION, const int styleCount = -1) {
  std::vector<uint8_t> v(32, 0);
  std::memcpy(v.data(), "CPFONT\0\0", 8);
  put(v, 8, version, 2);
  put(v, 10, twoBit ? 1 : 0, 2);
  v[12] = static_cast<uint8_t>(styleCount < 0 ? styles.size() : styleCount);
  size_t data = 32 + 32 * styles.size();
  v.resize(data, 0);
  for (size_t i = 0; i < styles.size(); i++) {
    const Style& s = styles[i];
    const size_t toc = 32 + 32 * i;
    put(v, toc, s.id, 1);
    put(v, toc + 4, s.intervals.size(), 4);
    put(v, toc + 8, s.glyphs, 4);
    put(v, toc + 12, 30 + i, 1);
    put(v, toc + 13, static_cast<uint16_t>(24 + i), 2);
    put(v, toc + 15, static_cast<uint16_t>(-6), 2);
    put(v, toc + 17, s.kernLeft, 2);
    put(v, toc + 19, s.kernRight, 2);
    put(v, toc + 21, s.kernLeftClasses, 1);
    put(v, toc + 22, s.kernRightClasses, 1);
    put(v, toc + 23, s.ligatures, 1);
    put(v, toc + 24, data, 4);
    for (const Interval& iv : s.intervals) {
      put(v, data, iv.first, 4);
      put(v, data + 4, iv.last, 4);
      put(v, data + 8, iv.offset, 4);
      data += 12;
    }
  }
  return v;
}

bool load(SdCardFont& font, std::vector<uint8_t> file) {
  sdFontTestFile = std::move(file);
  return font.load("/font.cpfont");
}

uint32_t fnv(const std::vector<uint8_t>& v, const size_t len) {
  uint32_t hash = 2166136261u;
  for (size_t i = 0; i < len; i++) {
    hash ^= v[i];
    hash *= 16777619u;
  }
  return hash;
}

}  // namespace

TEST(SdCardFontLoad, ReadsHeaderStylesAndCoverage) {
  SdCardFont font;
  Style bold{2, {{'a', 'c', 0}, {0x1F600, 0x1F601, 3}}, 5};
  const auto file = makeFont({Style{}, bold}, true);
  ASSERT_TRUE(load(font, file));
  EXPECT_EQ(font.styleCount(), 2);
  EXPECT_TRUE(font.hasStyle(0));
  EXPECT_FALSE(font.hasStyle(1));
  EXPECT_TRUE(font.hasStyle(2));
  EXPECT_EQ(font.contentHash(), fnv(file, 32 + 2 * 32));
  const EpdFontData* regular = font.getEpdFont(0)->data;
  EXPECT_EQ(regular->advanceY, 30);
  EXPECT_EQ(regular->ascender, 24);
  EXPECT_EQ(regular->descender, -6);
  EXPECT_TRUE(regular->is2Bit);
  EXPECT_TRUE(font.getEpdFont(0)->hasCodepoint('M'));
  EXPECT_FALSE(font.getEpdFont(0)->hasCodepoint('a'));
  // Above the BMP: the full-width interval table.
  EXPECT_TRUE(font.getEpdFont(2)->hasCodepoint(0x1F601));
  EXPECT_TRUE(font.getEpdFont(2)->hasCodepoint('b'));
  EXPECT_FALSE(font.getEpdFont(2)->hasCodepoint('d'));
}

TEST(SdCardFontLoad, StylesWithTheSameCoverageShareIt) {
  SdCardFont font;
  Style italic{1};
  Style other{3, {{'A', 'Y', 0}}, 26};
  ASSERT_TRUE(load(font, makeFont({Style{}, italic, other})));
  for (const uint8_t style : {0, 1}) {
    EXPECT_TRUE(font.getEpdFont(style)->hasCodepoint('Z')) << int(style);
  }
  EXPECT_FALSE(font.getEpdFont(3)->hasCodepoint('Z'));
  font.releaseResidentCaches();
  EXPECT_TRUE(font.getEpdFont(1)->hasCodepoint('A'));
}

TEST(SdCardFontLoad, RefusesBadHeaders) {
  SdCardFont font;
  auto magic = makeFont({Style{}});
  magic[3] = 'X';
  EXPECT_FALSE(load(font, magic));
  EXPECT_FALSE(load(font, makeFont({Style{}}, false, CPFONT_VERSION + 1)));
  EXPECT_FALSE(load(font, makeFont({}, false, CPFONT_VERSION, 0)));
  EXPECT_FALSE(load(font, makeFont({Style{}, Style{1}, Style{2}, Style{3}, Style{0}})));
  auto shortHeader = makeFont({Style{}});
  shortHeader.resize(31);
  EXPECT_FALSE(load(font, shortHeader));
  auto shortToc = makeFont({Style{}});
  shortToc.resize(32 + 20);
  EXPECT_FALSE(load(font, shortToc));
  EXPECT_FALSE(load(font, makeFont({Style{4}})));
}

TEST(SdCardFontLoad, RefusesUnreasonableCounts) {
  SdCardFont font;
  Style many{};
  many.glyphs = 65537;
  EXPECT_FALSE(load(font, makeFont({many})));
  Style glyphs{};
  glyphs.glyphs = 65536;
  glyphs.intervals = {{'A', 'Z', 0}};
  EXPECT_TRUE(load(font, makeFont({glyphs})));
  Style kern{};
  kern.kernLeft = 4097;
  EXPECT_FALSE(load(font, makeFont({kern})));
  kern.kernLeft = 4096;
  kern.kernRight = 4097;
  EXPECT_FALSE(load(font, makeFont({kern})));
  auto intervals = makeFont({Style{}});
  put(intervals, 32 + 4, 4097, 4);
  EXPECT_FALSE(load(font, intervals));
}

TEST(SdCardFontLoad, RefusesBadIntervalTables) {
  SdCardFont font;
  const auto refuse = [&](std::vector<Interval> intervals, const uint32_t glyphs) {
    Style s{0, std::move(intervals), glyphs};
    return !load(font, makeFont({s}));
  };
  EXPECT_TRUE(refuse({{'Z', 'A', 0}}, 26));                      // reversed
  EXPECT_TRUE(refuse({{'A', 'M', 0}, {'M', 'Z', 13}}, 27));      // overlapping
  EXPECT_TRUE(refuse({{'A', 'M', 0}, {'N', 'Z', 14}}, 27));      // offset gap
  EXPECT_TRUE(refuse({{'A', 'Z', 0}}, 25));                      // span past the glyphs
  EXPECT_TRUE(refuse({{'A', 'B', 0}, {'D', 'Z', 2}}, 20));       // offsets past the glyphs
  EXPECT_FALSE(refuse({{'A', 'M', 0}, {'O', 'Z', 13}}, 25));     // a gap in codepoints is fine
  EXPECT_FALSE(refuse({}, 0));                                   // no intervals
  auto cut = makeFont({Style{0, {{'A', 'M', 0}, {'O', 'Z', 13}}, 25}});
  cut.resize(cut.size() - 4);
  EXPECT_FALSE(load(font, cut));
  auto far = makeFont({Style{}});
  put(far, 32 + 24, 100000, 4);
  EXPECT_FALSE(load(font, far));
}

TEST(SdCardFontLoad, AFailedLoadLeavesNothingLoaded) {
  SdCardFont font;
  ASSERT_TRUE(load(font, makeFont({Style{}, Style{1}})));
  auto bad = makeFont({Style{}, Style{1, {{'Z', 'A', 0}}, 26}});
  EXPECT_FALSE(load(font, bad));
  EXPECT_FALSE(font.hasStyle(0));
  EXPECT_FALSE(font.hasStyle(1));
}
