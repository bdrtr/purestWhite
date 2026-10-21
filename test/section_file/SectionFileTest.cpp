#include <Epub.h>
#include <Epub/Page.h>
#include <Epub/Section.h>
#include <GfxRenderer.h>
#include <gtest/gtest.h>
#include <unistd.h>

#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iterator>
#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace fs = std::filesystem;

namespace {

// The section file as written today, decoded independently of Section: a packed 44-byte header, the pages,
// then the page LUT, the anchor map, the paragraph LUT (count first), the list-item LUT, the visible-offset
// LUT and, for a partial, the parse watermark.
constexpr uint8_t VERSION = 60;
constexpr uint8_t PARTIAL_VERSION = 0xFE - (VERSION - 28);
constexpr size_t HEADER_SIZE = 44;
constexpr size_t PAGE_COUNT_AT = 22;
constexpr size_t LUT_AT = 24;
constexpr size_t ANCHORS_AT = 28;
constexpr size_t PARAGRAPHS_AT = 32;
constexpr size_t LIST_ITEMS_AT = 36;
constexpr size_t VISIBLE_AT = 40;

template <typename T>
T get(const std::vector<uint8_t>& bytes, const size_t at) {
  T value{};
  std::memcpy(&value, bytes.data() + at, sizeof(T));
  return value;
}

template <typename T>
void put(std::vector<uint8_t>& bytes, const size_t at, const T value) {
  std::memcpy(bytes.data() + at, &value, sizeof(T));
}

struct Decoded {
  uint8_t version = 0;
  uint16_t pageCount = 0;
  std::vector<uint32_t> pages;
  std::vector<std::pair<std::string, uint16_t>> anchors;
  std::vector<uint16_t> paragraphs;
  std::vector<uint16_t> listItems;
  std::vector<uint32_t> visible;
  uint32_t consumed = 0;
  uint32_t total = 0;
};

Decoded decode(const std::vector<uint8_t>& bytes) {
  Decoded d;
  d.version = bytes.at(0);
  d.pageCount = get<uint16_t>(bytes, PAGE_COUNT_AT);
  const auto lut = get<uint32_t>(bytes, LUT_AT);
  const auto anchors = get<uint32_t>(bytes, ANCHORS_AT);
  const auto paragraphs = get<uint32_t>(bytes, PARAGRAPHS_AT);
  const auto listItems = get<uint32_t>(bytes, LIST_ITEMS_AT);
  const auto visible = get<uint32_t>(bytes, VISIBLE_AT);
  EXPECT_EQ(anchors, lut + 4 * d.pageCount);
  for (uint16_t i = 0; i < d.pageCount; ++i) d.pages.push_back(get<uint32_t>(bytes, lut + 4 * i));
  size_t at = anchors;
  const auto anchorCount = get<uint16_t>(bytes, at);
  at += 2;
  for (uint16_t i = 0; i < anchorCount; ++i) {
    const auto length = get<uint32_t>(bytes, at);
    at += 4;
    std::string name(reinterpret_cast<const char*>(bytes.data() + at), length);
    at += length;
    d.anchors.emplace_back(name, get<uint16_t>(bytes, at));
    at += 2;
  }
  EXPECT_EQ(paragraphs, at);
  EXPECT_EQ(get<uint16_t>(bytes, paragraphs), d.pageCount);
  for (uint16_t i = 0; i < d.pageCount; ++i) d.paragraphs.push_back(get<uint16_t>(bytes, paragraphs + 2 + 2 * i));
  EXPECT_EQ(listItems, paragraphs + 2 + 2 * d.pageCount);
  for (uint16_t i = 0; i < d.pageCount; ++i) d.listItems.push_back(get<uint16_t>(bytes, listItems + 2 * i));
  EXPECT_EQ(visible, listItems + 2 * d.pageCount);
  for (uint16_t i = 0; i < d.pageCount; ++i) d.visible.push_back(get<uint32_t>(bytes, visible + 4 * i));
  const size_t end = visible + 4 * d.pageCount;
  if (d.version == PARTIAL_VERSION) {
    d.consumed = get<uint32_t>(bytes, end);
    d.total = get<uint32_t>(bytes, end + 4);
    EXPECT_EQ(bytes.size(), end + 8);
  } else {
    EXPECT_EQ(bytes.size(), end);
  }
  return d;
}

std::string pageText(const Page& page) {
  std::string text;
  for (const auto& element : page.elements) {
    if (element->getTag() != TAG_PageLine) continue;
    const auto* block = static_cast<const PageLine&>(*element).getBlock();
    for (uint16_t i = 0; i < block->wordCount(); ++i) {
      if (!text.empty()) text += ' ';
      text += block->wordText(i);
    }
  }
  return text;
}

// Paragraph n holds the words "n.0" .. "n.<words-1>", and an id "p<n>" when anchored.
std::string paragraphs(const int count, const int words, const bool anchored = true) {
  std::string html = "<html><body>";
  for (int n = 0; n < count; ++n) {
    html += anchored ? "<p id=\"p" + std::to_string(n) + "\">" : "<p>";
    for (int w = 0; w < words; ++w) html += (w ? " " : "") + std::to_string(n) + "." + std::to_string(w);
    html += "</p>";
  }
  return html + "</body></html>";
}

class SectionFileTest : public ::testing::Test {
 protected:
  fs::path dir;
  std::shared_ptr<Epub> epub = std::make_shared<Epub>();
  GfxRenderer renderer;

