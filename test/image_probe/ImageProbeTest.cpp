// bembeyaz-image's size probe against ImageDimsProbe's old parser (OldImageDimsProbe.h): random
// JPEG and PNG headers, some damaged, fed in random pieces, must take the same bytes and find the
// same size.
#include <bembeyaz/image.h>
#include <gtest/gtest.h>

#include <cstdint>
#include <vector>

#include "OldImageDimsProbe.h"

namespace {

uint32_t seed;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}
int range(int lo, int hi) { return lo + static_cast<int>(rnd() % static_cast<uint32_t>(hi - lo + 1)); }

void put16(std::vector<uint8_t>& out, const int v) {
  out.push_back(static_cast<uint8_t>(v >> 8));
  out.push_back(static_cast<uint8_t>(v));
}

// A JPEG's first segments: SOI, then markers of every kind, then a frame header or a scan.
std::vector<uint8_t> jpeg() {
  std::vector<uint8_t> out = {0xFF, 0xD8};
  for (int n = range(0, 6); n > 0; --n) {
    out.push_back(0xFF);
    for (int fill = rnd() % 3; fill > 0; --fill) out.push_back(0xFF);
    switch (rnd() % 4) {
      case 0:  // standalone: TEM, RSTn, SOI
        out.push_back(rnd() % 2 ? 0x01 : static_cast<uint8_t>(0xD0 + rnd() % 9));
        break;
      default: {  // a segment: APPn, DQT, DHT, JPG, DAC, COM, or anything
        static constexpr uint8_t MARKERS[] = {0xE0, 0xE1, 0xDB, 0xC4, 0xC8, 0xCC, 0xFE, 0xDD};
        out.push_back(rnd() % 8 ? MARKERS[rnd() % 8] : static_cast<uint8_t>(rnd()));
        const int len = rnd() % 6 ? range(2, 300) : range(0, 2);
        put16(out, len);
        for (int i = 2; i < len; ++i) out.push_back(static_cast<uint8_t>(rnd()));
      }
    }
  }
  out.push_back(0xFF);
  static constexpr uint8_t ENDS[] = {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xCF, 0xDA, 0xD9};
  out.push_back(ENDS[rnd() % 8]);
  put16(out, rnd() % 5 ? 17 : range(0, 8));
  for (int i = 0; i < 15; ++i) out.push_back(static_cast<uint8_t>(rnd()));
  return out;
}

std::vector<uint8_t> png() {
  std::vector<uint8_t> out = {0x89, 'P', 'N', 'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, 'I', 'H', 'D', 'R'};
  for (int i = 0; i < 8; ++i) out.push_back(static_cast<uint8_t>(rnd() % 3 ? rnd() % 4 : rnd()));
  for (int i = 0; i < 20; ++i) out.push_back(static_cast<uint8_t>(rnd()));
  return out;
}

}  // namespace

TEST(ImageProbe, MatchesTheOldParser) {
  seed = 43;
  unsigned diffs = 0;
  for (int c = 0; c < 20000; ++c) {
    std::vector<uint8_t> bytes;
    switch (rnd() % 5) {
      case 0:
      case 1:
        bytes = jpeg();
        break;
      case 2:
      case 3:
        bytes = png();
        break;
      default:
        for (int i = range(0, 40); i > 0; --i) bytes.push_back(static_cast<uint8_t>(rnd()));
    }
    // Damage some.
    if (!bytes.empty() && rnd() % 4 == 0) {
      for (int n = range(1, 3); n > 0; --n) bytes[rnd() % bytes.size()] = static_cast<uint8_t>(rnd());
    }

    oldProbe::ImageDimsProbe o;
    bb_image_probe p;
    bb_image_probe_init(&p);
    bool same = true;
    size_t at = 0;
    while (at < bytes.size() && same) {
      const size_t len = std::min(bytes.size() - at, static_cast<size_t>(range(1, 40)));
      const size_t a = o.write(bytes.data() + at, len);
      const size_t b = bb_image_probe_feed(&p, bytes.data() + at, len);
      same = a == b;
      if (a < len) break;
      at += len;
    }
    // One more piece after settling takes nothing either way.
    same = same && o.write(bytes.data(), bytes.size()) == bb_image_probe_feed(&p, bytes.data(), bytes.size());
    uint32_t w = 0, h = 0;
    const bool found = bb_image_probe_size(&p, &w, &h);
    const bool oldFound = o.state == oldProbe::ImageDimsProbe::State::Done;
    if (!same || found != oldFound || (found && (w != o.width || h != o.height))) {
      ADD_FAILURE() << "case " << c << " size " << bytes.size() << (same ? "" : " taken") << " found " << found
                    << " old " << oldFound;
      if (++diffs == 5) return;
    }
  }
}
