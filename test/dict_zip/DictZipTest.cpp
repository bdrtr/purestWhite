// DictZip over a real dictzip file (DictZipFixture.h, from make_fixture.py): byte ranges inside one
// chunk, across chunks and to the end, and the cause it reports for a range outside the text, a
// damaged chunk, a cut chunk, a failing read and a failing write.

#include <gtest/gtest.h>

#include <string>
#include <vector>

#include "DictZipFixture.h"
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