  void SetUp() override {
    dir = fs::temp_directory_path() /
          ("section_file_" + std::to_string(::getpid()) + "_" +
           ::testing::UnitTest::GetInstance()->current_test_info()->name());
    fs::remove_all(dir);
    fs::create_directories(dir);
    epub->cachePath = dir.string();
    epub->spine = {"OEBPS/c0.xhtml"};
  }
  void TearDown() override { fs::remove_all(dir); }

  static ReaderRenderSpec spec() {
    ReaderRenderSpec s;
    s.fontId = 7;
    s.viewportWidth = 200;
    s.viewportHeight = 120;
    s.embeddedStyle = false;
    return s;
  }

  std::string binPath() const { return (dir / "sections" / "0.bin").string(); }

  std::vector<uint8_t> readBin() const {
    std::ifstream in(binPath(), std::ios::binary);
    return {std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>()};
  }
  void writeBin(const std::vector<uint8_t>& bytes) const {
    std::ofstream out(binPath(), std::ios::binary | std::ios::trunc);
    out.write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
  }

  // Builds the section from html in one go and returns the page texts the build reported.
  std::vector<std::string> build(const std::string& html, const ReaderRenderSpec& s = spec()) {
    epub->contents[epub->spine[0]] = html;
    Section section(epub, 0, renderer);
    EXPECT_TRUE(section.createSectionFile(s));
    std::vector<std::string> texts;
    for (int i = 0; i < section.pageCount; ++i) {
      const auto page = section.loadPage(i);
      texts.push_back(page ? pageText(*page) : "<missing>");
    }
    return texts;
  }

