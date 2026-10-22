#include <HalStorage.h>
#include <gtest/gtest.h>

#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iterator>
#include <string>
#include <vector>

#include "Epub/BookMetadataCache.h"
#include "ZipFile.h"

namespace fs = std::filesystem;

namespace {

using Cache = BookMetadataCache;

// book.bin as written today, decoded independently: version (u8), LUT offset (u32), spine and TOC
// counts (u16 each), five metadata strings (u32 length, bytes), the spine LUT then the TOC LUT (u32
// positions), the spine entries (href, cumulative size u32, TOC index i16) and the TOC entries (title,
// href, anchor, level u8, spine index i16).
struct Decoded {
  uint8_t version = 0;
  uint32_t lutOffset = 0;
  uint16_t spineCount = 0, tocCount = 0;
  std::vector<std::string> metadata;
  std::vector<uint32_t> spineLut, tocLut;
  std::vector<Cache::SpineEntry> spine;
  std::vector<Cache::TocEntry> toc;
};

struct Reader {
  const std::vector<uint8_t>& b;
  size_t at = 0;
  template <typename T>
  T pod() {
    T v{};
    std::memcpy(&v, b.data() + at, sizeof(T));
    at += sizeof(T);
    return v;
  }
  std::string str() {
    const auto len = pod<uint32_t>();
    std::string s(reinterpret_cast<const char*>(b.data() + at), len);
    at += len;
    return s;
  }
};

Decoded decode(const std::vector<uint8_t>& bytes) {
  Decoded d;
  Reader r{bytes};
  d.version = r.pod<uint8_t>();
  d.lutOffset = r.pod<uint32_t>();
  d.spineCount = r.pod<uint16_t>();
  d.tocCount = r.pod<uint16_t>();
  for (int i = 0; i < 5; ++i) d.metadata.push_back(r.str());
  EXPECT_EQ(r.at, d.lutOffset);
  for (int i = 0; i < d.spineCount; ++i) d.spineLut.push_back(r.pod<uint32_t>());
  for (int i = 0; i < d.tocCount; ++i) d.tocLut.push_back(r.pod<uint32_t>());
  for (int i = 0; i < d.spineCount; ++i) {
    EXPECT_EQ(r.at, d.spineLut[i]) << "spine " << i;
    Cache::SpineEntry e;
    e.href = r.str();
    e.cumulativeSize = r.pod<uint32_t>();
    e.tocIndex = r.pod<int16_t>();
    d.spine.push_back(e);
  }
  for (int i = 0; i < d.tocCount; ++i) {
    EXPECT_EQ(r.at, d.tocLut[i]) << "toc " << i;
    Cache::TocEntry e;
    e.title = r.str();
    e.href = r.str();
    e.anchor = r.str();
    e.level = r.pod<uint8_t>();
    e.spineIndex = r.pod<int16_t>();
    d.toc.push_back(e);
  }
  EXPECT_EQ(r.at, bytes.size());
  return d;
}

struct Toc {
  std::string title, href, anchor;
  uint8_t level;
};

class BookMetadataCacheFormat : public ::testing::Test {
 protected:
  std::string dir;
  void SetUp() override {
    dir = testing::TempDir() + "bmc_format_" + testing::UnitTest::GetInstance()->current_test_info()->name();
    fs::remove_all(dir);
    fs::create_directories(dir);
    zipEntrySizes.clear();
  }
  void TearDown() override { fs::remove_all(dir); }

  std::string bin() const { return dir + "/book.bin"; }
  std::vector<uint8_t> readBin() const {
    std::ifstream in(bin(), std::ios::binary);
    return {std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>()};
  }
  void writeBin(const std::vector<uint8_t>& bytes) const {
    std::ofstream out(bin(), std::ios::binary | std::ios::trunc);
    out.write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
  }

