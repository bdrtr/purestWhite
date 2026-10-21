// Pins lib/Xtc's XtcParser: header checks, metadata, the page table, page loading and chapters.
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <string>
#include <vector>

#include "XtcParser.h"

using xtc::XtcError;
using xtc::XtcParser;

namespace {

void put(std::vector<uint8_t>& v, const size_t at, const uint64_t value, const int bytes) {
  if (v.size() < at + bytes) v.resize(at + bytes);
  for (int i = 0; i < bytes; i++) v[at + i] = static_cast<uint8_t>(value >> (8 * i));
}

struct Page {
  uint16_t width;
  uint16_t height;
  uint8_t fill;
};

struct Chapter {
  std::string name;
  uint16_t start;  // 1-based, as stored
  uint16_t end;
};

// An XTC (or XTCH with twoBit) book: header, optional metadata, chapters, page table, pages.
std::vector<uint8_t> makeBook(const std::vector<Page>& pages, const bool twoBit = false, const std::string& title = "",
                              const std::string& author = "", const std::vector<Chapter>& chapters = {}) {
  std::vector<uint8_t> v(56, 0);
  put(v, 0, twoBit ? xtc::XTCH_MAGIC : xtc::XTC_MAGIC, 4);
  v[4] = 1;
  put(v, 6, pages.size(), 2);
  v[9] = title.empty() && author.empty() ? 0 : 1;
  v[11] = chapters.empty() ? 0 : 1;
  if (v[9]) {
    v.resize(std::max<size_t>({0xF8, 0x38 + title.size(), 0xB8 + author.size()}), 0);
    std::copy(title.begin(), title.end(), v.begin() + 0x38);
    std::copy(author.begin(), author.end(), v.begin() + 0xB8);
  }
  if (!chapters.empty()) {
    put(v, 0x30, v.size(), 4);
    for (const auto& c : chapters) {
      const size_t at = v.size();
      v.resize(at + 96, 0);
      std::copy(c.name.begin(), c.name.end(), v.begin() + at);
      put(v, at + 0x50, c.start, 2);
      put(v, at + 0x52, c.end, 2);
    }
  }
  const size_t table = v.size();
  put(v, 0x18, table, 8);
  v.resize(table + 16 * pages.size(), 0);
  put(v, 0x20, v.size(), 8);
  for (size_t i = 0; i < pages.size(); i++) {
    const auto& p = pages[i];
    const size_t size = twoBit ? p.width * ((p.height + 7) / 8) * 2 : ((p.width + 7) / 8) * p.height;
    const size_t at = v.size();
    put(v, table + 16 * i, at, 8);
    put(v, table + 16 * i + 8, size + 22, 4);
    put(v, table + 16 * i + 12, p.width, 2);
    put(v, table + 16 * i + 14, p.height, 2);
    put(v, at, twoBit ? xtc::XTH_MAGIC : xtc::XTG_MAGIC, 4);
    put(v, at + 4, p.width, 2);
    put(v, at + 6, p.height, 2);
    put(v, at + 10, size, 4);
    v.resize(at + 22 + size, p.fill);
  }
  return v;
}

XtcError openBook(XtcParser& parser, const std::vector<uint8_t>& data, const std::string& name = "/book.xtc") {
  Storage.files[name] = data;
  Storage.opensLeft = -1;
  return parser.open(name.c_str());
}

}  // namespace

TEST(XtcParser, OpensABookWithMetadata) {
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{480, 800, 0xAA}, {16, 3, 0x55}}, false, "A Title", "An Author")), XtcError::OK);
  EXPECT_TRUE(parser.isOpen());
  EXPECT_EQ(parser.getPageCount(), 2);
  EXPECT_EQ(parser.getWidth(), 480);
  EXPECT_EQ(parser.getHeight(), 800);
  EXPECT_EQ(parser.getBitDepth(), 1);
  EXPECT_EQ(parser.getTitle(), "A Title");
  EXPECT_EQ(parser.getAuthor(), "An Author");
  EXPECT_FALSE(parser.hasChapters());

  xtc::PageInfo info{};
  ASSERT_TRUE(parser.getPageInfo(1, info));
  EXPECT_EQ(info.width, 16);
  EXPECT_EQ(info.height, 3);
  EXPECT_EQ(info.size, 6u + 22);
  EXPECT_EQ(info.bitDepth, 1);
  EXPECT_FALSE(parser.getPageInfo(2, info));

  std::vector<uint8_t> buffer(6, 0);
  EXPECT_EQ(parser.loadPage(1, buffer.data(), buffer.size()), 6u);
  EXPECT_EQ(buffer, std::vector<uint8_t>(6, 0x55));
  EXPECT_EQ(parser.getLastError(), XtcError::OK);
}