  std::unique_ptr<Section> loaded(const ReaderRenderSpec& s = spec()) {
    auto section = std::make_unique<Section>(epub, 0, renderer);
    if (!section->loadSectionFile(s)) return nullptr;
    return section;
  }
};

TEST_F(SectionFileTest, ABuiltFileHasTheFormatAndReloadsEveryPage) {
  const auto texts = build(paragraphs(24, 10));
  ASSERT_GT(texts.size(), 4u);

  const auto bytes = readBin();
  const Decoded d = decode(bytes);
  EXPECT_EQ(d.version, VERSION);
  EXPECT_EQ(d.pageCount, texts.size());
  EXPECT_EQ(get<int>(bytes, 1), 7);  // font id
  EXPECT_EQ(get<uint16_t>(bytes, 12), 200);
  EXPECT_EQ(get<uint16_t>(bytes, 14), 120);
  EXPECT_EQ(d.pages.front(), HEADER_SIZE);
  for (size_t i = 1; i < d.pages.size(); ++i) EXPECT_GT(d.pages[i], d.pages[i - 1]);
  EXPECT_EQ(d.visible.front(), 0u);

  const auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_FALSE(section->isPartial());
  EXPECT_EQ(section->pageCount, texts.size());
  EXPECT_EQ(section->estimatedTotalPages(), texts.size());
  EXPECT_EQ(section->getCachedPageCount(), texts.size());
  for (size_t i = 0; i < texts.size(); ++i) {
    const auto page = section->loadPage(static_cast<int>(i));
    ASSERT_TRUE(page) << i;
    EXPECT_EQ(pageText(*page), texts[i]);
    EXPECT_EQ(page->visibleTextOffset, d.visible[i]);
  }
  EXPECT_EQ(section->loadPage(static_cast<int>(texts.size())), nullptr);
  EXPECT_EQ(section->loadPage(-1), nullptr);
}

TEST_F(SectionFileTest, AnchorsParagraphsAndListItemsLookUpTheTables) {
  std::string html = "<html><body>";
  for (int n = 0; n < 6; ++n) {
    html += "<p id=\"p" + std::to_string(n) + "\">";
    for (int w = 0; w < 10; ++w) html += (w ? " " : "") + std::to_string(n) + "." + std::to_string(w);
    html += "</p><ol>";
    for (int li = 0; li < 3; ++li) html += "<li id=\"l" + std::to_string(n) + "_" + std::to_string(li) + "\">item</li>";
    html += "</ol>";
  }
  build(html + "</body></html>");
  const Decoded d = decode(readBin());
  ASSERT_FALSE(d.anchors.empty());

  const auto section = loaded();
  ASSERT_TRUE(section);
  for (const auto& [name, page] : d.anchors) {
    EXPECT_EQ(section->getPageForAnchor(name), page) << name;
    EXPECT_EQ(section->findAnchor(name), page) << name;
  }
  for (int n = 0; n < 6; ++n) EXPECT_TRUE(section->getPageForAnchor("p" + std::to_string(n))) << n;
  EXPECT_EQ(section->getPageForAnchor("missing"), std::nullopt);

  const uint16_t lastPage = d.pageCount - 1;
  for (uint16_t page = 0; page < d.pageCount; ++page) {
    EXPECT_EQ(section->getParagraphIndexForPage(page), d.paragraphs[page]);
  }
  EXPECT_EQ(section->getParagraphIndexForPage(d.pageCount), std::nullopt);
  // First page whose index reaches the target, else the last page.
  const auto firstReaching = [&](const std::vector<uint16_t>& lut, const uint16_t target) {
    for (uint16_t page = 0; page < lut.size(); ++page) {
      if (lut[page] >= target) return page;
    }
    return lastPage;
  };
  for (uint16_t target = 0; target < d.paragraphs.back() + 3; ++target) {
    EXPECT_EQ(section->getPageForParagraphIndex(target), firstReaching(d.paragraphs, target)) << target;
  }
  ASSERT_GT(d.listItems.back(), 0u);
  for (uint16_t target = 0; target < d.listItems.back() + 3; ++target) {
    EXPECT_EQ(section->getPageForListItemIndex(target), firstReaching(d.listItems, target)) << target;
  }
}

TEST_F(SectionFileTest, VisibleOffsetsMapToPagesBothWays) {
  build(paragraphs(12, 10));
  const Decoded d = decode(readBin());
  const auto section = loaded();
  ASSERT_TRUE(section);
  for (uint16_t page = 0; page < d.pageCount; ++page) {
    EXPECT_EQ(section->getVisibleTextOffsetForPage(page), d.visible[page]);
    EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible[page]), page);
    EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible[page], true), page);
    if (page > 0) EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible[page] - 1), page - 1);
  }
  EXPECT_EQ(section->getVisibleTextOffsetForPage(d.pageCount), std::nullopt);
  EXPECT_EQ(section->getPageForVisibleTextOffset(UINT32_MAX), d.pageCount - 1);
}

