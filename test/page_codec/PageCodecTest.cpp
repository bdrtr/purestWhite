#include <Epub/Page.h>
#include <GfxRenderer.h>
#include <gtest/gtest.h>
#include <unistd.h>

#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iterator>
#include <memory>
#include <string>
#include <vector>

namespace fs = std::filesystem;

namespace {

// The page body as written today, encoded independently of Page: a u16 element count, then per element its
// tag (u8) and fields, then the footnotes (u16 count, 32 + 256 bytes each) and the links (u16 count, 256-byte
// href and four i16). Packed, little-endian.
struct Bytes {
  std::vector<uint8_t> b;
  template <typename T>
  Bytes& pod(const T value) {
    const auto* p = reinterpret_cast<const uint8_t*>(&value);
    b.insert(b.end(), p, p + sizeof(T));
    return *this;
  }
  Bytes& raw(const void* data, const size_t n) {
    const auto* p = static_cast<const uint8_t*>(data);
    b.insert(b.end(), p, p + n);
    return *this;
  }
  Bytes& str(const std::string& s) {
    pod<uint32_t>(s.size());
    return raw(s.data(), s.size());
  }
  Bytes& fixed(const std::string& s, const size_t n) {
    std::vector<char> field(n, '\0');
    std::memcpy(field.data(), s.data(), std::min(s.size(), n));
    return raw(field.data(), n);
  }
};

template <typename T>
void put(std::vector<uint8_t>& bytes, const size_t at, const T value) {
  std::memcpy(bytes.data() + at, &value, sizeof(T));
}

struct Word {
  std::string text;
  int16_t x;
  uint8_t style;
  uint32_t start, end;
  uint8_t boundary = 0;
  uint16_t suffixX = 0;
  std::string ruby;
  uint8_t slot = 0;
};

struct Line {
  std::vector<Word> words;
  bool focus = false;
  uint16_t paragraphStart = UINT16_MAX;
  BlockStyle style;
  bool fonts = false;
  int32_t fontTable[3] = {0, 0, 0};
};

// A text block's bytes: counts, the arena (ranges, offsets, x, [suffix x], styles, [boundaries], text), the
// ruby strings, the block style, and the optional word fonts.
void encodeLine(Bytes& out, const Line& line) {
  const auto wc = static_cast<uint16_t>(line.words.size());
  uint16_t textBytes = 0;
  for (const auto& w : line.words) textBytes += w.text.size() + 1;
  out.pod(wc).pod<uint8_t>(line.focus).pod(textBytes).pod(line.paragraphStart);
  if (wc > 0) {
    for (const auto& w : line.words) out.pod(w.start).pod(w.end);
    uint16_t off = 0;
    for (const auto& w : line.words) {
      out.pod(off);
      off += w.text.size() + 1;
    }
    for (const auto& w : line.words) out.pod(w.x);
    if (line.focus)
      for (const auto& w : line.words) out.pod(w.suffixX);
    for (const auto& w : line.words) out.pod(w.style);
    if (line.focus)
      for (const auto& w : line.words) out.pod(w.boundary);
    for (const auto& w : line.words) out.raw(w.text.c_str(), w.text.size() + 1);
  }
  for (const auto& w : line.words) out.str(w.ruby);
  const BlockStyle& s = line.style;
  out.pod(s.alignment).pod(s.textAlignDefined).pod(s.marginTop).pod(s.marginBottom).pod(s.marginLeft);
  out.pod(s.marginRight).pod(s.paddingTop).pod(s.paddingBottom).pod(s.paddingLeft).pod(s.paddingRight);
  out.pod(s.textIndent).pod(s.textIndentDefined).pod(s.isRtl).pod(s.directionDefined).pod(s.characterSpacing);
  out.pod(s.fontId);
  out.pod<uint8_t>(line.fonts);
  if (line.fonts) {
    out.raw(line.fontTable, sizeof(line.fontTable));
    for (const auto& w : line.words) out.pod(w.slot);
  }
}

std::unique_ptr<TextBlock> makeBlock(const Line& line) {
  std::vector<std::string> words, rubies;
  std::vector<int16_t> xs;
  std::vector<EpdFontFamily::Style> styles;
  std::vector<uint8_t> boundaries;
  std::vector<uint16_t> suffixes;
  std::vector<TextBlock::SourceRange> ranges;
  std::vector<uint8_t> slots;
  for (const auto& w : line.words) {
    words.push_back(w.text);
    xs.push_back(w.x);
    styles.push_back(static_cast<EpdFontFamily::Style>(w.style));
    if (line.focus) {
      boundaries.push_back(w.boundary);
      suffixes.push_back(w.suffixX);
    }
    ranges.push_back({w.start, w.end});
    rubies.push_back(w.ruby);
    slots.push_back(w.slot);
  }
  auto block = std::make_unique<TextBlock>(words, xs, styles, boundaries, suffixes, line.style, rubies,
                                           std::vector<TextBlock::LinkSpan>{}, ranges, line.paragraphStart);
  if (line.fonts) block->setWordFonts(slots.data(), line.fontTable);
  return block;
}

Line sampleLine() {
  Line line;
  line.focus = true;
  line.paragraphStart = 0;
  line.words = {
      {"Ça", 0, EpdFontFamily::BOLD, 10, 12, 1, 9, "", 0},
      {"va", 20, static_cast<uint8_t>(EpdFontFamily::ITALIC | TextBlock::DISCRETIONARY_HYPHEN_FLAG), 13, 15, 0, 0,
       "かな", 1},
      {"?", 40, EpdFontFamily::REGULAR, 15, 16, 0, 0, "", static_cast<uint8_t>(2 | TextBlock::SMALL_CAPS_SLOT_FLAG)},
  };
  line.style.alignment = CssTextAlign::Center;
  line.style.textAlignDefined = true;
  line.style.marginTop = -3;
  line.style.marginBottom = 4;
  line.style.marginLeft = 5;
  line.style.marginRight = 6;
  line.style.paddingTop = 7;
  line.style.paddingBottom = 8;
  line.style.paddingLeft = 9;
  line.style.paddingRight = 10;
  line.style.textIndent = 11;
  line.style.textIndentDefined = true;
  line.style.isRtl = true;
  line.style.directionDefined = true;
  line.style.characterSpacing = -2;
  line.style.fontId = 77;
  line.fonts = true;
  line.fontTable[0] = 31;
  line.fontTable[1] = 32;
  return line;
}

const CssBorderSide kSides[4] = {
    {2, CssBorderStyle::Solid}, {0, CssBorderStyle::None}, {3, CssBorderStyle::Dashed}, {1, CssBorderStyle::Dotted}};

// The page every round trip uses: a line, a rule, a drop cap, a border box, two footnotes and two links.
Bytes samplePageBytes() {
  Bytes out;
  out.pod<uint16_t>(4);
  out.pod<uint8_t>(TAG_PageLine).pod<int16_t>(5).pod<int16_t>(7);
  encodeLine(out, sampleLine());
  out.pod<uint8_t>(TAG_PageHorizontalRule).pod<int16_t>(-1).pod<int16_t>(90).pod<uint16_t>(100).pod<uint8_t>(2);
  out.pod<uint8_t>(TAG_PageDropCap).pod<int16_t>(3).pod<int16_t>(60).pod<int32_t>(9).pod<uint16_t>(512);
  out.pod<uint8_t>(EpdFontFamily::BOLD)
      .pod<uint8_t>(3)
      .raw(
          "\xC2\xAB"
          "A",
          3);
  out.pod<uint8_t>(TAG_PageBorderBox).pod<int16_t>(0).pod<int16_t>(80).pod<uint16_t>(120).pod<uint16_t>(40);
  for (const auto& side : kSides) out.pod(side.width).pod(static_cast<uint8_t>(side.style));
  out.pod<uint8_t>(1);
  out.pod<uint16_t>(2).fixed("1", 32).fixed("notes.xhtml#n1", 256).fixed("22", 32).fixed("notes.xhtml#n22", 256);
  out.pod<uint16_t>(2);
  out.fixed("ch2.xhtml", 256).pod<int16_t>(1).pod<int16_t>(2).pod<int16_t>(30).pod<int16_t>(12);
  out.fixed("#top", 256).pod<int16_t>(-4).pod<int16_t>(50).pod<int16_t>(8).pod<int16_t>(1);
  return out;
}

std::unique_ptr<Page> samplePage() {
  auto page = std::make_unique<Page>();
  page->elements.push_back(std::make_unique<PageLine>(makeBlock(sampleLine()), 5, 7));
  page->elements.push_back(std::make_unique<PageHorizontalRule>(100, 2, -1, 90));
  page->elements.push_back(std::make_unique<PageDropCap>(9, 512, EpdFontFamily::BOLD,
                                                         "\xC2\xAB"
                                                         "A",
                                                         3, 60));
  page->elements.push_back(std::make_unique<PageBorderBox>(120, 40, kSides, true, 0, 80));
  page->addFootnote("1", "notes.xhtml#n1");
  page->addFootnote("22", "notes.xhtml#n22");
  page->addLink("ch2.xhtml", 1, 2, 30, 12);
  page->addLink("#top", -4, 50, 8, 1);
  return page;
}

class PageCodecTest : public ::testing::Test {
 protected:
  fs::path path;
  void SetUp() override {
    path = fs::temp_directory_path() / ("page_codec_" + std::to_string(::getpid()) + "_" +
                                        ::testing::UnitTest::GetInstance()->current_test_info()->name());
  }
  void TearDown() override { fs::remove(path); }

