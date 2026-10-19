#pragma once
#include <Print.h>
#include <XmlParser.h>

#include <string>

class BookMetadataCache;

class TocNcxParser final : public Print {
  enum ParserState { START, IN_NCX, IN_NAV_MAP, IN_NAV_POINT, IN_NAV_LABEL, IN_NAV_LABEL_TEXT, IN_CONTENT };

  const std::string& baseContentPath;
  size_t remainingSize;
  XmlParser parser;
  ParserState state = START;
  BookMetadataCache* cache;

  std::string currentLabel;
  std::string currentSrc;
  uint8_t currentDepth = 0;

  static void startElement(void* userData, const char* name, const char** atts);
  static void characterData(void* userData, const char* s, int len);
  static void endElement(void* userData, const char* name);

 public:
  explicit TocNcxParser(const std::string& baseContentPath, const size_t xmlSize, BookMetadataCache* cache)
      : baseContentPath(baseContentPath), remainingSize(xmlSize), cache(cache) {}

  bool setup();

  size_t write(uint8_t) override;
  size_t write(const uint8_t* buffer, size_t size) override;
};