TEST_F(SectionFileTest, TiedVisibleOffsetsPickTheLastOrTheFirstPage) {
  build(paragraphs(12, 10));
  auto bytes = readBin();
  const Decoded d = decode(bytes);
  ASSERT_GE(d.pageCount, 4);
  // Pages 1 and 2 start at the same offset, as an image-only page does.
  const auto visible = get<uint32_t>(bytes, VISIBLE_AT);
  put<uint32_t>(bytes, visible + 8, d.visible[1]);
  writeBin(bytes);
  const auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible[1]), 2);
  EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible[1], true), 1);
}

TEST_F(SectionFileTest, AnotherSpecOrVersionDropsTheFile) {
  build(paragraphs(4, 10));
  const auto good = readBin();
  const std::vector<std::function<void(ReaderRenderSpec&)>> changes = {
      [](ReaderRenderSpec& s) { s.fontId++; },
      [](ReaderRenderSpec& s) { s.lineCompression = 0.9f; },
      [](ReaderRenderSpec& s) { s.extraParagraphSpacing = true; },
      [](ReaderRenderSpec& s) { s.paragraphIndentSpaces++; },
      [](ReaderRenderSpec& s) { s.paragraphAlignment++; },
      [](ReaderRenderSpec& s) { s.viewportWidth++; },
      [](ReaderRenderSpec& s) { s.viewportHeight++; },
      [](ReaderRenderSpec& s) { s.hyphenationEnabled = true; },
      [](ReaderRenderSpec& s) { s.embeddedStyle = true; },
      [](ReaderRenderSpec& s) { s.imageRendering++; },
      [](ReaderRenderSpec& s) { s.focusReadingEnabled = true; },
      [](ReaderRenderSpec& s) { s.characterSpacing++; },
      [](ReaderRenderSpec& s) { s.wordSpacingPercent++; },
  };
  for (size_t i = 0; i < changes.size(); ++i) {
    writeBin(good);
    ReaderRenderSpec s = spec();
    changes[i](s);
    EXPECT_EQ(loaded(s), nullptr) << i;
    EXPECT_FALSE(fs::exists(binPath())) << i;
  }
  for (const uint8_t version : {uint8_t{0}, uint8_t{VERSION - 1}, uint8_t{PARTIAL_VERSION + 1}}) {
    auto bytes = good;
    bytes[0] = version;
    writeBin(bytes);
    EXPECT_EQ(loaded(), nullptr) << int(version);
    EXPECT_FALSE(fs::exists(binPath())) << int(version);
  }
  writeBin(good);
  EXPECT_TRUE(loaded());
}

TEST_F(SectionFileTest, ASuspendedBuildReloadsAsAPartial) {
  epub->contents[epub->spine[0]] = paragraphs(24, 10);
  uint16_t built = 0;
  {
    Section section(epub, 0, renderer);
    ASSERT_TRUE(section.startBuild(spec()));
    ASSERT_TRUE(section.buildSomeMore(3));
    EXPECT_FALSE(section.isBuildComplete());
    built = section.pageCount;
    EXPECT_GE(section.estimatedTotalPages(), built);
  }  // the destructor suspends the build
  const Decoded d = decode(readBin());
  EXPECT_EQ(d.version, PARTIAL_VERSION);
  EXPECT_EQ(d.pageCount, built);
  EXPECT_GT(d.consumed, 0u);
  EXPECT_GT(d.total, d.consumed);
  for (const auto& [name, page] : d.anchors) EXPECT_LT(page, built) << name;

  const auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_TRUE(section->isPartial());
  EXPECT_EQ(section->pageCount, built);
  EXPECT_EQ(section->getCachedPageCount(), std::nullopt);
  const uint64_t estimate = uint64_t{built} * d.total / d.consumed;
  EXPECT_EQ(section->estimatedTotalPages(), estimate > built ? estimate : built);
  for (uint16_t page = 0; page < built; ++page) EXPECT_TRUE(section->loadPage(page)) << page;
  EXPECT_EQ(section->loadPage(built), nullptr);
  // Past the last page's start a partial does not know where an offset falls.
  EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible.back()), built - 1);
  EXPECT_EQ(section->getPageForVisibleTextOffset(d.visible.back() + 1), std::nullopt);
  EXPECT_EQ(section->getVisibleTextOffsetForPage(built - 1), d.visible.back());
}

