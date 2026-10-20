#pragma once
#include <Print.h>
#include <bembeyaz/toc.h>

#include <string>

class BookMetadataCache;

// Reads a table of contents document streamed into it (rust/crates/bembeyaz-toc); each entry goes
// into the cache. Once the document fails (parse error or out of memory), that write and every
// later write return 0.
class TocParser : public Print {
  const int kind;
  const char* const logTag;
  const std::string& baseContentPath;
  size_t remainingSize;
  BookMetadataCache* cache;
  bb_toc* toc = nullptr;

  static void entry(void* context, const bb_toc_entry* entry);

 protected:
  TocParser(int kind, const char* logTag, const std::string& baseContentPath, size_t xmlSize, BookMetadataCache* cache)
      : kind(kind), logTag(logTag), baseContentPath(baseContentPath), remainingSize(xmlSize), cache(cache) {}

 public:
  ~TocParser() override;
  TocParser(const TocParser&) = delete;
  TocParser& operator=(const TocParser&) = delete;

  bool setup();

  size_t write(uint8_t) override;
  size_t write(const uint8_t* buffer, size_t size) override;
};
