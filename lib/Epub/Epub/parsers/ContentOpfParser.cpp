#include "ContentOpfParser.h"

#include <Logging.h>
#include <bembeyaz/xml.h>

#include <cmath>
#include <cstdlib>

#include "Epub/BookMetadataCache.h"

bool ContentOpfParser::setup() {
  bb_opf_free(opf);
  const uint32_t flags = (cache ? BB_OPF_SPINE : 0) | (metadataOnly ? BB_OPF_METADATA_ONLY : 0);
  opf = bb_opf_new(baseContentPath.data(), baseContentPath.size(), flags, BB_XML_DEFAULT_LIMIT, BB_OPF_MANIFEST_LIMIT);
  if (!opf) {
    LOG_DBG("COF", "Couldn't allocate memory for parser");
    return false;
  }
  return true;
}

ContentOpfParser::~ContentOpfParser() { bb_opf_free(opf); }

void ContentOpfParser::spineItem(void* context, const char* href, const size_t len) {
  static_cast<ContentOpfParser*>(context)->cache->createSpineEntry(std::string(href, len));
}

void ContentOpfParser::stylesheet(void* context, const char* href, const size_t len) {
  static_cast<ContentOpfParser*>(context)->cssFiles.emplace_back(href, len);
}

bool ContentOpfParser::parseFloat(void*, const char* text, const size_t len, float* out) {
  const std::string value(text, len);
  char* end = nullptr;
  const float parsed = std::strtof(value.c_str(), &end);
  if (end == value.c_str() || *end != '\0' || !std::isfinite(parsed)) return false;
  *out = parsed;
  return true;
}

void ContentOpfParser::readFields() {
  const auto read = [this](const int field, std::string& out) {
    size_t len = 0;
    const char* value = bb_opf_field(opf, field, &len);
    out.assign(value, len);
  };
  const bool hadNav = !tocNavPath.empty();
  read(BB_OPF_TITLE, title);
  read(BB_OPF_AUTHOR, author);
  read(BB_OPF_LANGUAGE, language);
  read(BB_OPF_ISBN, isbn);
  read(BB_OPF_ASIN, asin);
  read(BB_OPF_SERIES, series);
  read(BB_OPF_TOC_NCX, tocNcxPath);
  read(BB_OPF_TOC_NAV, tocNavPath);
  read(BB_OPF_COVER, coverItemHref);
  read(BB_OPF_GUIDE_COVER, guideCoverPageHref);
  read(BB_OPF_TEXT_REFERENCE, textReferenceHref);
  float index = 0;
  seriesIndex = bb_opf_series_index(opf, &index) ? std::optional<float>(index) : std::nullopt;
  if (!hadNav && !tocNavPath.empty()) LOG_DBG("COF", "Found EPUB 3 nav document: %s", tocNavPath.c_str());
}

size_t ContentOpfParser::write(const uint8_t data) { return write(&data, 1); }

size_t ContentOpfParser::write(const uint8_t* buffer, const size_t size) {
  if (!opf) return 0;

  const bool last = remainingSize == size;
  remainingSize -= size;
  const bb_opf_callbacks callbacks = {spineItem, stylesheet, parseFloat};
  const int result = bb_opf_parse(opf, reinterpret_cast<const char*>(buffer), size, last, &callbacks, this);
  readFields();
  if (result == BB_XML_OK) {
    // The fields are copied; the manifest is not needed any more.
    if (last) {
      bb_opf_free(opf);
      opf = nullptr;
    }
    return size;
  }
  // Stopped at the end of the metadata: a short write tells the stream it has what it needs.
  if (result == BB_XML_ABORTED) return size > 0 ? size - 1 : 0;
  LOG_DBG("COF", "Parse error at byte %llu: %s", static_cast<unsigned long long>(bb_opf_error_offset(opf)),
          bb_xml_error_message(result));
  bb_opf_free(opf);
  opf = nullptr;
  return 0;
}