TEST_F(SectionFileTest, ARebuildOverAPartialFinishesTheFile) {
  epub->contents[epub->spine[0]] = paragraphs(24, 10);
  {
    Section section(epub, 0, renderer);
    ASSERT_TRUE(section.startBuild(spec()));
    ASSERT_TRUE(section.buildSomeMore(2));
  }
  const uint16_t partial = decode(readBin()).pageCount;
  Section section(epub, 0, renderer);
  ASSERT_TRUE(section.loadSectionFile(spec()));
  ASSERT_TRUE(section.startBuild(spec()));
  // The partial's pages stay readable before the rebuild reaches them.
  EXPECT_EQ(section.pageCount, partial);
  EXPECT_TRUE(section.loadPage(partial - 1));
  ASSERT_TRUE(section.buildSomeMore(0));
  EXPECT_TRUE(section.isBuildComplete());
  EXPECT_FALSE(section.isPartial());
  const Decoded d = decode(readBin());
  EXPECT_EQ(d.version, VERSION);
  EXPECT_GT(d.pageCount, partial);
  EXPECT_EQ(section.pageCount, d.pageCount);
}

TEST_F(SectionFileTest, PagesAndTablesAreReadableDuringTheBuild) {
  epub->contents[epub->spine[0]] = paragraphs(24, 10);
  Section section(epub, 0, renderer);
  ASSERT_TRUE(section.startBuild(spec()));
  ASSERT_TRUE(section.buildSomeMore(3));
  std::vector<std::string> during;
  std::vector<uint32_t> offsets;
  for (int page = 0; page < section.pageCount; ++page) {
    const auto loadedPage = section.loadPage(page);
    ASSERT_TRUE(loadedPage) << page;
    during.push_back(pageText(*loadedPage));
    offsets.push_back(loadedPage->visibleTextOffset);
    EXPECT_EQ(section.getVisibleTextOffsetForPage(page), offsets.back());
  }
  EXPECT_TRUE(section.buildReachedVisibleTextOffset(offsets.back()));
  EXPECT_EQ(section.getPageForVisibleTextOffset(offsets[1]), 1);
  EXPECT_EQ(section.findAnchorDuringBuild("p0"), 0);
  EXPECT_EQ(section.findAnchor("p0"), 0);
  ASSERT_TRUE(section.buildSomeMore(0));
  for (size_t page = 0; page < during.size(); ++page) {
    EXPECT_EQ(pageText(*section.loadPage(static_cast<int>(page))), during[page]);
  }
}

TEST_F(SectionFileTest, TocBoundariesFollowTheAnchors) {
  const auto texts = build(paragraphs(12, 10));
  const Decoded d = decode(readBin());
  epub->toc = {{"", 0}, {"p3", 0}, {"p8", 0}, {"elsewhere", 1}};
  const auto section = loaded();
  ASSERT_TRUE(section);
  const auto p3 = *section->getPageForAnchor("p3");
  const auto p8 = *section->getPageForAnchor("p8");
  ASSERT_LT(p3, p8);
  for (int page = 0; page < d.pageCount; ++page) {
    const int expected = page >= p8 ? 2 : page >= p3 ? 1 : 0;
    EXPECT_EQ(section->getTocIndexForPage(page), expected) << page;
  }
}

