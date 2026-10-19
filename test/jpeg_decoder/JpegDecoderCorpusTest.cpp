// What the JPEG decoder hands JpegToFramebufferConverter and JpegToBmpConverter for the files in
// corpus/ (make_corpus.py) and for each one cut short: the result of opening it (size, baseline or
// progressive, error), and, for files the converters go on to decode, every block it draws at each
// scale they ask for (position, size, the 8-bit gray pixels in use) and the result of the decode.
// Files are decoded only when the converters would decode them: a size within their limits.
// The reports hash to a value recorded from JPEGDEC (the commit platformio.ini pins, with the
// patches in scripts/jpegdec_patches), built as the firmware builds it. Rerun with
// JPEG_CORPUS_PRINT=1 to list the cases. The hash freezes the corpus.
//
// Left out, because JPEGDEC's output there is not a behaviour to keep: a sampling factor of 3, on
// which it divides by zero (a crash on the device too), and a frame of more than four components,
// on which it writes past its component table into the rest of its state. Its other undefined
// behaviour, shifts by a negative amount while it builds Huffman tables, comes out the same on the
// device (Xtensa and x86 both shift by the low 5 bits); reports from 32- and 64-bit builds match.

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

#include "JPEGDEC.h"

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

// The data source the firmware gives the decoder: a file it reads and seeks as HalFile does
// (a seek past the end fails).
struct MemoryFile {
  const std::vector<uint8_t>* data;
  int32_t position = 0;
};

// The file being decoded: JPEGDEC's open callback takes only a name.
MemoryFile* currentFile = nullptr;

void* openFile(const char*, int32_t* size) {
  currentFile->position = 0;
  *size = static_cast<int32_t>(currentFile->data->size());
  return currentFile;
}

void closeFile(void*) {}

// As the converters' callbacks: iPos follows every read and seek.
int32_t readFile(JPEGFILE* f, uint8_t* buffer, const int32_t len) {
  auto* m = static_cast<MemoryFile*>(f->fHandle);
  const int32_t available = static_cast<int32_t>(m->data->size()) - m->position;
  const int32_t n = std::max<int32_t>(0, std::min(len, available));
  if (n > 0) std::memcpy(buffer, m->data->data() + m->position, static_cast<size_t>(n));
  m->position += n;
  f->iPos += n;
  return n;
}

int32_t seekFile(JPEGFILE* f, const int32_t position) {
  auto* m = static_cast<MemoryFile*>(f->fHandle);
  if (position < 0 || position > static_cast<int32_t>(m->data->size())) return -1;
  m->position = position;
  f->iPos = position;
  return position;
}

int drawBlock(JPEGDRAW* draw) {
  std::string& out = *static_cast<std::string*>(draw->pUser);
  // The converters read iWidthUsed columns of each of iHeight rows, iWidth bytes apart.
  const auto* pixels = reinterpret_cast<const uint8_t*>(draw->pPixels);
  uint64_t h = 0xCBF29CE484222325ULL;
  for (int row = 0; row < draw->iHeight; ++row) {
    for (int x = 0; x < draw->iWidthUsed; ++x) {
      h ^= pixels[row * draw->iWidth + x];
      h *= 0x100000001B3ULL;
    }
  }
  char line[160];
  std::snprintf(line, sizeof(line), "block %d,%d %dx%d used %d bpp %d pixels %016llx\n", draw->x, draw->y, draw->iWidth,
                draw->iHeight, draw->iWidthUsed, draw->iBpp, static_cast<unsigned long long>(h));
  out += line;
  return 1;
}

std::string report() {
  static JPEGDEC jpeg;  // ~17 KB: kept off the stack
  std::string out;
  char line[160];
  // What getDimensions sees.
  int rc = jpeg.open("file", openFile, closeFile, readFile, seekFile, nullptr);
  std::snprintf(line, sizeof(line), "open %d error %d %dx%d type %d\n", rc, jpeg.getLastError(), jpeg.getWidth(),
                jpeg.getHeight(), jpeg.getJPEGType());
  out += line;
  const bool decodes = rc == 1 && convertersWouldDecode(jpeg.getWidth(), jpeg.getHeight());
  jpeg.close();
  if (!decodes) return out;
  // Each scale the framebuffer converter picks (the cover converter asks for 0).
  for (const int options : {0, JPEG_SCALE_HALF, JPEG_SCALE_QUARTER, JPEG_SCALE_EIGHTH}) {
    rc = jpeg.open("file", openFile, closeFile, readFile, seekFile, drawBlock);
    jpeg.setPixelType(EIGHT_BIT_GRAYSCALE);
    jpeg.setUserPointer(&out);
    const int decoded = rc == 1 ? jpeg.decode(0, 0, options) : -1;
    std::snprintf(line, sizeof(line), "decode %d open %d result %d error %d\n", options, rc, decoded,
                  jpeg.getLastError());
    out += line;
    jpeg.close();
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
    MemoryFile memory{&file};
    currentFile = &memory;
    const std::string r = report();
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
