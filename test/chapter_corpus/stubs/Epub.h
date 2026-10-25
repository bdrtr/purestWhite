#pragma once

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

// Serves image headers by name, so the parser's image path runs: images/i0.jpg is 600x800,
// i1.jpg 40x30, i2.jpg 3000x400, c.png 300x200 and i3.jpg (or anything else) bytes no probe reads.
class Epub {
 public:
  template <typename Output>
  bool readItemContentsToStream(const std::string& href, Output& out, size_t, bool = false) const {
    const std::vector<uint8_t> bytes = contents(href);
    out.write(bytes.data(), bytes.size());
    return true;
  }

 private:
  static std::vector<uint8_t> jpeg(const int width, const int height) {
    return {0xFF,
            0xD8,
            0xFF,
            0xE0,
            0x00,
            0x04,
            0x00,
            0x00,
            0xFF,
            0xC0,
            0x00,
            0x11,
            0x08,
            static_cast<uint8_t>(height >> 8),
            static_cast<uint8_t>(height),
            static_cast<uint8_t>(width >> 8),
            static_cast<uint8_t>(width),
            0x03,
            0x01,
            0x22,
            0x00,
            0x02,
            0x11,
            0x01,
            0x03,
            0x11,
            0x01,
            0xFF,
            0xD9};
  }

  static std::vector<uint8_t> png(const int width, const int height) {
    std::vector<uint8_t> out = {0x89, 'P', 'N', 'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, 'I', 'H', 'D', 'R'};
    for (const int v : {width, height}) {
      for (int shift = 24; shift >= 0; shift -= 8) out.push_back(static_cast<uint8_t>(v >> shift));
    }
    for (const uint8_t b : {8, 0, 0, 0, 0, 0, 0, 0, 0}) out.push_back(b);
    return out;
  }

  static std::vector<uint8_t> contents(const std::string& href) {
    const auto endsWith = [&](const char* name) {
      const std::string suffix = name;
      return href.size() >= suffix.size() && href.compare(href.size() - suffix.size(), suffix.size(), suffix) == 0;
    };
    if (endsWith("images/i0.jpg")) return jpeg(600, 800);
    if (endsWith("images/i1.jpg")) return jpeg(40, 30);
    if (endsWith("images/i2.jpg")) return jpeg(3000, 400);
    if (endsWith("images/c.png")) return png(300, 200);
    return {'n', 'o', 't', ' ', 'a', 'n', ' ', 'i', 'm', 'a', 'g', 'e'};
  }
};