// Damaged tables: what each lookup gives today.
TEST_F(SectionFileTest, TablesPastTheEndAreRefused) {
  build(paragraphs(12, 10));
  const auto good = readBin();
  const Decoded d = decode(good);
  const auto size = static_cast<uint32_t>(good.size());

  auto bytes = good;
  put<uint32_t>(bytes, ANCHORS_AT, size);
  put<uint32_t>(bytes, PARAGRAPHS_AT, size);
  put<uint32_t>(bytes, LIST_ITEMS_AT, size);
  put<uint32_t>(bytes, VISIBLE_AT, size);
  writeBin(bytes);
  auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->getPageForAnchor("p0"), std::nullopt);
  EXPECT_EQ(section->getPageForParagraphIndex(0), std::nullopt);
  EXPECT_EQ(section->getParagraphIndexForPage(0), std::nullopt);
  EXPECT_EQ(section->getPageForListItemIndex(0), std::nullopt);
  EXPECT_EQ(section->getVisibleTextOffsetForPage(0), std::nullopt);
  EXPECT_EQ(section->getPageForVisibleTextOffset(0), std::nullopt);
  // The page itself still loads, with no visible offset.
  const auto page = section->loadPage(1);
  ASSERT_TRUE(page);
  EXPECT_EQ(page->visibleTextOffset, 0u);

  // A visible LUT inside the header, or zero offsets, read as absent.
  bytes = good;
  put<uint32_t>(bytes, VISIBLE_AT, HEADER_SIZE - 4);
  put<uint32_t>(bytes, PARAGRAPHS_AT, 0);
  put<uint32_t>(bytes, LIST_ITEMS_AT, 0);
  put<uint32_t>(bytes, ANCHORS_AT, 0);
  writeBin(bytes);
  section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->getVisibleTextOffsetForPage(1), std::nullopt);
  EXPECT_EQ(section->getPageForVisibleTextOffset(0), std::nullopt);
  EXPECT_EQ(section->getPageForParagraphIndex(0), std::nullopt);
  EXPECT_EQ(section->getPageForListItemIndex(0), std::nullopt);
  EXPECT_EQ(section->getPageForAnchor("p0"), std::nullopt);
  EXPECT_EQ(section->loadPage(1)->visibleTextOffset, 0u);

  // Tables cut short by the file's end.
  bytes = good;
  put<uint32_t>(bytes, PARAGRAPHS_AT, size - 4);
  put<uint32_t>(bytes, LIST_ITEMS_AT, size - 2);
  writeBin(bytes);
  section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->getPageForParagraphIndex(0), std::nullopt);
  EXPECT_EQ(section->getParagraphIndexForPage(d.pageCount - 1), std::nullopt);
  EXPECT_EQ(section->getPageForListItemIndex(0), std::nullopt);

  // A page count past the visible LUT.
  bytes = good;
  put<uint16_t>(bytes, PAGE_COUNT_AT, d.pageCount + 50);
  writeBin(bytes);
  section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->pageCount, d.pageCount + 50);
  EXPECT_EQ(section->getVisibleTextOffsetForPage(d.pageCount + 10), std::nullopt);
  EXPECT_EQ(section->getPageForVisibleTextOffset(0), std::nullopt);
}

TEST_F(SectionFileTest, AnAnchorMapCutShortFindsNothingPastTheCut) {
  build(paragraphs(12, 10));
  auto bytes = readBin();
  const Decoded d = decode(bytes);
  ASSERT_GE(d.anchors.size(), 3u);
  // A huge count and a name length past the file: the lookup stops at the end.
  const auto anchors = get<uint32_t>(bytes, ANCHORS_AT);
  put<uint16_t>(bytes, anchors, 0xFFFF);
  put<uint32_t>(bytes, anchors + 2 + 4 + d.anchors[0].first.size() + 2, 0x7FFFFFFF);
  writeBin(bytes);
  const auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->getPageForAnchor(d.anchors[0].first), d.anchors[0].second);
  EXPECT_EQ(section->getPageForAnchor(d.anchors[2].first), std::nullopt);
  EXPECT_EQ(section->getPageForAnchor("missing"), std::nullopt);
}

