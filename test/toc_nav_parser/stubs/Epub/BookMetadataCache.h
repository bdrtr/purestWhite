#pragma once

#include <cstdint>
#include <string>
#include <vector>

class BookMetadataCache {
 public:
  struct TocEntry {
    std::string title;
    std::string href;
    std::string anchor;
    uint8_t level;
  };
  std::vector<TocEntry> toc;

  void createTocEntry(const std::string& title, const std::string& href, const std::string& anchor,
                      const uint8_t level) {
    toc.push_back({title, href, anchor, level});
  }
};
