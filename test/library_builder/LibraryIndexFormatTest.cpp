// The library index the builder writes, byte for byte: an independent decoder checks every
// section of each index, and recorded hashes pin whole indexes for generated cards (spelling
// votes, unknown authors, renames, arrival order) and for every allocation failure point.
#include <gtest/gtest.h>

#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

#include "Epub.h"
#include "LibraryBuilder.h"
#include "LibraryFormat.h"

using namespace library;

namespace {

constexpr char INDEX[] = "/.crosspoint/library.idx";

uint64_t fnv(const std::vector<uint8_t>& bytes, uint64_t h = 14695981039346656037ull) {
  for (const uint8_t b : bytes) h = (h ^ b) * 1099511628211ull;
  return h;
}

uint64_t mix(uint64_t h, const uint64_t value) {
  for (int i = 0; i < 8; i++) h = (h ^ ((value >> (8 * i)) & 0xFF)) * 1099511628211ull;
  return h;
}

struct Reader {
  const std::vector<uint8_t>& b;
  uint32_t u32(const size_t at) const {
    uint32_t v = 0;
    for (int i = 3; i >= 0; i--) v = v << 8 | b.at(at + i);
    return v;
  }
  uint16_t u16(const size_t at) const { return static_cast<uint16_t>(b.at(at) | b.at(at + 1) << 8); }
  uint64_t u64(const size_t at) const { return static_cast<uint64_t>(u32(at + 4)) << 32 | u32(at); }
};

uint32_t align(const uint32_t v) { return (v + 511) / 512 * 512; }

void expectZeros(const std::vector<uint8_t>& b, const size_t from, const size_t to, const char* what) {
  for (size_t i = from; i < to; i++) ASSERT_EQ(b.at(i), 0) << what << " padding at " << i;
}

// Decodes the index independently of the reader and checks every section against the header.
void checkIndex(const std::vector<uint8_t>& b) {
  const Reader r{b};
  ASSERT_GE(b.size(), 512u);
  ASSERT_EQ(std::memcmp(b.data(), "CLX1", 4), 0);
  EXPECT_EQ(b[4], CLIX_FORMAT_VERSION);
  EXPECT_EQ(b[5], CLIX_FOLD_VERSION);
  EXPECT_LE(b[6], 3);
  EXPECT_LE(b[7], 1);
  const uint16_t books = r.u16(8), folders = r.u16(10);
  EXPECT_EQ(r.u16(14), 0);
  const uint32_t folderStart = r.u32(16), folderLen = r.u32(20), recordStart = r.u32(24), permStart = r.u32(28),
                 nameStart = r.u32(32), nameLen = r.u32(36), selfSize = r.u32(40);
  expectZeros(b, 44, 64, "reserved");
  EXPECT_EQ(folderStart, 512u);
  EXPECT_EQ(recordStart, align(folderStart + folderLen));
  EXPECT_EQ(permStart, align(recordStart + books * 128u));
  EXPECT_EQ(nameStart, align(permStart + books * 4u));
  EXPECT_EQ(selfSize, nameStart + nameLen);
  ASSERT_EQ(b.size(), selfSize);
  expectZeros(b, 64, folderStart, "header");

  // Folders: a length byte (1..255) and that many path bytes each.
  std::vector<std::string> folderPaths;
  size_t at = folderStart;
  while (at < folderStart + folderLen) {
    const uint8_t len = b[at];
    ASSERT_GT(len, 0);
    ASSERT_LE(at + 1 + len, folderStart + folderLen);
    folderPaths.emplace_back(b.begin() + at + 1, b.begin() + at + 1 + len);
    at += 1 + len;
  }
  EXPECT_EQ(folderPaths.size(), folders);
  expectZeros(b, folderStart + folderLen, recordStart, "folders");

  // Records in title order, their name blobs in the same order and back to back.
  uint32_t blob = 0;
  std::vector<uint8_t> seen(books);
  for (uint16_t i = 0; i < books; i++) {
    const size_t rec = recordStart + i * 128u;
    EXPECT_EQ(r.u32(rec), blob) << i;
    EXPECT_LT(r.u16(rec + 10), folders) << i;
    const uint8_t nameLenByte = b[rec + 12], foldLen = b[rec + 13], keyLen = b[rec + 14];
    EXPECT_GT(nameLenByte, 0) << i;
    EXPECT_LE(foldLen, CLIX_FOLD_BYTES);
    EXPECT_LE(keyLen, CLIX_AUTHOR_KEY_BYTES);
    EXPECT_LE(b[rec + 15], 2);
    expectZeros(b, rec + 16 + foldLen, rec + 16 + CLIX_FOLD_BYTES, "fold");
    expectZeros(b, rec + 112 + keyLen, rec + 124, "author key");
    // Title order (unless the sorts were skipped): fold prefixes never go down.
    if (i > 0 && !(b[6] & CLIX_FLAG_RANKS_DEGRADED)) {
      const size_t prev = rec - 128;
      EXPECT_LE(std::memcmp(b.data() + prev + 16, b.data() + rec + 16, 12), 0) << i;
    }
    size_t p = nameStart + blob + 8 + nameLenByte;
    for (int field = 0; field < 3; field++) {
      ASSERT_LT(p, nameStart + nameLen) << i;
      p += 1 + b[p];
    }
    blob = static_cast<uint32_t>(p - nameStart);
  }
  EXPECT_EQ(blob, nameLen);
  expectZeros(b, recordStart + books * 128u, permStart, "records");

  // Both permutations name every record once.
  for (int perm = 0; perm < 2; perm++) {
    std::fill(seen.begin(), seen.end(), 0);
    for (uint16_t k = 0; k < books; k++) {
      const uint16_t ordinal = r.u16(permStart + (perm * books + k) * 2u);
      ASSERT_LT(ordinal, books);
      EXPECT_EQ(seen[ordinal]++, 0) << perm << ':' << k;
    }
  }
  expectZeros(b, permStart + books * 4u, nameStart, "permutations");
}

uint32_t seed;
uint32_t next() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}