TEST_F(SectionFileTest, AMalformedPartialIsDropped) {
  epub->contents[epub->spine[0]] = paragraphs(24, 10);
  {
    Section section(epub, 0, renderer);
    ASSERT_TRUE(section.startBuild(spec()));
    ASSERT_TRUE(section.buildSomeMore(2));
  }
  const auto good = readBin();
  const Decoded d = decode(good);
  const std::vector<std::function<void(std::vector<uint8_t>&)>> damages = {
      [](std::vector<uint8_t>& b) { b.resize(b.size() - 1); },
      [](std::vector<uint8_t>& b) { put<uint16_t>(b, PAGE_COUNT_AT, 0); },
      [&](std::vector<uint8_t>& b) { put<uint16_t>(b, PAGE_COUNT_AT, d.pageCount + 1); },
      [](std::vector<uint8_t>& b) { put<uint32_t>(b, LIST_ITEMS_AT, HEADER_SIZE - 1); },
      [](std::vector<uint8_t>& b) { put<uint32_t>(b, VISIBLE_AT, get<uint32_t>(b, LIST_ITEMS_AT)); },
  };
  for (size_t i = 0; i < damages.size(); ++i) {
    auto bytes = good;
    damages[i](bytes);
    writeBin(bytes);
    EXPECT_EQ(loaded(), nullptr) << i;
    EXPECT_FALSE(fs::exists(binPath())) << i;
  }
  // The trailer is found from the visible LUT, so extra bytes after it are ignored.
  auto bytes = good;
  bytes.push_back(0xAB);
  writeBin(bytes);
  const auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_TRUE(section->isPartial());
}

TEST_F(SectionFileTest, AShortFileReadsAsEmpty) {
  build(paragraphs(4, 10));
  const auto good = readBin();
  // The header still matches; the page count is the last field it can read.
  auto bytes = good;
  bytes.resize(HEADER_SIZE - 1);
  writeBin(bytes);
  auto section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->pageCount, get<uint16_t>(good, PAGE_COUNT_AT));
  EXPECT_EQ(section->getCachedPageCount(), std::nullopt);
  EXPECT_EQ(section->getVisibleTextOffsetForPage(0), std::nullopt);
  EXPECT_EQ(section->getPageForVisibleTextOffset(0), std::nullopt);
  EXPECT_EQ(section->getPageForParagraphIndex(0), std::nullopt);
  EXPECT_EQ(section->getPageForAnchor("p0"), std::nullopt);

  // Cut inside the page count, which then reads as zero.
  bytes.resize(PAGE_COUNT_AT + 1);
  writeBin(bytes);
  section = loaded();
  ASSERT_TRUE(section);
  EXPECT_EQ(section->pageCount, 0);
  EXPECT_EQ(section->loadPage(0), nullptr);

  writeBin({});
  EXPECT_EQ(loaded(), nullptr);
  EXPECT_FALSE(fs::exists(binPath()));
}

// Random damage anywhere in the file: no lookup may crash or hang.
TEST_F(SectionFileTest, AnyDamageNeverCrashes) {
  build(paragraphs(12, 10));
  const auto good = readBin();
  uint32_t seed = 0x5ec7105e;
  const auto next = [&seed]() {
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    return seed;
  };
  for (int round = 0; round < 300; ++round) {
    auto bytes = good;
    for (uint32_t n = next() % 8 + 1; n > 0; --n) {
      // Mostly the header and tables, where the offsets and counts are.
      const size_t at = next() % 2 ? next() % HEADER_SIZE : next() % bytes.size();
      bytes[at] = static_cast<uint8_t>(next());
    }
    if (next() % 4 == 0) bytes.resize(next() % bytes.size());
    if (!bytes.empty()) bytes[0] = next() % 2 ? VERSION : PARTIAL_VERSION;
    writeBin(bytes);
    const auto section = loaded();
    if (!section) continue;
    for (int page = 0; page < 4; ++page) {
      (void)section->loadPage(page);
      (void)section->getVisibleTextOffsetForPage(page);
      (void)section->getParagraphIndexForPage(page);
    }
    (void)section->getCachedPageCount();
    (void)section->getPageForVisibleTextOffset(next() % 2000, next() % 2);
    (void)section->getPageForParagraphIndex(next() % 20);
    (void)section->getPageForListItemIndex(next() % 20);
    (void)section->getPageForAnchor("p3");
    (void)section->estimatedTotalPages();
  }
}

}  // namespace
