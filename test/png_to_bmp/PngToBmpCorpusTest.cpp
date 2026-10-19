// What the cover converter (lib/PngToBmpConverter) writes for the PNG files in
// test/png_decoder/corpus and corpus/ (make_corpus.py), and for each one cut short: the result and
// the BMP bytes of each of its entry points, at a thumbnail size (1-bit), a small size (2-bit),
// the display's size cropped and fitted (2-bit, both thresholds), and the image's own size. The
// reports hash to a value recorded from the converter's own PNG decoder (before bembeyaz-png).
// Rerun with PNG_TO_BMP_CORPUS_PRINT=1 to list the cases. The hash freezes the corpus.
//
// Left out: outputs over 4 million pixels (a 1x3072 image cropped to the display is 1.4 million
// rows), which only take time.

#include <HalStorage.h>
#include <PngToBmpConverter.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iterator>
#include <string>
#include <vector>

namespace {

struct Hash {
  uint64_t value = 0xCBF29CE484222325ULL;  // FNV-1a
  void add(const uint8_t* data, const size_t len) {
    for (size_t i = 0; i < len; ++i) {
      value ^= data[i];
      value *= 0x100000001B3ULL;
    }
  }
  void add(const std::string& s) {
    add(reinterpret_cast<const uint8_t*>(s.data()), s.size());
    value ^= 0xFF;
    value *= 0x100000001B3ULL;
  }
};

// The BMP the converter writes, hashed as it goes.
class BmpSink : public Print {
 public:
  Hash hash;
  size_t bytes = 0;
  size_t write(const uint8_t data) override { return write(&data, 1); }
  size_t write(const uint8_t* data, const size_t size) override {
    hash.add(data, size);
    bytes += size;
    return size;
  }
};

uint32_t be32(const std::vector<uint8_t>& file, const size_t at) {
  return (uint32_t{file[at]} << 24) | (uint32_t{file[at + 1]} << 16) | (uint32_t{file[at + 2]} << 8) | file[at + 3];
}

// The output size the converter picks (PngToBmpConverter.cpp), when the header gets that far.
bool outputTooBig(const std::vector<uint8_t>& file, const int targetWidth, const int targetHeight, const bool crop) {
  if (file.size() < 24 || std::memcmp(&file[12], "IHDR", 4) != 0) return false;
  const uint32_t width = be32(file, 16);
  const uint32_t height = be32(file, 20);
  if (width == 0 || height == 0 || width > 2048 || height > 3072) return false;
  if (targetWidth <= 0 || targetHeight <= 0) return uint64_t{width} * height > 4000000;
  const float fitWidth = static_cast<float>(targetWidth) / width;
  const float fitHeight = static_cast<float>(targetHeight) / height;
  const float scale = crop ? std::max(fitWidth, fitHeight) : std::min(fitWidth, fitHeight);
  const int64_t outWidth = std::max(1, static_cast<int>(width * scale));
  const int64_t outHeight = std::max(1, static_cast<int>(height * scale));
  return outWidth * outHeight > 4000000;
}

struct Variant {
  const char* name;
  int width;
  int height;
  bool crop;
  std::function<bool(HalFile&, Print&)> convert;
};

const Variant kVariants[] = {
    {"thumb-1bit", 61, 37, true,
     [](HalFile& f, Print& out) { return PngToBmpConverter::pngFileTo1BitBmpStreamWithSize(f, out, 61, 37); }},
    {"small", 45, 70, true,
     [](HalFile& f, Print& out) { return PngToBmpConverter::pngFileToBmpStreamWithSize(f, out, 45, 70); }},
    {"cover-crop", 480, 800, true,
     [](HalFile& f, Print& out) { return PngToBmpConverter::pngFileToBmpStream(f, out, true, false); }},
    {"cover-fit-original", 480, 800, false,
     [](HalFile& f, Print& out) { return PngToBmpConverter::pngFileToBmpStream(f, out, false, true); }},
    {"own-size", 0, 0, true,
     [](HalFile& f, Print& out) { return PngToBmpConverter::pngFileToBmpStreamWithSize(f, out, 0, 0); }},
};

std::string report(const std::vector<uint8_t>& file) {
  std::string out;
  for (const Variant& v : kVariants) {
    char line[160];
    if (outputTooBig(file, v.width, v.height, v.crop)) {
      std::snprintf(line, sizeof(line), "%s skipped\n", v.name);
    } else {
      HalFile in(file);
      BmpSink bmp;
      const bool ok = v.convert(in, bmp);
      std::snprintf(line, sizeof(line), "%s %d bytes %zu bmp %016llx\n", v.name, ok, bmp.bytes,
                    static_cast<unsigned long long>(bmp.hash.value));
    }
    out += line;
  }
  return out;
}

// Recorded from the converter's own PNG decoder.
constexpr uint64_t kHash = 0x5B5A8D75FADCAF8DULL;

}  // namespace

TEST(PngToBmpCorpus, BmpsMatchTheRecordedConverter) {
  std::vector<std::filesystem::path> paths;
  for (const char* dir : {PNG_CORPUS_DIR, PNG_TO_BMP_CORPUS_DIR}) {
    for (const auto& entry : std::filesystem::directory_iterator(dir)) paths.push_back(entry.path());
  }
  std::sort(paths.begin(), paths.end(),
            [](const auto& a, const auto& b) { return a.filename().string() < b.filename().string(); });
  ASSERT_GT(paths.size(), 160u);
  const bool print = std::getenv("PNG_TO_BMP_CORPUS_PRINT") != nullptr;
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
    // Cut short, anywhere: the converter stops when the data ends.
    for (const size_t cut : {file.size() / 3, file.size() / 2, file.size() * 9 / 10, file.size() - 1}) {
      if (cut == 0 || cut >= file.size()) continue;
      run(name + " cut " + std::to_string(cut), std::vector<uint8_t>(file.begin(), file.begin() + cut));
    }
  }
  EXPECT_GT(cases, 600u);
  EXPECT_EQ(hash.value, kHash) << "0x" << std::hex << std::uppercase << hash.value << "ULL";
}