const char* const AUTHORS[] = {
    "Lu Xun", "Xun Lu", "Lu  Xun", "Victor Hugo", "Hugo, Victor", "Hugo Victor", "Natsume Soseki", "Unknown", "",
    "Émile Zola", "Zola, Émile", "Emile Zola", "J. R. R. Tolkien", "Tolkien, J.R.R.", "Anonymous", "Ursula K. Le Guin",
};
const char* const TITLES[] = {"", "Germinal", "Les Misérables", "The Hobbit", "Kokoro", "A Madman's Diary",
                              "Ça", "The Dispossessed", "  Spaces  ", "Zebra", "Éclair"};
const char* const FOLDERS[] = {"", "/Books", "/Books/Old", "/Zola", "/日本"};
const char* const EXTS[] = {".epub", ".epub", ".epub", ".txt", ".md"};

std::vector<std::string> paths;

void addBook(const std::string& path) {
  fake::add(path, std::string(1 + next() % 24, 'x'), next() % 4 == 0 ? 0 : 1 + next() % 50);
  auto& meta = bookMetadata[path];
  meta.title = TITLES[next() % (sizeof(TITLES) / sizeof(*TITLES))];
  meta.author = AUTHORS[next() % (sizeof(AUTHORS) / sizeof(*AUTHORS))];
  meta.success = next() % 8 != 0;
  paths.push_back(path);
}

// A card of `count` books spread over a few folders.
void makeCard(const uint32_t s, const unsigned count) {
  fake::reset();
  bookMetadata.clear();
  paths.clear();
  seed = s * 2654435761u + 1;
  for (unsigned i = 0; i < count; i++) {
    const std::string name = "b" + std::to_string(next() % 1000) + "-" + std::to_string(i);
    addBook(std::string(FOLDERS[next() % 5]) + "/" + name + EXTS[next() % 5]);
  }
}

// Removes, renames (same size), adds and touches some books.
void changeCard() {
  for (int i = 0; i < 3 && !paths.empty(); i++) {
    const size_t k = next() % paths.size();
    Storage.remove(paths[k].c_str());
    paths.erase(paths.begin() + k);
  }
  for (int i = 0; i < 2 && !paths.empty(); i++) {
    const size_t k = next() % paths.size();
    const std::string to = "/Books/renamed" + std::to_string(i) + ".epub";
    Storage.rename(paths[k].c_str(), to.c_str());
    bookMetadata[to] = bookMetadata[paths[k]];
    paths[k] = to;
  }
  for (int i = 0; i < 3; i++) addBook("/Books/new" + std::to_string(i) + ".epub");
  for (int i = 0; i < 2 && !paths.empty(); i++) fake::files[paths[next() % paths.size()]]->time += 7;
}

// Builds and returns the index's hash (0 when no index), after checking it.
uint64_t buildAndHash(const bool readMetadata, bool* ok = nullptr) {
  BuildStats stats;
  const bool built = buildLibraryIndex("/", stats, readMetadata);
  if (ok) *ok = built;
  if (!fake::files.count(INDEX)) return 0;
  const auto& bytes = fake::files.at(INDEX)->bytes;
  checkIndex(bytes);
  return mix(fnv(bytes), (stats.ranksDegraded ? 1 : 0) | (stats.dedupDegraded ? 2 : 0) | (built ? 4 : 0));
}

}  // namespace

