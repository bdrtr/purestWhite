#pragma once

#include <HalStorage.h>

#include <cstddef>
#include <map>
#include <string>
#include <vector>

class CssParser;

// The parts of Epub that Section uses: a cache folder, spine items whose content is held in memory, and a TOC.
class Epub {
 public:
  struct SpineItem {
    std::string href;
  };
  struct TocItem {
    std::string anchor;
    int16_t spineIndex = -1;
  };

  std::string cachePath;
  std::string language = "en";
  std::vector<std::string> spine;  // href per spine index
  std::map<std::string, std::string> contents;
  std::vector<TocItem> toc;

  const std::string& getCachePath() const { return cachePath; }
  const std::string& getLanguage() const { return language; }
  SpineItem getSpineItem(const int index) const { return {spine.at(index)}; }
  TocItem getTocItem(const int index) const { return toc.at(index); }
  int getTocItemsCount() const { return static_cast<int>(toc.size()); }
  int getTocIndexForSpineIndex(const int spineIndex) const {
    for (size_t i = 0; i < toc.size(); ++i) {
      if (toc[i].spineIndex == spineIndex) return static_cast<int>(i);
    }
    return -1;
  }
  CssParser* getCssParser() const { return nullptr; }
  template <typename Output>
  bool readItemContentsToStream(const std::string& href, Output& out, size_t, bool = false) const {
    const auto found = contents.find(href);
    if (found == contents.end()) return false;
    return out.write(reinterpret_cast<const uint8_t*>(found->second.data()), found->second.size()) ==
           found->second.size();
  }
};
