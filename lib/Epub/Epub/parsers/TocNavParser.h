#pragma once
#include "TocParser.h"

// An EPUB 3 nav document: entries are the links of <nav epub:type="toc">, their depth the nesting
// of its lists.
class TocNavParser final : public TocParser {
 public:
  explicit TocNavParser(const std::string& baseContentPath, const size_t xmlSize, BookMetadataCache* cache)
      : TocParser(BB_TOC_NAV, "NAV", baseContentPath, xmlSize, cache) {}
};
