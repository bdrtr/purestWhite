#pragma once
#include <Print.h>
#include <bembeyaz/opf.h>

#include <optional>
#include <string>
#include <vector>

class BookMetadataCache;

// Reads a package document (content.opf) streamed into it: rust/crates/bembeyaz-opf. The fields
// hold what has been read after each write.
class ContentOpfParser final : public Print {
  const std::string& baseContentPath;
  size_t remainingSize;
  BookMetadataCache* cache;
  const bool metadataOnly;
  bb_opf* opf = nullptr;

  static void spineItem(void* context, const char* href, size_t len);
  static void stylesheet(void* context, const char* href, size_t len);
  static bool parseFloat(void* context, const char* text, size_t len, float* out);
  void readFields();

 public:
  std::string title;
  std::string author;
  std::string language;
  std::string isbn;
  std::string asin;
  std::string series;
  std::optional<float> seriesIndex;
  std::string tocNcxPath;
  std::string tocNavPath;  // EPUB 3 nav document path
  std::string coverItemHref;
  std::string guideCoverPageHref;  // Guide reference with type="cover" or "cover-page" (points to XHTML wrapper)
  std::string textReferenceHref;
  std::vector<std::string> cssFiles;  // CSS stylesheet paths

  // With a cache, the spine goes into it; metadataOnly stops after the metadata.
  explicit ContentOpfParser(const std::string& baseContentPath, const size_t xmlSize, BookMetadataCache* cache,
                            const bool metadataOnly = false)
      : baseContentPath(baseContentPath), remainingSize(xmlSize), cache(cache), metadataOnly(metadataOnly) {}
  ~ContentOpfParser() override;
  ContentOpfParser(const ContentOpfParser&) = delete;
  ContentOpfParser& operator=(const ContentOpfParser&) = delete;

  bool setup();

  size_t write(uint8_t) override;
  size_t write(const uint8_t* buffer, size_t size) override;
};
