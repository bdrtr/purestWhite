#include <BuildScratch.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <array>
#include <cstdlib>
#include <cstring>
#include <vector>

namespace {
size_t allocationAttempts = 0;
size_t liveAllocations = 0;
size_t failOnAttempt = 0;

void* testMalloc(size_t size) {
  if (++allocationAttempts == failOnAttempt) return nullptr;
  void* result = std::malloc(size);
  if (result) ++liveAllocations;
  return result;
}

void testFree(void* ptr) {
  if (ptr) --liveAllocations;
  std::free(ptr);
}
}  // namespace

// Intercept only InflateStream's allocations, leaving the test runner untouched.
#define malloc testMalloc
#define free testFree
#include "../../lib/InflateStream/InflateStream.cpp"
#undef free
#undef malloc

namespace {
constexpr uint8_t COMPRESSED[] = {
    0xed, 0xc9, 0xb1, 0x09, 0xc0, 0x20, 0x10, 0x00, 0xc0, 0x3e, 0x53, 0xb8, 0x8b, 0x0b, 0x64, 0x05, 0x0b, 0x03, 0x42,
    0x50, 0x78, 0xbf, 0x71, 0xfb, 0x8c, 0x91, 0xe6, 0xae, 0xbd, 0x1a, 0x6b, 0xef, 0x7b, 0x8d, 0x99, 0x65, 0xcc, 0xe7,
    0x6d, 0xd9, 0x4b, 0xf4, 0x8c, 0x73, 0x55, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84,
    0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21,
    0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08,
    0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42,
    0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10,
    0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84,
    0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21,
    0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08,
    0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42,
    0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10,
    0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84,
    0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21,
    0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08, 0x21, 0x84, 0x10, 0x42, 0x08,
    0x21, 0xc4, 0xdf, 0xf1, 0x01};
constexpr char LINE[] = "CrossPoint inflate retry\n";
constexpr size_t OUTPUT_SIZE = (sizeof(LINE) - 1) * 4000;

class InflateStreamTest : public ::testing::Test {
 protected:
  alignas(8) inline static std::array<uint8_t, 48000> framebuffer{};

  void SetUp() override {
    allocationAttempts = 0;
    liveAllocations = 0;
    failOnAttempt = 0;
    framebuffer.fill(0);
  }
  void TearDown() override {
    EXPECT_EQ(liveAllocations, 0u);
    buildscratch::reclaim();
  }

  static uint8_t expected(const size_t offset) { return LINE[offset % (sizeof(LINE) - 1)]; }