TEST(XtcParser, MetadataStopsAtTheFieldEnds) {
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{8, 1, 0}}, false, std::string(130, 't'), std::string(70, 'a'))), XtcError::OK);
  // The title runs into the author field: 127 bytes are kept.
  EXPECT_EQ(parser.getTitle(), std::string(127, 't'));
  EXPECT_EQ(parser.getAuthor(), std::string(63, 'a'));
}

TEST(XtcParser, TwoBitBooksUseColumnPlanes) {
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{5, 9, 0x11}}, true)), XtcError::OK);
  EXPECT_EQ(parser.getBitDepth(), 2);
  std::vector<uint8_t> buffer(5 * 2 * 2, 0);
  EXPECT_EQ(parser.loadPage(0, buffer.data(), buffer.size()), 20u);
  EXPECT_EQ(parser.loadPage(0, buffer.data(), 19), 0u);
  EXPECT_EQ(parser.getLastError(), XtcError::MEMORY_ERROR);
}

TEST(XtcParser, RefusesBadHeaders) {
  const auto good = makeBook({{8, 2, 0}});
  const auto check = [](std::vector<uint8_t> data, const XtcError expected) {
    XtcParser parser;
    EXPECT_EQ(openBook(parser, data), expected) << xtc::errorToString(expected);
    EXPECT_EQ(parser.isOpen(), expected == XtcError::OK);
  };
  check(good, XtcError::OK);
  check(std::vector<uint8_t>(good.begin(), good.begin() + 55), XtcError::READ_ERROR);
  auto magic = good;
  magic[3] = 'X';
  check(magic, XtcError::INVALID_MAGIC);
  auto swapped = good;
  swapped[4] = 0;
  swapped[5] = 1;
  check(swapped, XtcError::OK);
  auto version = good;
  version[4] = 2;
  check(version, XtcError::INVALID_VERSION);
  auto empty = good;
  put(empty, 6, 0, 2);
  check(empty, XtcError::CORRUPTED_HEADER);
  auto noTable = good;
  put(noTable, 0x18, 0, 8);
  check(noTable, XtcError::CORRUPTED_HEADER);
  auto early = good;
  put(early, 0x18, 0x2F, 8);
  check(early, XtcError::CORRUPTED_HEADER);
  auto legacy = good;
  put(legacy, 0x18, 0x30, 8);
  check(legacy, XtcError::OK);
  auto past = good;
  put(past, 0x18, good.size() + 1, 8);
  check(past, XtcError::CORRUPTED_HEADER);
  auto tooMany = good;
  put(tooMany, 6, 500, 2);
  check(tooMany, XtcError::CORRUPTED_HEADER);
  auto huge = good;
  put(huge, 0x18, UINT64_MAX - 4, 8);
  check(huge, XtcError::CORRUPTED_HEADER);
  XtcParser parser;
  Storage.opensLeft = -1;
  EXPECT_EQ(parser.open("/missing.xtc"), XtcError::FILE_NOT_FOUND);
}

TEST(XtcParser, MetadataPastTheEndIsAReadError) {
  auto data = makeBook({{8, 1, 0}});
  data[9] = 1;  // metadata, but the file ends inside the title: the author cannot be reached
  XtcParser parser;
  Storage.files["/short.xtc"] = std::vector<uint8_t>(data.begin(), data.begin() + 0x40);
  EXPECT_EQ(parser.open("/short.xtc"), XtcError::READ_ERROR);
}

