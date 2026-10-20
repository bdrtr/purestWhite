#pragma once
#include "TocParser.h"

// An EPUB 2 NCX document: entries are navPoints with their navLabel and content.
class TocNcxParser final : public TocParser {
 public:
  explicit TocNcxParser(const std::string& baseContentPath, const size_t xmlSize, BookMetadataCache* cache)
      : TocParser(BB_TOC_NCX, "TOC", baseContentPath, xmlSize, cache) {}
};
