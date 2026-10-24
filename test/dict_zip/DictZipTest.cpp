// DictZip over a real dictzip file (DictZipFixture.h, from make_fixture.py): byte ranges inside one
// chunk, across chunks and to the end, and the cause it reports for a range outside the text, a
// damaged chunk, a cut chunk, a failing read and a failing write. Then bembeyaz-dict's dictzip
// header and chunk plan against the code they replaced (OldDictZip.h), over random headers.

#include <bembeyaz/dict.h>
#include <gtest/gtest.h>

#include <string>
#include <vector>

#include "DictZipFixture.h"
#include "OldDictZip.h"
#include "util/DictZip.h"

namespace {

// The chunk table starts after the gzip header (10), XLEN (2), "RA" and its length (4) and the
// version, chunk length and count (6): two bytes per chunk.
constexpr size_t kChunkSizeAt = 22;

class DictZipTest : public ::testing::Test {
 protected:
  void SetUp() override { dictZipFiles = {std::vector<uint8_t>(std::begin(kDictZip), std::end(kDictZip))}; }

  // The uncompressed range [offset, offset + size), or the error extractEntry reported.
  std::string extract(const uint32_t offset, const uint32_t size, DictZip::ExtractError* error = nullptr) {
    HalFile out;
    DictZip::ExtractError e = DictZip::ExtractError::None;
    const bool ok = DictZip::extractEntry("/dict.dz", offset, size, out, &e);
    if (error) *error = e;
    EXPECT_EQ(ok, e == DictZip::ExtractError::None);
    return std::string(out.written.begin(), out.written.end());
  }

  static std::string text(const uint32_t offset, const uint32_t size) {
    return std::string(reinterpret_cast<const char*>(kDictText) + offset, size);
  }
};

TEST_F(DictZipTest, ParsesTheChunkTable) {
  HalFile file;
  ASSERT_TRUE(Storage.openFileForRead("", "", file));
  DictZip::Info info;
  ASSERT_TRUE(DictZip::parse(file, &info));
  EXPECT_EQ(info.chunkLength, kChunkLength);
  EXPECT_EQ(info.totalSize, sizeof(kDictText));
  EXPECT_EQ(info.dataOffset, kChunkFileOffsets[0]);
  ASSERT_EQ(info.chunkOffsets.size(), kChunkCount + 1);
  for (size_t i = 0; i <= kChunkCount; ++i) {
    EXPECT_EQ(info.dataOffset + info.chunkOffsets[i], kChunkFileOffsets[i]) << "chunk " << i;
  }
}

TEST_F(DictZipTest, ExtractsRangesInsideAndAcrossChunks) {
  const uint32_t total = sizeof(kDictText);
  const struct {
    uint32_t offset, size;
  } ranges[] = {
      {0, 1},                                         // the first byte
      {0, 512},                                       // one read batch
      {100, 513},                                     // a batch and one byte, after a discard
      {kChunkLength - 1, 2},                          // across the first boundary
      {kChunkLength, kChunkLength},                   // exactly the second chunk
      {kChunkLength - 700, 2 * kChunkLength + 1400},  // three chunks
      {total - 5, 5},                                 // the end of the short last chunk
      {0, total},                                     // everything
  };
  for (const auto& r : ranges) {
    EXPECT_EQ(extract(r.offset, r.size), text(r.offset, r.size)) << r.offset << "+" << r.size;
  }
}

TEST_F(DictZipTest, ARangeOutsideTheTextIsAReadError) {
  DictZip::ExtractError error;
  EXPECT_EQ(extract(sizeof(kDictText), 1, &error), "");
  EXPECT_EQ(error, DictZip::ExtractError::ReadError);
  EXPECT_EQ(extract(10, sizeof(kDictText), &error), "");
  EXPECT_EQ(error, DictZip::ExtractError::ReadError);
}

TEST_F(DictZipTest, ADamagedChunkIsADecompressError) {
  // BTYPE 11 is reserved: the second chunk's first block cannot be decoded.
  dictZipFiles.dz[kChunkFileOffsets[1]] = 0x07;
  DictZip::ExtractError error;
  extract(kChunkLength + 10, 20, &error);
  EXPECT_EQ(error, DictZip::ExtractError::Decompress);
  EXPECT_EQ(extract(10, 20), text(10, 20)) << "the other chunks still read";
}

TEST_F(DictZipTest, DamageAfterTheRangeDoesNotSpoilIt) {
  // The first chunk rebuilt as a stored block of its first 1000 bytes that is not the last, then a
  // block of the reserved type 11. The decoder meets the damage while decoding the range before it.
  constexpr uint16_t kStored = 1000;
  std::vector<uint8_t> chunk = {0x00, kStored & 0xFF, kStored >> 8, static_cast<uint8_t>(~kStored & 0xFF),
                                static_cast<uint8_t>(~kStored >> 8)};
  chunk.insert(chunk.end(), kDictText, kDictText + kStored);
  chunk.push_back(0x07);
  std::vector<uint8_t>& dz = dictZipFiles.dz;
  dz.erase(dz.begin() + kChunkFileOffsets[0], dz.begin() + kChunkFileOffsets[1]);
  dz.insert(dz.begin() + kChunkFileOffsets[0], chunk.begin(), chunk.end());
  dz[kChunkSizeAt] = chunk.size() & 0xFF;
  dz[kChunkSizeAt + 1] = chunk.size() >> 8;

  EXPECT_EQ(extract(10, 20), text(10, 20));
  DictZip::ExtractError error;
  extract(kStored - 5, 10, &error);
  EXPECT_EQ(error, DictZip::ExtractError::Decompress);
}

TEST_F(DictZipTest, ACutChunkIsADecompressError) {
  // The table gives the first chunk 40 bytes: its stream stops long before its 3000 bytes.
  dictZipFiles.dz[kChunkSizeAt] = 40;
  dictZipFiles.dz[kChunkSizeAt + 1] = 0;
  DictZip::ExtractError error;
  extract(kChunkLength - 10, 10, &error);
  EXPECT_EQ(error, DictZip::ExtractError::Decompress);
}

TEST_F(DictZipTest, AFailingReadIsAReadError) {
  // The reads of the header and table succeed; the second chunk's data fails half way.
  dictZipFiles.failReadsFrom = (kChunkFileOffsets[1] + kChunkFileOffsets[2]) / 2;
  DictZip::ExtractError error;
  extract(kChunkLength, kChunkLength, &error);
  EXPECT_EQ(error, DictZip::ExtractError::ReadError);
}

// ReadError covers the output file too (DictZip.h).
TEST_F(DictZipTest, AFailingWriteIsAReadError) {
  dictZipFiles.failWrites = true;
  DictZip::ExtractError error;
  extract(0, 10, &error);
  EXPECT_EQ(error, DictZip::ExtractError::ReadError);
}

}  // namespace

