// What the JPEG decoder hands JpegToFramebufferConverter and JpegToBmpConverter for the files in
// corpus/ (make_corpus.py) and for each one cut short: the result of opening it (size, baseline or
// progressive, error), and, for files the converters go on to decode, every block it draws at each
// scale they ask for (position, size, the 8-bit gray pixels in use) and the result of the decode.
// Files are decoded only when the converters would decode them: a size within their limits.
// The reports hash to a value recorded from JPEGDEC (the commit platformio.ini pinned, with
// CrossPoint's three patches, built as the firmware built it: a 32-bit bit buffer, no unaligned
// reads, no SIMD), the decoder the firmware used before bembeyaz-jpeg. Rerun with
// JPEG_CORPUS_PRINT=1 to list the cases. The hash freezes the corpus.
//
// Left out, because JPEGDEC's output there is not a behaviour to keep: a sampling factor of 3, on
// which it divided by zero (a crash on the device too), and a frame of more than four components,
// on which it wrote past its component table into the rest of its state. bembeyaz-jpeg's own tests
// cover those. Its other undefined behaviour, shifts by a negative amount while it built Huffman
// tables, came out the same on the device (Xtensa and x86 both shift by the low 5 bits); reports
// from 32- and 64-bit builds of it matched.

#include <bembeyaz/jpeg.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>
#include <vector>

namespace {

struct Hash {
  uint64_t value = 0xCBF29CE484222325ULL;  // FNV-1a
  void add(const std::string& s) {
    for (const char c : s) {
      value ^= static_cast<uint8_t>(c);
      value *= 0x100000001B3ULL;
    }
    value ^= 0xFF;
    value *= 0x100000001B3ULL;
  }
};

// The converters' limits (ImageToFramebufferDecoder.h, JpegToBmpConverter.cpp): the wider ones.
bool convertersWouldDecode(const int width, const int height) {
  return width > 0 && height > 0 && width <= INT16_MAX && height <= INT16_MAX &&
         static_cast<int64_t>(width) * height <= 8388608;
}

// The data source the firmware gives the decoder: a file it reads at a position.
size_t readAt(void* context, const uint32_t pos, uint8_t* buf, const size_t len) {
  const auto& data = *static_cast<const std::vector<uint8_t>*>(context);
  if (pos >= data.size()) return 0;
  const size_t n = std::min(len, data.size() - pos);
  std::memcpy(buf, data.data() + pos, n);
  return n;
}

int drawBlock(void* user, const bb_jpeg_block* block) {
  std::string& out = *static_cast<std::string*>(user);
  // The converters read width_used columns of each of height rows, width bytes apart.
  uint64_t h = 0xCBF29CE484222325ULL;
  for (int row = 0; row < block->height; ++row) {
    for (int x = 0; x < block->width_used; ++x) {
      h ^= block->pixels[row * block->width + x];
      h *= 0x100000001B3ULL;
    }
  }
  char line[160];
  std::snprintf(line, sizeof(line), "block %d,%d %dx%d used %d bpp %d pixels %016llx\n", block->x, block->y,
                block->width, block->height, block->width_used, 8, static_cast<unsigned long long>(h));
  out += line;
  return 1;
}

std::string report(const std::vector<uint8_t>& file) {
  std::string out;
  char line[160];
  void* context = const_cast<std::vector<uint8_t>*>(&file);
  const auto size = static_cast<int32_t>(file.size());
  // What getDimensions sees, in JPEGDEC's terms: open's 1 or 0 and the error.
  int status = 0;
  bb_jpeg* jpeg = bb_jpeg_open(readAt, context, size, &status);
  if (!jpeg) return "out of memory\n";
  bb_jpeg_info info;
  bb_jpeg_get_info(jpeg, &info);
  std::snprintf(line, sizeof(line), "open %d error %d %dx%d type %d\n", status == BB_JPEG_SUCCESS, status, info.width,
                info.height, info.progressive ? 1 : 0);
  out += line;
  const bool decodes = status == BB_JPEG_SUCCESS && convertersWouldDecode(info.width, info.height);
  bb_jpeg_free(jpeg);
  if (!decodes) return out;
  // Each scale the framebuffer converter picks (the cover converter asks for 0).
  const int scales[] = {0, BB_JPEG_SCALE_HALF, BB_JPEG_SCALE_QUARTER, BB_JPEG_SCALE_EIGHTH};
  for (const int options : scales) {
    jpeg = bb_jpeg_open(readAt, context, size, &status);
    if (!jpeg) return out + "out of memory\n";
    const bool opened = status == BB_JPEG_SUCCESS;
    if (opened) status = bb_jpeg_decode(jpeg, options, drawBlock, &out);
    std::snprintf(line, sizeof(line), "decode %d open %d result %d error %d\n", options, opened,
                  opened ? status == BB_JPEG_SUCCESS : -1, status);
    out += line;
    bb_jpeg_free(jpeg);
  }
  return out;
}

// Recorded from JPEGDEC.
constexpr uint64_t kHash = 0x8D9C017533611BF5ULL;

}  // namespace

TEST(JpegDecoderCorpus, BlocksMatchTheRecordedDecoder) {
  std::vector<std::filesystem::path> paths;
  for (const auto& entry : std::filesystem::directory_iterator(JPEG_CORPUS_DIR)) paths.push_back(entry.path());
  std::sort(paths.begin(), paths.end());
  ASSERT_GT(paths.size(), 90u);
  const bool print = std::getenv("JPEG_CORPUS_PRINT") != nullptr;
  Hash hash;
  size_t cases = 0;
  const auto run = [&](const std::string& name, const std::vector<uint8_t>& file) {
    const std::string r = report(file);
    hash.add(name);
    hash.add(r);
    ++cases;
    if (print) std::printf("--- %s\n%s", name.c_str(), r.c_str());
  };
  for (const auto& path : paths) {
    std::ifstream in(path, std::ios::binary);
    const std::vector<uint8_t> file((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    const std::string name = path.filename().string();
    run(name, file);
    // Cut short.
    for (const size_t cut : {file.size() / 3, file.size() / 2, file.size() * 9 / 10, file.size() - 1}) {
      if (cut == 0 || cut >= file.size()) continue;
      run(name + " cut " + std::to_string(cut), std::vector<uint8_t>(file.begin(), file.begin() + cut));
    }
  }
  EXPECT_GT(cases, 300u);
  EXPECT_EQ(hash.value, kHash) << "0x" << std::hex << std::uppercase << hash.value << "ULL";
}