TEST(LibraryIndexFormat, GeneratedCardsMatchTheRecordedIndexes) {
  const uint64_t expected[][3] = {
      {0x87fdb5723f9eeda1ull, 0xe1972861c9539095ull, 0x06534c35655a1236ull},
      {0x3fb39d1d272a7694ull, 0xdc61becdcf9a4bbbull, 0xc73f1d456f79b504ull},
      {0x1ec95dad2f6d09ecull, 0x312da6dd70d31c67ull, 0x8b086285da19ededull},
      {0xfe115630b1cfd77cull, 0x91272e84471c63a9ull, 0x5c91f18be5e5ea4dull},
  };
  const unsigned counts[] = {1, 7, 60, 300};
  for (int c = 0; c < 4; c++) {
    makeCard(c + 1, counts[c]);
    const uint64_t first = buildAndHash(true);
    changeCard();
    const uint64_t changed = buildAndHash(true);
    const uint64_t plain = buildAndHash(false);
    EXPECT_EQ(first, expected[c][0]) << std::hex << c << " first 0x" << first;
    EXPECT_EQ(changed, expected[c][1]) << std::hex << c << " changed 0x" << changed;
    EXPECT_EQ(plain, expected[c][2]) << std::hex << c << " plain 0x" << plain;
  }
}

TEST(LibraryIndexFormat, EveryAllocationFailureMatchesTheRecordedOutcome) {
  uint64_t sweep = 14695981039346656037ull;
  unsigned points = 0;
  for (int k = 0; k < 64; k++) {
    makeCard(9, 40);
    fake::failAlloc = k;
    bool ok = false;
    const uint64_t h = buildAndHash(true, &ok);
    const bool failed = fake::failureTriggered;
    fake::failAlloc = -1;
    sweep = mix(mix(sweep, h), ok | failed << 1);
    if (!failed) break;
    points++;
  }
  EXPECT_GT(points, 10u);
  EXPECT_EQ(sweep, 0x840906004a2b2923ull) << std::hex << "0x" << sweep << " after " << std::dec << points;
}

TEST(LibraryIndexFormat, SpellingVoteAndUnknownAuthors) {
  fake::reset();
  bookMetadata.clear();
  const struct {
    const char* path;
    const char* author;
    const char* title;
  } books[] = {
      {"/a.epub", "Xun Lu", "One"},          {"/b.epub", "Lu Xun", "Two"},   {"/c.epub", "Lu Xun", "Three"},
      {"/d.epub", "Hugo Victor", "Four"},    {"/e.epub", "Victor Hugo", "Five"},
      {"/f.epub", "Unknown", "Six"},         {"/g.epub", "", "Seven"},
      {"/h.epub", "Émile Zola", "Eight"},    {"/i.epub", "Emile Zola", "Nine"},
      {"/j.epub", "???", "Ten"},
  };
  for (const auto& b : books) {
    fake::add(b.path);
    bookMetadata[b.path].author = b.author;
    bookMetadata[b.path].title = b.title;
  }
  BuildStats stats;
  ASSERT_TRUE(buildLibraryIndex("/", stats, true));
  const auto& bytes = fake::files.at(INDEX)->bytes;
  checkIndex(bytes);
  const Reader r{bytes};
  const uint32_t recordStart = r.u32(24), nameStart = r.u32(32);
  // Per title-ordered record: the shown author, title and source author from its blob.
  std::vector<std::string> shown, source, titles;
  for (uint16_t i = 0; i < r.u16(8); i++) {
    size_t p = nameStart + r.u32(recordStart + i * 128u) + 8 + bytes[recordStart + i * 128u + 12];
    std::string fields[3];
    for (auto& f : fields) {
      f.assign(bytes.begin() + p + 1, bytes.begin() + p + 1 + bytes[p]);
      p += 1 + bytes[p];
    }
    shown.push_back(fields[0]);
    titles.push_back(fields[1]);
    source.push_back(fields[2]);
  }
  const std::vector<std::string> expectedTitles = {"Eight", "Five", "Four", "Nine",  "One",
                                                   "Seven", "Six",  "Ten",  "Three", "Two"};
  EXPECT_EQ(titles, expectedTitles);
  // Two "Lu Xun" outvote one "Xun Lu"; "Emile Zola" and "Émile Zola" tie on count, so the
  // shorter wins; "Hugo Victor" and "Victor Hugo" tie on count and length, so the first
  // alphabetically wins; "Unknown" and "" stay without an author, and "???" (no author key either)
  // keeps its own spelling: books without a key are not voted on.
  const std::vector<std::string> expectedShown = {"Emile Zola", "Hugo Victor", "Hugo Victor", "Emile Zola", "Lu Xun",
                                                  "",           "",            "???",         "Lu Xun",     "Lu Xun"};
  EXPECT_EQ(shown, expectedShown);
  const std::vector<std::string> expectedSource = {"Émile Zola", "Victor Hugo", "Hugo Victor", "Emile Zola", "Xun Lu",
                                                   "",           "",            "???",         "Lu Xun",     "Lu Xun"};
  EXPECT_EQ(source, expectedSource);
  EXPECT_EQ(bytes[recordStart + 7 * 128u + 14], 0);  // "Ten": no author key
}