namespace {

uint32_t seed;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}

void le16(std::vector<uint8_t>& out, const uint32_t v) {
  out.push_back(static_cast<uint8_t>(v));
  out.push_back(static_cast<uint8_t>(v >> 8));
}

// A dictzip-like file: a gzip header with FEXTRA subfields (RA tables and others, with lengths
// that may not add up), optional name, comment and header CRC, data and the trailer; sometimes cut.
std::vector<uint8_t> randomDz() {
  std::vector<uint8_t> f = {0x1f, 0x8b, 8};
  if (rnd() % 20 == 0) f[rnd() % 3] ^= static_cast<uint8_t>(1 + rnd() % 255);
  const uint8_t flags = static_cast<uint8_t>((rnd() % 10 ? 0x04 : 0) | (rnd() % 4 ? 0 : 0x08) | (rnd() % 4 ? 0 : 0x10) |
                                             (rnd() % 4 ? 0 : 0x02) | (rnd() % 30 ? 0 : 0xE1));
  f.push_back(flags);
  for (int i = 0; i < 6; ++i) f.push_back(static_cast<uint8_t>(rnd()));
  std::vector<uint8_t> extra;
  uint32_t chunks = 0;
  for (int n = rnd() % 3 + (rnd() % 3 == 0 ? 0 : 1); n > 0; --n) {
    if (rnd() % 3 == 0) {
      extra.push_back('X');
      extra.push_back('Y');
      const uint32_t len = rnd() % 9;
      le16(extra, len);
      for (uint32_t i = 0; i < len; ++i) extra.push_back(static_cast<uint8_t>(rnd()));
      continue;
    }
    extra.push_back('R');
    extra.push_back('A');
    const uint32_t count = rnd() % 25 == 0 ? (rnd() % 2 ? 0 : 8193 + rnd() % 10) : 1 + rnd() % 20;
    const uint32_t listed = rnd() % 15 == 0 ? rnd() % 8 : count;
    le16(extra, rnd() % 10 == 0 ? rnd() % 6 : 6 + 2 * listed);
    le16(extra, rnd() % 15 == 0 ? rnd() % 3 : 1);
    le16(extra, rnd() % 20 == 0 ? 0 : 1 + rnd() % 2000);
    le16(extra, count);
    // Tables past the 8192-chunk cap are written whole, so that only the cap refuses them.
    for (uint32_t i = 0; i < listed && (i < 30 || listed > 8192); ++i) le16(extra, 1 + rnd() % 300);
    chunks = listed;
  }
  le16(f, rnd() % 12 == 0 ? rnd() % (extra.size() + 4) : extra.size());
  f.insert(f.end(), extra.begin(), extra.end());
  if (flags & 0x08) f.insert(f.end(), {'n', 'a', 'm', 'e', 0});
  if (flags & 0x10) f.insert(f.end(), {'c', 0});
  if (flags & 0x02) f.insert(f.end(), {1, 2});
  for (uint32_t i = rnd() % (chunks * 200 + 10); i > 0; --i) f.push_back(static_cast<uint8_t>(rnd()));
  for (int i = 0; i < 4; ++i) f.push_back(static_cast<uint8_t>(rnd()));
  const uint32_t isize = rnd() % 10 == 0 ? 0 : rnd() % (chunks * 2000 + 10);
  for (int i = 0; i < 4; ++i) f.push_back(static_cast<uint8_t>(isize >> (8 * i)));
  if (rnd() % 8 == 0) f.resize(rnd() % (f.size() + 1));
  return f;
}