TEST(XtcParser, PagesAreChecked) {
  auto data = makeBook({{8, 2, 7}, {8, 2, 7}, {8, 2, 7}});
  const size_t table = 56;
  const size_t page1 = data[table + 16] | (data[table + 17] << 8) | (data[table + 18] << 16);
  const size_t page2 = data[table + 32] | (data[table + 33] << 8) | (data[table + 34] << 16);
  data[page1] = 'Q';         // page 1: bad magic
  put(data, page2 + 10, 3, 4);  // page 2: declared size does not match 8x2
  XtcParser parser;
  ASSERT_EQ(openBook(parser, data), XtcError::OK);
  std::vector<uint8_t> buffer(16, 0);
  EXPECT_EQ(parser.loadPage(0, buffer.data(), buffer.size()), 2u);
  EXPECT_EQ(parser.loadPage(1, buffer.data(), buffer.size()), 0u);
  EXPECT_EQ(parser.getLastError(), XtcError::INVALID_MAGIC);
  EXPECT_EQ(parser.loadPage(2, buffer.data(), buffer.size()), 0u);
  EXPECT_EQ(parser.getLastError(), XtcError::CORRUPTED_HEADER);
  EXPECT_EQ(parser.loadPage(3, buffer.data(), buffer.size()), 0u);
  EXPECT_EQ(parser.getLastError(), XtcError::PAGE_OUT_OF_RANGE);

  const auto ignore = [](const uint8_t*, size_t, size_t) {};
  EXPECT_EQ(parser.loadPageStreaming(1, ignore), XtcError::READ_ERROR);
  EXPECT_EQ(parser.loadPageStreaming(2, ignore), XtcError::CORRUPTED_HEADER);
  EXPECT_EQ(parser.loadPageStreaming(3, ignore), XtcError::PAGE_OUT_OF_RANGE);

  // A page whose data is cut short.
  auto cut = makeBook({{64, 10, 9}});
  cut.resize(cut.size() - 1);
  XtcParser second;
  ASSERT_EQ(openBook(second, cut, "/cut.xtc"), XtcError::OK);
  std::vector<uint8_t> big(80, 0);
  EXPECT_EQ(second.loadPage(0, big.data(), big.size()), 0u);
  EXPECT_EQ(second.getLastError(), XtcError::READ_ERROR);
  size_t streamed = 0;
  EXPECT_EQ(second.loadPageStreaming(
                0, [&](const uint8_t*, const size_t size, size_t) { streamed += size; }, 32),
            XtcError::READ_ERROR);
  EXPECT_EQ(streamed, 79u);

  XtcParser closed;
  EXPECT_EQ(closed.loadPage(0, big.data(), big.size()), 0u);
  EXPECT_EQ(closed.getLastError(), XtcError::FILE_NOT_FOUND);
  EXPECT_EQ(closed.loadPageStreaming(0, ignore), XtcError::FILE_NOT_FOUND);
}

TEST(XtcParser, StreamsAPageInChunks) {
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{40, 10, 0x3C}})), XtcError::OK);
  std::vector<std::pair<size_t, size_t>> chunks;
  std::vector<uint8_t> all;
  EXPECT_EQ(parser.loadPageStreaming(
                0,
                [&](const uint8_t* data, const size_t size, const size_t offset) {
                  chunks.emplace_back(size, offset);
                  all.insert(all.end(), data, data + size);
                },
                16),
            XtcError::OK);
  EXPECT_EQ(chunks, (std::vector<std::pair<size_t, size_t>>{{16, 0}, {16, 16}, {16, 32}, {2, 48}}));
  EXPECT_EQ(all, std::vector<uint8_t>(50, 0x3C));
}

TEST(XtcParser, ChaptersAreOneBasedAndClamped) {
  const std::vector<Chapter> chapters = {
      {"One", 1, 2}, {"Two", 3, 99}, {"Late", 7, 9}, {"Backwards", 3, 2}, {"", 0, 0}, {"After end", 1, 1}};
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{8, 1, 0}, {8, 1, 0}, {8, 1, 0}, {8, 1, 0}}, false, "", "", chapters)),
            XtcError::OK);
  EXPECT_TRUE(parser.hasChapters());
  const auto& read = parser.getChapters();
  ASSERT_EQ(read.size(), 2u);
  EXPECT_EQ(read[0].name, "One");
  EXPECT_EQ(read[0].startPage, 0);
  EXPECT_EQ(read[0].endPage, 1);
  EXPECT_EQ(read[1].name, "Two");
  EXPECT_EQ(read[1].startPage, 2);
  EXPECT_EQ(read[1].endPage, 3);
}