  bool build(const std::vector<std::string>& spine, const std::vector<Toc>& toc,
             const Cache::BookMetadata& metadata = {}, const std::string& bookPath = "book.epub") {
    Cache cache(dir);
    if (!cache.beginWrite() || !cache.beginContentOpfPass()) return false;
    for (const auto& href : spine) cache.createSpineEntry(href);
    if (!cache.endContentOpfPass() || !cache.beginTocPass()) return false;
    for (const auto& t : toc) cache.createTocEntry(t.title, t.href, t.anchor, t.level);
    if (!cache.endTocPass() || !cache.endWrite()) return false;
    const bool built = cache.buildBookBin(bookPath, metadata);
    cache.cleanupTmpFiles();
    return built;
  }
};

TEST_F(BookMetadataCacheFormat, ABookIsWrittenInTheFormat) {
  zipEntrySizes = {{"a.xhtml", 100}, {"b.xhtml", 0}, {"c.xhtml", 50}, {"d.xhtml", 7}};
  Cache::BookMetadata metadata{"Başlık", "Yazar", "tr", "cover.jpg", "a.xhtml"};
  // b has no TOC entry and takes a's; d has two and takes the first; a TOC entry outside the spine
  // has no spine index.
  ASSERT_TRUE(build({"a.xhtml", "b.xhtml", "c.xhtml", "d.xhtml"},
                    {{"One", "a.xhtml", "", 1},
                     {"Three", "c.xhtml", "part", 2},
                     {"Four", "d.xhtml", "", 1},
                     {"Four again", "d.xhtml", "x", 3},
                     {"Elsewhere", "e.xhtml", "", 1}},
                    metadata));
  const Decoded d = decode(readBin());
  EXPECT_EQ(d.version, 11);
  EXPECT_EQ(d.spineCount, 4);
  EXPECT_EQ(d.tocCount, 5);
  EXPECT_EQ(d.metadata, (std::vector<std::string>{"Başlık", "Yazar", "tr", "cover.jpg", "a.xhtml"}));
  const std::vector<uint32_t> cumulative = {100, 100, 150, 157};
  const std::vector<int16_t> tocIndex = {0, 0, 1, 2};
  for (int i = 0; i < 4; ++i) {
    EXPECT_EQ(d.spine[i].cumulativeSize, cumulative[i]) << i;
    EXPECT_EQ(d.spine[i].tocIndex, tocIndex[i]) << i;
  }
  const std::vector<int16_t> spineIndex = {0, 2, 3, 3, -1};
  for (int i = 0; i < 5; ++i) EXPECT_EQ(d.toc[i].spineIndex, spineIndex[i]) << i;
  EXPECT_EQ(d.toc[1].anchor, "part");
  EXPECT_EQ(d.toc[3].level, 3);

  Cache cache(dir);
  ASSERT_TRUE(cache.load());
  EXPECT_EQ(cache.coreMetadata.title, "Başlık");
  EXPECT_EQ(cache.coreMetadata.textReferenceHref, "a.xhtml");
  EXPECT_EQ(cache.getSpineCount(), 4);
  EXPECT_EQ(cache.getTocCount(), 5);
  for (int i = 0; i < 4; ++i) {
    const auto e = cache.getSpineEntry(i);
    EXPECT_EQ(e.href, d.spine[i].href);
    EXPECT_EQ(e.cumulativeSize, cumulative[i]);
    EXPECT_EQ(e.tocIndex, tocIndex[i]);
    EXPECT_EQ(cache.getCumulativeSize(i), cumulative[i]);
  }
  for (int i = 0; i < 5; ++i) {
    const auto e = cache.getTocEntry(i);
    EXPECT_EQ(e.title, d.toc[i].title);
    EXPECT_EQ(e.anchor, d.toc[i].anchor);
    EXPECT_EQ(e.level, d.toc[i].level);
    EXPECT_EQ(e.spineIndex, spineIndex[i]);
  }
  EXPECT_EQ(cache.getSpineEntry(4).href, "");
  EXPECT_EQ(cache.getSpineEntry(-1).tocIndex, -1);
  EXPECT_EQ(cache.getTocEntry(5).spineIndex, -1);
  EXPECT_EQ(cache.getSpineIndexForSize(101), 2);
  EXPECT_EQ(cache.getSpineIndexForSize(158), -1);
}

// Spine items before the first TOC entry have none; TOC titles are stored composed (NFC).
TEST_F(BookMetadataCacheFormat, TitlesAreComposedAndLeadingItemsHaveNoTocEntry) {
  zipEntrySizes = {{"cover.xhtml", 1}, {"a.xhtml", 2}};
  ASSERT_TRUE(build({"cover.xhtml", "a.xhtml"}, {{"Cafe\xCC\x81", "a.xhtml", "", 1}}));
  const Decoded d = decode(readBin());
  EXPECT_EQ(d.spine[0].tocIndex, -1);
  EXPECT_EQ(d.spine[1].tocIndex, 0);
  EXPECT_EQ(d.toc[0].title, "Caf\xC3\xA9");
}

// From 400 spine items TOC hrefs are matched through a hash index; the result is the same.
TEST_F(BookMetadataCacheFormat, ALargeSpineMapsTheTocTheSameWay) {
  for (const int count : {399, 400, 900}) {
    std::vector<std::string> spine;
    std::vector<Toc> toc;
    for (int i = 0; i < count; ++i) {
      spine.push_back("ch" + std::to_string(i) + ".xhtml");
      zipEntrySizes[spine.back()] = 10 + i % 7;
      if (i % 3 == 0) toc.push_back({"T" + std::to_string(i), spine.back(), "", 1});
    }
    toc.push_back({"missing", "nope.xhtml", "", 1});
    ASSERT_TRUE(build(spine, toc)) << count;
    const Decoded d = decode(readBin());
    for (size_t j = 0; j + 1 < toc.size(); ++j) EXPECT_EQ(d.toc[j].spineIndex, 3 * j) << count << " " << j;
    EXPECT_EQ(d.toc.back().spineIndex, -1) << count;
    uint32_t total = 0;
    for (int i = 0; i < count; ++i) {
      total += 10 + i % 7;
      ASSERT_EQ(d.spine[i].cumulativeSize, total) << count << " " << i;
      ASSERT_EQ(d.spine[i].tocIndex, i / 3) << count << " " << i;
    }
  }
}

// A text book's one spine item counts the file's size; a missing zip entry counts nothing.
TEST_F(BookMetadataCacheFormat, TextBooksCountTheFileAndMissingEntriesNothing) {
  const std::string text = dir + "/book.txt";
  std::ofstream(text) << std::string(1234, 'x');
  ASSERT_TRUE(build({"book.txt"}, {{"Book", "book.txt", "", 1}}, {}, text));
  EXPECT_EQ(decode(readBin()).spine[0].cumulativeSize, 1234u);

  zipEntrySizes = {{"a.xhtml", 5}};
  ASSERT_TRUE(build({"a.xhtml", "gone.xhtml", "a.xhtml"}, {}));
  const Decoded d = decode(readBin());
  EXPECT_EQ(d.spine[1].cumulativeSize, 5u);
  EXPECT_EQ(d.spine[2].cumulativeSize, 10u);
  EXPECT_EQ(d.spine[2].tocIndex, -1);
}

TEST_F(BookMetadataCacheFormat, AnotherVersionIsNotLoaded) {
  zipEntrySizes = {{"a.xhtml", 5}};
  ASSERT_TRUE(build({"a.xhtml"}, {}));
  auto bytes = readBin();
  for (const uint8_t version : {uint8_t{0}, uint8_t{10}, uint8_t{12}}) {
    bytes[0] = version;
    writeBin(bytes);
    Cache cache(dir);
    EXPECT_FALSE(cache.load()) << int(version);
    EXPECT_FALSE(cache.isLoaded());
  }
  writeBin({});
  EXPECT_FALSE(Cache(dir).load());
}

// What the readers give for tables pointing past the file today.
TEST_F(BookMetadataCacheFormat, EntriesPastTheEndReadEmpty) {
  zipEntrySizes = {{"a.xhtml", 5}, {"b.xhtml", 6}};
  ASSERT_TRUE(build({"a.xhtml", "b.xhtml"}, {{"A", "a.xhtml", "an", 2}}));
  auto bytes = readBin();
  const Decoded d = decode(bytes);
  // The second spine LUT entry and the TOC LUT entry point past the end.
  const auto past = static_cast<uint32_t>(bytes.size());
  std::memcpy(&bytes[d.lutOffset + 4], &past, 4);
  std::memcpy(&bytes[d.lutOffset + 8], &past, 4);
  writeBin(bytes);
  Cache cache(dir);
  ASSERT_TRUE(cache.load());
  EXPECT_EQ(cache.getSpineEntry(0).href, "a.xhtml");
  const auto spine = cache.getSpineEntry(1);
  EXPECT_EQ(spine.href, "");
  EXPECT_EQ(spine.cumulativeSize, 0u);
  EXPECT_EQ(spine.tocIndex, 0);
  const auto toc = cache.getTocEntry(0);
  EXPECT_EQ(toc.title, "");
  EXPECT_EQ(toc.level, 0);
  EXPECT_EQ(toc.spineIndex, 0);
  // Sizes come from the entries, which are intact.
  EXPECT_EQ(cache.getCumulativeSize(1), 11u);
}

// A spine entry with a name length past the end still loads: the sizes after it read as zero, and
// its window, which cannot be read whole, answers the total before it.
TEST_F(BookMetadataCacheFormat, ADamagedSpineEntryLoadsWithZeroSizes) {
  zipEntrySizes = {{"a.xhtml", 5}, {"b.xhtml", 6}, {"c.xhtml", 7}};
  ASSERT_TRUE(build({"a.xhtml", "b.xhtml", "c.xhtml"}, {}));
  auto bytes = readBin();
  const Decoded d = decode(bytes);
  const uint32_t huge = 0x7FFFFFFF;
  std::memcpy(&bytes[d.spineLut[1]], &huge, 4);
  writeBin(bytes);
  Cache cache(dir);
  ASSERT_TRUE(cache.load());
  EXPECT_EQ(cache.getCumulativeSize(0), 0u);
  EXPECT_EQ(cache.getCumulativeSize(2), 0u);  // the book's total, read as zero
  EXPECT_EQ(cache.getSpineIndexForSize(1), -1);
  EXPECT_EQ(cache.getSpineEntry(1).href, "");
}

TEST_F(BookMetadataCacheFormat, AnyDamageNeverCrashes) {
  std::vector<std::string> spine;
  std::vector<Toc> toc;
  for (int i = 0; i < 70; ++i) {
    spine.push_back("ch" + std::to_string(i) + ".xhtml");
    zipEntrySizes[spine.back()] = 100 + i;
    if (i % 2) toc.push_back({"T", spine.back(), "a", 1});
  }
  ASSERT_TRUE(build(spine, toc, {"t", "a", "en", "c", "r"}));
  const auto good = readBin();
  uint32_t seed = 0xb00cca5e;
  const auto next = [&seed]() {
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    return seed;
  };
  for (int round = 0; round < 400; ++round) {
    auto bytes = good;
    for (uint32_t n = next() % 6 + 1; n > 0; --n) bytes[next() % bytes.size()] = static_cast<uint8_t>(next());
    if (next() % 4 == 0) bytes.resize(next() % bytes.size());
    if (!bytes.empty()) bytes[0] = 11;
    writeBin(bytes);
    Cache cache(dir);
    if (!cache.load()) continue;
    for (int i = -1; i < 72; i += 7) {
      (void)cache.getSpineEntry(i);
      (void)cache.getTocEntry(i);
      (void)cache.getCumulativeSize(i);
    }
    (void)cache.getSpineIndexForSize(next() % 10000);
  }
}

}  // namespace