// A plan's slices, and the slice (from 1) that fails with `error`, 0 for none.
struct Plan {
  std::vector<uint32_t> slices;
  uint32_t failAt = 0;
  DictZip::ExtractError error = DictZip::ExtractError::None;
};

}  // namespace

TEST(DictZipOracle, MatchesTheOldParserAndChunkPlan) {
  seed = 61;
  unsigned diffs = 0;
  for (int c = 0; c < 20000; ++c) {
    dictZipFiles = {randomDz()};
    if (rnd() % 10 == 0) dictZipFiles.failReadsFrom = rnd() % (dictZipFiles.dz.size() + 1);
    // A heap too fragmented for the chunk table, sometimes.
    ESP.maxAllocHeap = rnd() % 10 == 0 ? rnd() % 1200 : 200 * 1024;
    HalFile a, b;
    a.seekSet(0);
    b.seekSet(0);
    DictZip::Info infoA, infoB;
    DictZip::ExtractError errA = DictZip::ExtractError::None, errB = DictZip::ExtractError::None;
    const bool okA = oldDictZip::parse(a, &infoA, &errA);
    const bool okB = DictZip::parse(b, &infoB, &errB);
    bool same = okA == okB && errA == errB;
    if (same && okA) {
      same = infoA.dataOffset == infoB.dataOffset && infoA.totalSize == infoB.totalSize &&
             infoA.chunkLength == infoB.chunkLength && infoA.chunkOffsets == infoB.chunkOffsets;
    }
    // Chunk plans for ranges in and around the text, some cut short by a failing slice.
    for (int r = 0; same && okA && r < 20; ++r) {
      const uint32_t offset = rnd() % (infoA.totalSize + 20);
      const uint32_t size = rnd() % 4 == 0 ? rnd() : rnd() % 5000;
      const uint32_t failAt = rnd() % 3 == 0 ? 1 + rnd() % 4 : 0;
      static constexpr DictZip::ExtractError KINDS[] = {
          DictZip::ExtractError::ReadError, DictZip::ExtractError::Decompress, DictZip::ExtractError::LowMemory};
      const DictZip::ExtractError kind = KINDS[rnd() % 3];
      Plan pa{{}, failAt, kind}, pb{{}, failAt, kind};
      DictZip::ExtractError ea = DictZip::ExtractError::None;
      const bool oka =
          oldDictZip::plan(infoA, offset, size, &ea,
                           [&pa](uint32_t o, uint32_t s, uint32_t skip, uint32_t take, DictZip::ExtractError* e) {
                             pa.slices.insert(pa.slices.end(), {o, s, skip, take});
                             if (pa.slices.size() / 4 == pa.failAt) {
                               *e = pa.error;
                               return false;
                             }
                             return true;
                           });
      const bb_dictzip_header header{infoB.dataOffset, infoB.totalSize, infoB.chunkLength};
      const uint8_t rb = bb_dictzip_slices(
          &header, infoB.chunkOffsets.data(), infoB.chunkOffsets.size(), offset, size,
          [](void* ctx, uint32_t o, uint32_t s, uint32_t skip, uint32_t take) -> uint8_t {
            auto& p = *static_cast<Plan*>(ctx);
            p.slices.insert(p.slices.end(), {o, s, skip, take});
            if (p.slices.size() / 4 != p.failAt) return BB_DICTZIP_OK;
            return p.error == DictZip::ExtractError::ReadError   ? BB_DICTZIP_READ
                   : p.error == DictZip::ExtractError::LowMemory ? BB_DICTZIP_MEMORY
                                                                 : BB_DICTZIP_CORRUPT;
          },
          &pb);
      const DictZip::ExtractError eb = rb == BB_DICTZIP_OK       ? DictZip::ExtractError::None
                                       : rb == BB_DICTZIP_READ   ? DictZip::ExtractError::ReadError
                                       : rb == BB_DICTZIP_MEMORY ? DictZip::ExtractError::LowMemory
                                                                 : DictZip::ExtractError::Decompress;
      same = oka == (rb == BB_DICTZIP_OK) && ea == eb && pa.slices == pb.slices;
    }
    if (!same) {
      ADD_FAILURE() << "case " << c << " size " << dictZipFiles.dz.size() << " old " << okA << "/"
                    << static_cast<int>(errA) << " new " << okB << "/" << static_cast<int>(errB);
      if (++diffs == 5) break;
    }
  }
  ESP.maxAllocHeap = 200 * 1024;
}
