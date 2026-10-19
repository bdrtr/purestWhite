// What the chapter-image PNG decoder hands PngToFramebufferConverter for the files in corpus/
// (make_corpus.py) and for each one cut short: the
// result of opening it (size, pixel type, bit depth, alpha), and, for files the converter goes on
// to decode, every row it receives (pixels, pixel type, bit depth, alpha, color key, the palette
// entries the row uses) and the result of the decode. Files are decoded only when the converter
// would decode them: a size, a supported bit depth and rows that fit the decoder's buffer.
// The reports hash to a value recorded from PNGdec 1.1.6, the decoder the firmware used. Rerun with
// PNG_CORPUS_PRINT=1 to list the cases. The hash freezes the corpus.
//
// Left out, because PNGdec's output there is not a behaviour to keep: changed compressed data, on
// which it can loop forever or overwrite its own state (a flipped IDAT bit in c0d1_33x17.png
// changed the row pitch mid-decode, another never returned), and files cut inside IDAT data, on
// which it waits forever for the rest.

#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <set>
#include <string>
#include <vector>

#include "PNGdec.h"

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

uint64_t fnv(const uint8_t* data, const size_t len) {
  uint64_t h = 0xCBF29CE484222325ULL;
  for (size_t i = 0; i < len; ++i) {
    h ^= data[i];
    h *= 0x100000001B3ULL;
  }
  return h;
}

// The converter's limits (PngToFramebufferConverter.cpp).
int bytesPerPixel(const int type) {
  switch (type) {
    case PNG_PIXEL_TRUECOLOR:
      return 3;
    case PNG_PIXEL_GRAY_ALPHA:
      return 2;
    case PNG_PIXEL_TRUECOLOR_ALPHA:
      return 4;
    default:
      return 1;
  }
}

bool converterWouldDecode(const int width, const int height, const int type, const int bpp) {
  if (width <= 0 || height <= 0) return false;
  int pitch = width * bytesPerPixel(type);
  if ((type == PNG_PIXEL_GRAYSCALE || type == PNG_PIXEL_INDEXED) && bpp < 8) pitch = (width * bpp + 7) / 8;
  if ((pitch + 1) * 2 + 32 > PNG_MAX_BUFFERED_PIXELS) return false;
  if (bpp == 8) return true;
  if (bpp != 1 && bpp != 2 && bpp != 4) return false;
  return type == PNG_PIXEL_GRAYSCALE || type == PNG_PIXEL_INDEXED;
}

uint8_t packedSample(const uint8_t* pixels, const int x, const int bpp) {
  if (bpp == 8) return pixels[x];
  const int bit = x * bpp;
  return (pixels[bit >> 3] >> (8 - bpp - (bit & 7))) & ((1 << bpp) - 1);
}

// The data source the firmware gives the decoder: a file it reads and seeks.
struct MemoryFile {
  const std::vector<uint8_t>* data;
  int32_t position = 0;
};

// The file being decoded: PNGdec's open callback takes only a name.
MemoryFile* currentFile = nullptr;

struct Decode {
  PNG* png;
  std::string* out;
};

int drawRow(PNGDRAW* draw) {
  auto* decode = static_cast<Decode*>(draw->pUser);
  std::string& out = *decode->out;
  char line[160];
  std::snprintf(line, sizeof(line), "row %d pitch %d type %d bpp %d alpha %d pixels %016llx", draw->y, draw->iPitch,
                draw->iPixelType, draw->iBpp, draw->iHasAlpha,
                static_cast<unsigned long long>(fnv(draw->pPixels, static_cast<size_t>(draw->iPitch))));
  out += line;
  if (draw->iHasAlpha && (draw->iPixelType == PNG_PIXEL_GRAYSCALE || draw->iPixelType == PNG_PIXEL_TRUECOLOR)) {
    std::snprintf(line, sizeof(line), " key %06x", decode->png->getTransparentColor());
    out += line;
  }
  if (draw->iPixelType == PNG_PIXEL_INDEXED) {
    std::set<int> used;
    for (int x = 0; x < draw->iWidth; ++x) used.insert(packedSample(draw->pPixels, x, draw->iBpp));
    for (const int i : used) {
      const uint8_t* p = draw->pPalette;
      std::snprintf(line, sizeof(line), " %d:%02x%02x%02x%02x", i, p[i * 3], p[i * 3 + 1], p[i * 3 + 2], p[768 + i]);
      out += line;
    }
  }
  out += '\n';
  return 1;
}