TEST(XtcParser, ChapterNamesStopAt80Bytes) {
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{8, 1, 0}}, false, "", "", {{std::string(90, 'n'), 0, 0}})), XtcError::OK);
  const auto& read = parser.getChapters();
  ASSERT_EQ(read.size(), 1u);
  EXPECT_EQ(read[0].name, std::string(80, 'n'));
}

TEST(XtcParser, ChapterTablesOutOfBoundsAreIgnored) {
  const auto base = makeBook({{8, 1, 0}, {8, 1, 0}}, false, "", "", {{"One", 1, 1}, {"Two", 2, 2}});
  const auto chaptersOf = [](std::vector<uint8_t> data) {
    XtcParser parser;
    EXPECT_EQ(openBook(parser, data), XtcError::OK);
    const auto& read = parser.getChapters();
    std::vector<std::string> names;
    for (const auto& c : read) names.push_back(c.name);
    return std::make_pair(names, parser.hasChapters());
  };
  EXPECT_EQ(chaptersOf(base).first, (std::vector<std::string>{"One", "Two"}));
  auto padding = base;
  padding[0x37] = 1;  // read with the chapter offset as 8 bytes
  EXPECT_EQ(chaptersOf(padding), std::make_pair(std::vector<std::string>{}, true));
  auto inside = base;
  put(inside, 0x30, 40, 4);
  EXPECT_EQ(chaptersOf(inside).first, std::vector<std::string>{});
  auto zero = base;
  put(zero, 0x30, 0, 4);
  EXPECT_EQ(chaptersOf(zero), std::make_pair(std::vector<std::string>{}, true));
  auto flag = base;
  flag[11] = 2;
  EXPECT_FALSE(chaptersOf(flag).second);
  // With the page table at the chapters, the data offset bounds them; with both there, the file
  // end does (records past the chapters read as further, out-of-range chapters).
  auto dataBound = base;
  put(dataBound, 0x18, 56, 8);
  put(dataBound, 0x20, 56 + 96 + 10, 8);
  EXPECT_EQ(chaptersOf(dataBound).first, (std::vector<std::string>{"One"}));
  auto fileBound = dataBound;
  put(fileBound, 0x20, 56, 8);
  EXPECT_EQ(chaptersOf(fileBound).first, (std::vector<std::string>{"One", "Two"}));
}

TEST(XtcParser, AFailedReopenLeavesNoChapters) {
  XtcParser parser;
  ASSERT_EQ(openBook(parser, makeBook({{8, 1, 0}}, false, "", "", {{"One", 1, 1}})), XtcError::OK);
  Storage.opensLeft = 0;
  EXPECT_TRUE(parser.getChapters().empty());
  EXPECT_FALSE(parser.hasChapters());
  xtc::PageInfo info{};
  EXPECT_FALSE(parser.getPageInfo(0, info));
  Storage.opensLeft = -1;
  EXPECT_TRUE(parser.getPageInfo(0, info));
}

TEST(XtcParser, RecognizesItsMagic) {
  Storage.files["/a.xtc"] = makeBook({{8, 1, 0}});
  Storage.files["/b.xtch"] = makeBook({{8, 1, 0}}, true);
  Storage.files["/c.bin"] = {'X', 'T', 'G', 0};
  Storage.files["/d.bin"] = {'X', 'T', 'C'};
  Storage.opensLeft = -1;
  EXPECT_TRUE(XtcParser::isValidXtcFile("/a.xtc"));
  EXPECT_TRUE(XtcParser::isValidXtcFile("/b.xtch"));
  EXPECT_FALSE(XtcParser::isValidXtcFile("/c.bin"));
  EXPECT_FALSE(XtcParser::isValidXtcFile("/d.bin"));
  EXPECT_FALSE(XtcParser::isValidXtcFile("/none"));
}