  void verifyOutput(InflateStream& stream) {
    stream.setSource(COMPRESSED, sizeof(COMPRESSED));
    std::array<uint8_t, 127> output{};
    size_t total = 0;
    while (total < OUTPUT_SIZE) {
      const size_t wanted = std::min(output.size(), OUTPUT_SIZE - total);
      ASSERT_TRUE(stream.read(output.data(), wanted));
      for (size_t i = 0; i < wanted; ++i) {
        ASSERT_EQ(output[i], expected(total + i));
      }
      total += wanted;
    }
  }
};

TEST_F(InflateStreamTest, BorrowedFramebufferDecodesAcrossWindowWrapsWithoutHeap) {
  buildscratch::lend(framebuffer.data(), framebuffer.size());
  failOnAttempt = 1;
  for (int pass = 0; pass < 3; ++pass) {
    InflateStream stream;
    ASSERT_TRUE(stream.init(true));
    verifyOutput(stream);
  }
  EXPECT_EQ(allocationAttempts, 0u);
  EXPECT_EQ(buildscratch::claim(framebuffer.size()), framebuffer.data());
  buildscratch::release(framebuffer.data());
}

TEST_F(InflateStreamTest, WindowAllocationFailureCanRetry) {
  InflateStream stream;
  failOnAttempt = 1;
  EXPECT_FALSE(stream.init(true));
  EXPECT_EQ(liveAllocations, 0u);
  failOnAttempt = 0;
  ASSERT_TRUE(stream.init(true));
  verifyOutput(stream);
}

TEST_F(InflateStreamTest, StateAllocationFailureImmediatelyReleasesWindow) {
  InflateStream stream;
  failOnAttempt = 2;
  EXPECT_FALSE(stream.init(true));
  EXPECT_EQ(liveAllocations, 0u);
  failOnAttempt = 0;
  ASSERT_TRUE(stream.init(true));
  verifyOutput(stream);
}

TEST_F(InflateStreamTest, OccupiedScratchIsNotOverwrittenOnHeapFailure) {
  buildscratch::lend(framebuffer.data(), framebuffer.size());
  ASSERT_EQ(buildscratch::claim(framebuffer.size()), framebuffer.data());
  framebuffer.fill(0xa5);
  InflateStream stream;
  failOnAttempt = 1;
  EXPECT_FALSE(stream.init(true));
  EXPECT_EQ(framebuffer.front(), 0xa5);
  EXPECT_EQ(framebuffer.back(), 0xa5);
  buildscratch::release(framebuffer.data());
  ASSERT_TRUE(stream.init(true));
  verifyOutput(stream);
  EXPECT_EQ(allocationAttempts, 1u);
}

TEST_F(InflateStreamTest, ZlibWrappedStreamChecksItsTrailer) {
  // COMPRESSED inside a zlib header and the Adler-32 of its output, as in PNG.
  uint32_t a = 1, b = 0;
  for (size_t i = 0; i < OUTPUT_SIZE; ++i) {
    a = (a + expected(i)) % 65521;
    b = (b + a) % 65521;
  }
  const uint32_t adler = (b << 16) | a;
  std::array<uint8_t, sizeof(COMPRESSED) + 6> zlib{0x78, 0x9c};
  std::memcpy(zlib.data() + 2, COMPRESSED, sizeof(COMPRESSED));
  for (int i = 0; i < 4; ++i) zlib[zlib.size() - 1 - i] = static_cast<uint8_t>(adler >> (8 * i));

  for (const bool corrupt : {false, true}) {
    if (corrupt) zlib.back() ^= 1;
    InflateStream stream;
    ASSERT_TRUE(stream.init(true));
    stream.setZlibWrapped();
    stream.setSource(zlib.data(), zlib.size());
    std::vector<uint8_t> output(OUTPUT_SIZE + 1);
    size_t produced = 0;
    EXPECT_EQ(stream.readAtMost(output.data(), output.size(), &produced),
              corrupt ? InflateStream::Status::Error : InflateStream::Status::Done);
    // A bad checksum still hands out what was decoded.
    ASSERT_EQ(produced, OUTPUT_SIZE);
    for (size_t i = 0; i < produced; ++i) ASSERT_EQ(output[i], expected(i)) << i;
  }
}

TEST_F(InflateStreamTest, TruncatedStreamHandsOutWhatItDecoded) {
  for (const bool streaming : {true, false}) {
    InflateStream stream;
    ASSERT_TRUE(stream.init(streaming));
    stream.setSource(COMPRESSED, sizeof(COMPRESSED) / 2);
    std::vector<uint8_t> output(OUTPUT_SIZE);
    size_t produced = 0;
    EXPECT_EQ(stream.readAtMost(output.data(), output.size(), &produced), InflateStream::Status::Error);
    EXPECT_GT(produced, 0u);
    for (size_t i = 0; i < produced; ++i) ASSERT_EQ(output[i], expected(i)) << i;
  }
}

TEST_F(InflateStreamTest, OneShotRejectsOutputElsewhere) {
  InflateStream stream;
  ASSERT_TRUE(stream.init(false));
  stream.setSource(COMPRESSED, sizeof(COMPRESSED));
  std::vector<uint8_t> output(OUTPUT_SIZE);
  ASSERT_TRUE(stream.read(output.data(), 100));
  // Back-references need the earlier output right in front of dest.
  std::array<uint8_t, 100> elsewhere{};
  EXPECT_FALSE(stream.read(elsewhere.data(), elsewhere.size()));
}
}  // namespace