std::string report() {
  static PNG png;  // 42 KB: kept off the stack
  std::string out;
  const int rc = png.open(
      "file",
      [](const char*, int32_t* size) -> void* {
        *size = static_cast<int32_t>(currentFile->data->size());
        return currentFile;
      },
      [](void*) {},
      [](PNGFILE* f, uint8_t* buffer, int32_t len) -> int32_t {
        auto* m = static_cast<MemoryFile*>(f->fHandle);
        const int32_t available = static_cast<int32_t>(m->data->size()) - m->position;
        const int32_t n = std::max<int32_t>(0, std::min(len, available));
        std::memcpy(buffer, m->data->data() + m->position, static_cast<size_t>(n));
        m->position += n;
        f->iPos = m->position;
        return n;
      },
      [](PNGFILE* f, int32_t position) -> int32_t {
        auto* m = static_cast<MemoryFile*>(f->fHandle);
        m->position = position;
        f->iPos = position;
        return position;
      },
      drawRow);
  char line[160];
  std::snprintf(line, sizeof(line), "open %d %dx%d type %d bpp %d alpha %d\n", rc, png.getWidth(), png.getHeight(),
                png.getPixelType(), png.getBpp(), png.hasAlpha());
  out += line;
  if (rc == PNG_SUCCESS && converterWouldDecode(png.getWidth(), png.getHeight(), png.getPixelType(), png.getBpp())) {
    Decode decode{&png, &out};
    const int decoded = png.decode(&decode, 0);
    std::snprintf(line, sizeof(line), "decode %d\n", decoded);
    out += line;
  }
  png.close();
  return out;
}

// Whether `cut` falls inside some IDAT chunk's data, where PNGdec waits forever for the rest.
bool cutsImageData(const std::vector<uint8_t>& file, const size_t cut) {
  size_t p = 8;
  while (p + 8 <= file.size()) {
    const uint32_t len =
        (uint32_t{file[p]} << 24) | (uint32_t{file[p + 1]} << 16) | (uint32_t{file[p + 2]} << 8) | file[p + 3];
    if (std::memcmp(&file[p + 4], "IDAT", 4) == 0 && cut >= p + 8 && cut < p + 8 + len) return true;
    if (len > file.size()) break;
    p += 12 + len;
  }
  return false;
}

// Recorded from PNGdec 1.1.6.
constexpr uint64_t kHash = 0xE2AB4C01FF8C0CC6ULL;

}  // namespace

TEST(PngDecoderCorpus, RowsMatchTheRecordedDecoder) {
  std::vector<std::filesystem::path> paths;
  for (const auto& entry : std::filesystem::directory_iterator(PNG_CORPUS_DIR)) paths.push_back(entry.path());
  std::sort(paths.begin(), paths.end());
  ASSERT_GT(paths.size(), 100u);
  const bool print = std::getenv("PNG_CORPUS_PRINT") != nullptr;
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
      if (cut == 0 || cut >= file.size() || cutsImageData(file, cut)) continue;
      run(name + " cut " + std::to_string(cut), std::vector<uint8_t>(file.begin(), file.begin() + cut));
    }
  }
  EXPECT_GT(cases, 400u);
  EXPECT_EQ(hash.value, kHash) << "0x" << std::hex << std::uppercase << hash.value << "ULL";
}
