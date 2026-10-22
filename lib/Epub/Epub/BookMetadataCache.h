#pragma once

#include <HalStorage.h>
#include <bembeyaz/book.h>

#include <memory>
#include <string>
#include <vector>

namespace serialization {
class BufferedFileWriter;
}

class BookMetadataCache {
 public:
  struct BookMetadata {
    std::string title;
    std::string author;
    std::string language;
    std::string coverItemHref;
    std::string textReferenceHref;
  };

  struct SpineEntry {
    std::string href;
    uint32_t cumulativeSize;
    int16_t tocIndex;

    SpineEntry() : cumulativeSize(0), tocIndex(-1) {}
    SpineEntry(std::string href, const uint32_t cumulativeSize, const int16_t tocIndex)
        : href(std::move(href)), cumulativeSize(cumulativeSize), tocIndex(tocIndex) {}
  };

  struct TocEntry {
    std::string title;
    std::string href;
    std::string anchor;
    uint8_t level;
    int16_t spineIndex;

    TocEntry() : level(0), spineIndex(-1) {}
    TocEntry(std::string title, std::string href, std::string anchor, const uint8_t level, const int16_t spineIndex)
        : title(std::move(title)),
          href(std::move(href)),
          anchor(std::move(anchor)),
          level(level),
          spineIndex(spineIndex) {}
  };

 private:
  std::string cachePath;
  uint32_t lutOffset;
  uint16_t spineCount;
  uint16_t tocCount;
  bool loaded;
  bool buildMode;

  HalFile bookFile;
  // Temp file handles during build
  HalFile spineFile;
  HalFile tocFile;
  // Buffers the per-entry tmp-file writes during the OPF/TOC passes: those
  // writes interleave with zip-inflate SD reads, and unbuffered they thrash
  // SdFat's shared sector cache (one 512B transaction per 4-byte pod). One
  // wrapper serves whichever pass is active (spine, then toc).
  std::unique_ptr<serialization::BufferedFileWriter> passOut;

  // Cumulative spine sizes stay in book.bin; RAM holds every BB_BOOK_WINDOW-th one and one window.
  std::vector<uint32_t> windowStarts;  // total before item w * BB_BOOK_WINDOW
  uint32_t bookSize = 0;
  mutable int32_t windowFirst = -1;
  mutable uint32_t window[BB_BOOK_WINDOW] = {};
  bool fillWindow(int32_t first, uint32_t* into) const;  // caller holds sizeWindowLock
  bb_book_sizes sizesView() const;

  // From this many spine items, TOC hrefs are found through spineHrefIndex (sorted by hash and
  // length) instead of by reading the spine file, and zip sizes are looked up in batches.
  static constexpr uint16_t LARGE_SPINE_THRESHOLD = 400;
  std::unique_ptr<bb_book_index_entry[]> spineHrefIndex;

  // Where the pass's next record goes: through passOut when it has one, else straight to file.
  bb_io_sink passSink(HalFile& file, uint32_t& at);

 public:
  BookMetadata coreMetadata;

  // Out of line, as passOut's type is complete only in the .cpp.
  explicit BookMetadataCache(std::string cachePath);
  ~BookMetadataCache();

  // Building phase (stream to disk immediately)
  bool beginWrite();
  bool beginContentOpfPass();
  void createSpineEntry(const std::string& href);
  bool endContentOpfPass();
  bool beginTocPass();
  void createTocEntry(const std::string& title, const std::string& href, const std::string& anchor, uint8_t level);
  bool endTocPass();
  bool endWrite();
  bool cleanupTmpFiles() const;

  // Post-processing to update mappings and sizes
  bool buildBookBin(const std::string& epubPath, const BookMetadata& metadata);

  // Reading phase (read mode)
  bool load();
  SpineEntry getSpineEntry(int index);
  TocEntry getTocEntry(int index);
  // Cumulative byte size up to and including the given spine item (0 if out of range
  // or not loaded).
  uint32_t getCumulativeSize(int index) const;
  // First spine index whose cumulative size reaches `size` (-1 past the end or not loaded). Reads at most one window.
  int getSpineIndexForSize(uint32_t size) const;
  int getSpineCount() const { return spineCount; }
  int getTocCount() const { return tocCount; }
  bool isLoaded() const { return loaded; }
};