  std::vector<uint8_t> serialized(const Page& page) const {
    HalFile f;
    EXPECT_TRUE(f.open(path.c_str(), "w+b"));
    EXPECT_TRUE(page.serialize(f));
    f.close();
    return readAll();
  }
  std::vector<uint8_t> readAll() const {
    std::ifstream in(path, std::ios::binary);
    return {std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>()};
  }
  // Decodes `bytes` placed after a few bytes of something else, as a page sits in its section file.
  std::unique_ptr<Page> decode(const std::vector<uint8_t>& bytes, size_t* end = nullptr, const bool tail = true) const {
    {
      std::ofstream out(path, std::ios::binary | std::ios::trunc);
      out.write("prefix", 6);
      out.write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
      if (tail) out.write("tail", 4);
    }
    HalFile f;
    EXPECT_TRUE(f.open(path.c_str(), "rb"));
    f.seek(6);
    auto page = Page::deserialize(f);
    if (end) *end = f.position() - 6;
    return page;
  }
};

TEST_F(PageCodecTest, APageIsWrittenInTheFormat) {
  // Unused bytes of the fixed-size strings are zero, so a section file is the same every build.
  EXPECT_EQ(serialized(*samplePage()), samplePageBytes().b);
}

TEST_F(PageCodecTest, APageReadsBackAsWritten) {
  const auto bytes = samplePageBytes().b;
  size_t end = 0;
  const auto page = decode(bytes, &end);
  ASSERT_TRUE(page);
  EXPECT_EQ(end, bytes.size());  // the file is left just past the page
  ASSERT_EQ(page->elements.size(), 4u);
  EXPECT_EQ(serialized(*page), bytes);

  ASSERT_EQ(page->elements[0]->getTag(), TAG_PageLine);
  const auto& line = static_cast<const PageLine&>(*page->elements[0]);
  EXPECT_EQ(line.xPos, 5);
  EXPECT_EQ(line.yPos, 7);
  const TextBlock& block = *line.getBlock();
  ASSERT_EQ(block.wordCount(), 3);
  EXPECT_STREQ(block.wordText(0), "Ça");
  EXPECT_EQ(block.wordTextLen(0), 3);
  EXPECT_STREQ(block.wordText(2), "?");
  EXPECT_EQ(block.wordXpos(1), 20);
  EXPECT_EQ(block.wordStyle(1), EpdFontFamily::ITALIC);
  EXPECT_TRUE(block.wordHasDiscretionaryHyphen(1));
  EXPECT_EQ(block.wordStyle(2), EpdFontFamily::SMALL_CAPS);
  EXPECT_EQ(block.focusBoundary(0), 1);
  EXPECT_EQ(block.focusSuffixX(0), 9);
  EXPECT_EQ(block.wordSourceRange(1).start, 13u);
  EXPECT_EQ(block.wordSourceRange(1).end, 15u);
  EXPECT_TRUE(block.wordStartsParagraph(0));
  ASSERT_EQ(block.getRubyTexts().size(), 3u);
  EXPECT_EQ(block.getRubyTexts()[1], "かな");
  EXPECT_EQ(block.wordFontId(0, 1), 77);
  EXPECT_EQ(block.wordFontId(1, 1), 31);
  EXPECT_EQ(block.wordFontId(2, 1), 32);
  EXPECT_EQ(block.getBlockStyle().marginTop, -3);
  EXPECT_EQ(block.getBlockStyle().alignment, CssTextAlign::Center);
  EXPECT_TRUE(block.getBlockStyle().isRtl);
  EXPECT_EQ(block.getBlockStyle().characterSpacing, -2);
  EXPECT_EQ(block.blockFontId(1), 77);

  EXPECT_EQ(page->elements[1]->getTag(), TAG_PageHorizontalRule);
  EXPECT_EQ(page->elements[1]->xPos, -1);
  ASSERT_EQ(page->elements[2]->getTag(), TAG_PageDropCap);
  EXPECT_STREQ(static_cast<const PageDropCap&>(*page->elements[2]).getText(),
               "\xC2\xAB"
               "A");
  EXPECT_EQ(page->elements[3]->getTag(), TAG_PageBorderBox);
  EXPECT_EQ(page->elements[3]->yPos, 80);

  ASSERT_EQ(page->footnotes.size(), 2u);
  EXPECT_STREQ(page->footnotes[1].number, "22");
  EXPECT_STREQ(page->footnotes[1].href, "notes.xhtml#n22");
  ASSERT_EQ(page->links.size(), 2u);
  EXPECT_STREQ(page->links[1].href, "#top");
  EXPECT_EQ(page->links[1].x, -4);
  EXPECT_EQ(page->links[1].height, 1);
}

TEST_F(PageCodecTest, PlainAndEmptyLinesRoundTrip) {
  Line plain;
  plain.words = {{"plain", 0, EpdFontFamily::REGULAR, UINT32_MAX, UINT32_MAX}};
  Line empty;
  auto page = std::make_unique<Page>();
  page->elements.push_back(std::make_unique<PageLine>(makeBlock(plain), 0, 0));
  page->elements.push_back(std::make_unique<PageLine>(makeBlock(empty), 0, 16));
  Bytes expected;
  expected.pod<uint16_t>(2).pod<uint8_t>(TAG_PageLine).pod<int16_t>(0).pod<int16_t>(0);
  encodeLine(expected, plain);
  expected.pod<uint8_t>(TAG_PageLine).pod<int16_t>(0).pod<int16_t>(16);
  encodeLine(expected, empty);
  expected.pod<uint16_t>(0).pod<uint16_t>(0);
  EXPECT_EQ(serialized(*page), expected.b);

  const auto back = decode(expected.b);
  ASSERT_TRUE(back);
  const TextBlock& block = *static_cast<const PageLine&>(*back->elements[0]).getBlock();
  EXPECT_EQ(block.focusBoundary(0), 0);
  EXPECT_TRUE(block.getRubyTexts().empty());  // no ruby: nothing kept
  EXPECT_FALSE(block.wordStartsParagraph(0));
  EXPECT_EQ(block.wordFontId(0, 4), 4);
  EXPECT_EQ(static_cast<const PageLine&>(*back->elements[1]).getBlock()->wordCount(), 0);
  EXPECT_EQ(serialized(*back), expected.b);
}

TEST_F(PageCodecTest, AnEmptyPageAndOddButValidFieldsReadBack) {
  Bytes empty;
  empty.pod<uint16_t>(0).pod<uint16_t>(0).pod<uint16_t>(0);
  const auto page = decode(empty.b);
  ASSERT_TRUE(page);
  EXPECT_TRUE(page->elements.empty());

  // Unterminated footnote and link strings end at their last byte; an alignment past the named ones is kept.
  Bytes odd;
  odd.pod<uint16_t>(1).pod<uint8_t>(TAG_PageLine).pod<int16_t>(0).pod<int16_t>(0);
  Line line;
  line.words = {{"x", 0, 0, 0, 1}};
  line.style.alignment = static_cast<CssTextAlign>(9);
  encodeLine(odd, line);
  odd.pod<uint16_t>(1).fixed(std::string(40, 'n'), 32).fixed(std::string(300, 'h'), 256);
  odd.pod<uint16_t>(1)
      .fixed(std::string(300, 'l'), 256)
      .pod<int16_t>(0)
      .pod<int16_t>(0)
      .pod<int16_t>(1)
      .pod<int16_t>(1);
  const auto back = decode(odd.b);
  ASSERT_TRUE(back);
  EXPECT_EQ(std::string(back->footnotes[0].number), std::string(31, 'n'));
  EXPECT_EQ(std::string(back->footnotes[0].href), std::string(255, 'h'));
  EXPECT_EQ(std::string(back->links[0].href), std::string(255, 'l'));
  EXPECT_EQ(static_cast<const PageLine&>(*back->elements[0]).getBlock()->getBlockStyle().alignment,
            static_cast<CssTextAlign>(9));
}

// Each damage, at a known place, refuses the page.
TEST_F(PageCodecTest, DamagedPagesAreRefused) {
  const auto good = samplePageBytes().b;
  // Where the sample's parts start.
  const size_t line = 2 + 1 + 4;  // the line's text block
  Bytes lineOnly;
  encodeLine(lineOnly, sampleLine());
  const size_t rule = line + lineOnly.b.size();
  const size_t dropCap = rule + 1 + 4 + 2 + 1;
  const size_t box = dropCap + 1 + 4 + 4 + 2 + 1 + 1 + 3;
  const size_t notes = box + 1 + 4 + 2 + 2 + 8 + 1;
  const size_t links = notes + 2 + 2 * 288;
  const size_t arena = line + 7;
  const size_t offsets = arena + 3 * 8;
  const size_t text = offsets + 3 * 2 + 3 * 2 + 3 * 2 + 3 + 3;         // after offsets, x, suffix x, styles, boundaries
  const size_t afterArena = text + 4 + 3 + 2;                          // "Ça", "va", "?" with NULs
  const size_t style = afterArena + 4 + (4 + 6) + 4;                   // the ruby strings
  const size_t fonts = style + 1 + 1 + 8 * 2 + 2 + 1 + 1 + 1 + 1 + 4;  // the block style

  const std::vector<std::pair<const char*, std::function<void(std::vector<uint8_t>&)>>> damages = {
      {"unknown tag", [&](auto& b) { b[rule] = 6; }},
      {"zero tag", [&](auto& b) { b[rule] = 0; }},
      {"rule without width", [&](auto& b) { put<uint16_t>(b, rule + 5, 0); }},
      {"rule without thickness", [&](auto& b) { b[rule + 7] = 0; }},
      {"drop cap without text", [&](auto& b) { b[dropCap + 12] = 0; }},
      {"drop cap too long", [&](auto& b) { b[dropCap + 12] = 13; }},
      {"drop cap without scale", [&](auto& b) { put<uint16_t>(b, dropCap + 9, 0); }},
      {"border style past dashed", [&](auto& b) { b[box + 10] = 5; }},
      {"border box without width", [&](auto& b) { put<uint16_t>(b, box + 5, 0); }},
      {"border box without height", [&](auto& b) { put<uint16_t>(b, box + 7, 0); }},
      {"17 footnotes", [&](auto& b) { put<uint16_t>(b, notes, 17); }},
      {"33 links", [&](auto& b) { put<uint16_t>(b, links, 33); }},
      {"link without href", [&](auto& b) { b[links + 2] = 0; }},
      {"link without width", [&](auto& b) { put<int16_t>(b, links + 2 + 256 + 4, 0); }},
      {"link with negative height", [&](auto& b) { put<int16_t>(b, links + 2 + 256 + 6, -1); }},
      {"10001 words", [&](auto& b) { put<uint16_t>(b, line, 10001); }},
      {"paragraph start past the words", [&](auto& b) { put<uint16_t>(b, line + 5, 3); }},
      {"text shorter than one byte a word", [&](auto& b) { put<uint16_t>(b, line + 3, 2); }},
      {"first offset not zero", [&](auto& b) { put<uint16_t>(b, offsets, 1); }},
      {"offsets not rising", [&](auto& b) { put<uint16_t>(b, offsets + 4, 4); }},
      {"offset past the text", [&](auto& b) { put<uint16_t>(b, offsets + 4, 9); }},
      {"word not terminated", [&](auto& b) { b[text + 3] = 'x'; }},
      {"last word not terminated", [&](auto& b) { b[text + 8] = 'x'; }},
      {"word font flag 2", [&](auto& b) { b[fonts] = 2; }},
      {"word font slot 4", [&](auto& b) { b[fonts + 1 + 12 + 1] = 4; }},
      {"word font slot naming font 0", [&](auto& b) { b[fonts + 1 + 12 + 2] = 3; }},
      {"zero words with text",
       [&](auto& b) {
         Bytes zero;
         zero.pod<uint16_t>(1).pod<uint8_t>(TAG_PageLine).pod<int32_t>(0);
         zero.pod<uint16_t>(0).pod<uint8_t>(0).pod<uint16_t>(1).pod<uint16_t>(UINT16_MAX);
         b = zero.b;
       }},
  };
  for (const auto& [name, damage] : damages) {
    auto bytes = good;
    damage(bytes);
    EXPECT_EQ(decode(bytes), nullptr) << name;
  }
  // Cut short at the file's end, a field the file ends in reads as zero: the page is refused, except where
  // zero is valid, as for the element count, a border box's sides and shading, and the footnote and link
  // counts.
  for (size_t cut = 0; cut < good.size(); ++cut) {
    const bool valid = cut < 2 || (cut >= box + 9 && cut <= notes + 1) || (cut >= links && cut <= links + 1);
    const auto page = decode(std::vector<uint8_t>(good.begin(), good.begin() + cut), nullptr, false);
    EXPECT_EQ(page != nullptr, valid) << cut;
  }
}

TEST_F(PageCodecTest, AnImageRoundTripsWithItsPathAndSource) {
  auto page = std::make_unique<Page>();
  page->elements.push_back(std::make_unique<PageImage>(
      std::make_unique<ImageBlock>("/cache/img_3_1.pxc", "OEBPS/images/a.png", 320, -2), 4, 6));
  Bytes expected;
  expected.pod<uint16_t>(1).pod<uint8_t>(TAG_PageImage).pod<int16_t>(4).pod<int16_t>(6);
  expected.str("/cache/img_3_1.pxc").str("OEBPS/images/a.png").pod<int16_t>(320).pod<int16_t>(-2);
  expected.pod<uint16_t>(0).pod<uint16_t>(0);
  EXPECT_EQ(serialized(*page), expected.b);

  const auto back = decode(expected.b);
  ASSERT_TRUE(back);
  ASSERT_EQ(back->elements[0]->getTag(), TAG_PageImage);
  const ImageBlock& image = static_cast<const PageImage&>(*back->elements[0]).getImageBlock();
  EXPECT_EQ(image.getImagePath(), "/cache/img_3_1.pxc");
  EXPECT_EQ(image.getSrcPath(), "OEBPS/images/a.png");
  EXPECT_EQ(image.getWidth(), 320);
  EXPECT_EQ(image.getHeight(), -2);
  EXPECT_EQ(serialized(*back), expected.b);
}

// An image's strings are not checked: one the file cannot hold reads as empty, and so does the rest.
TEST_F(PageCodecTest, AnImageWithItsStringsCutReadsEmpty) {
  Bytes bytes;
  bytes.pod<uint16_t>(1).pod<uint8_t>(TAG_PageImage).pod<int16_t>(4).pod<int16_t>(6).pod<uint32_t>(1000);
  const auto page = decode(bytes.b, nullptr, false);
  ASSERT_TRUE(page);
  const ImageBlock& image = static_cast<const PageImage&>(*page->elements[0]).getImageBlock();
  EXPECT_EQ(image.getImagePath(), "");
  EXPECT_EQ(image.getSrcPath(), "");
  EXPECT_EQ(image.getWidth(), 0);
}

// Every word font slot names its font; slot 0 is the line's.
TEST_F(PageCodecTest, AllThreeWordFontSlotsRead) {
  Line line;
  line.words = {{"a", 0, 0, 0, 1, 0, 0, "", 0},
                {"b", 8, 0, 1, 2, 0, 0, "", 1},
                {"c", 16, 0, 2, 3, 0, 0, "", 2},
                {"d", 24, 0, 3, 4, 0, 0, "", 3}};
  line.fonts = true;
  line.fontTable[0] = 41;
  line.fontTable[1] = 42;
  line.fontTable[2] = 43;
  Bytes bytes;
  bytes.pod<uint16_t>(1).pod<uint8_t>(TAG_PageLine).pod<int16_t>(0).pod<int16_t>(0);
  encodeLine(bytes, line);
  bytes.pod<uint16_t>(0).pod<uint16_t>(0);
  const auto page = decode(bytes.b);
  ASSERT_TRUE(page);
  const TextBlock& block = *static_cast<const PageLine&>(*page->elements[0]).getBlock();
  EXPECT_EQ(block.wordFontId(0, 5), 5);
  EXPECT_EQ(block.wordFontId(1, 5), 41);
  EXPECT_EQ(block.wordFontId(2, 5), 42);
  EXPECT_EQ(block.wordFontId(3, 5), 43);
}

// A stored flag byte other than 0 or 1 reads as set.
TEST_F(PageCodecTest, AStyleFlagByteOtherThanZeroOrOneIsSet) {
  Line line;
  line.words = {{"x", 0, 0, 0, 1}};
  Bytes bytes;
  bytes.pod<uint16_t>(1).pod<uint8_t>(TAG_PageLine).pod<int16_t>(0).pod<int16_t>(0);
  const size_t lineAt = bytes.b.size();
  encodeLine(bytes, line);
  bytes.pod<uint16_t>(0).pod<uint16_t>(0);
  // The style after the counts (7), the arena (8 + 2 + 2 + 1 + 2) and one empty ruby (4).
  const size_t style = lineAt + 7 + 15 + 4;
  bytes.b[style + 1] = 2;   // text-align defined
  bytes.b[style + 21] = 7;  // is RTL
  const auto page = decode(bytes.b);
  ASSERT_TRUE(page);
  const BlockStyle& read = static_cast<const PageLine&>(*page->elements[0]).getBlock()->getBlockStyle();
  EXPECT_TRUE(read.textAlignDefined);
  EXPECT_TRUE(read.isRtl);
  EXPECT_FALSE(read.directionDefined);
}

// Text blocks read alone, one after another, each leaving the file just past it.
TEST_F(PageCodecTest, TextBlocksReadOneAfterAnother) {
  Line first = sampleLine();
  Line second;
  second.words = {{"next", 3, 0, 0, 4}};
  Bytes bytes;
  encodeLine(bytes, first);
  const size_t firstEnd = bytes.b.size();
  encodeLine(bytes, second);
  {
    std::ofstream out(path, std::ios::binary | std::ios::trunc);
    out.write(reinterpret_cast<const char*>(bytes.b.data()), static_cast<std::streamsize>(bytes.b.size()));
  }
  HalFile f;
  ASSERT_TRUE(f.open(path.c_str(), "rb"));
  const auto a = TextBlock::deserialize(f);
  ASSERT_TRUE(a);
  EXPECT_EQ(f.position(), firstEnd);
  const auto b = TextBlock::deserialize(f);
  ASSERT_TRUE(b);
  EXPECT_STREQ(b->wordText(0), "next");
  EXPECT_EQ(f.position(), bytes.b.size());
  EXPECT_EQ(TextBlock::deserialize(f), nullptr);
  f.close();

  // And written alone, as the format has them.
  ASSERT_TRUE(f.open(path.c_str(), "w+b"));
  ASSERT_TRUE(makeBlock(first)->serialize(f));
  ASSERT_TRUE(makeBlock(second)->serialize(f));
  f.close();
  EXPECT_EQ(readAll(), bytes.b);
}

TEST_F(PageCodecTest, AnyDamageNeverCrashes) {
  const auto good = samplePageBytes().b;
  uint32_t seed = 0x9a6ec0de;
  const auto next = [&seed]() {
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    return seed;
  };
  for (int round = 0; round < 2000; ++round) {
    auto bytes = good;
    for (uint32_t n = next() % 6 + 1; n > 0; --n) bytes[next() % bytes.size()] = static_cast<uint8_t>(next());
    if (next() % 4 == 0) bytes.resize(next() % bytes.size());
    if (const auto page = decode(bytes)) (void)serialized(*page);
  }
}

}  // namespace
