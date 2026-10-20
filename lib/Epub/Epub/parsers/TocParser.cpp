#include "TocParser.h"

#include <Logging.h>
#include <bembeyaz/xml.h>

#include <cstddef>

#include "Epub/BookMetadataCache.h"

// Checked against Entry in rust/crates/bembeyaz-ffi/src/toc.rs too.
static_assert(sizeof(bb_toc_entry) == 7 * sizeof(size_t) && offsetof(bb_toc_entry, level) == 6 * sizeof(size_t),
              "bb_toc_entry layout");

TocParser::~TocParser() { bb_toc_free(toc); }

bool TocParser::setup() {
  bb_toc_free(toc);
  toc = bb_toc_new(kind, baseContentPath.data(), baseContentPath.size(), BB_XML_DEFAULT_LIMIT);
  if (!toc) {
    LOG_DBG(logTag, "Couldn't allocate memory for parser");
    return false;
  }
  return true;
}

void TocParser::entry(void* context, const bb_toc_entry* entry) {
  auto* self = static_cast<TocParser*>(context);
  if (self->cache) {
    self->cache->createTocEntry(std::string(entry->label, entry->label_len), std::string(entry->href, entry->href_len),
                                std::string(entry->anchor, entry->anchor_len), entry->level);
  }
}

size_t TocParser::write(const uint8_t data) { return write(&data, 1); }

size_t TocParser::write(const uint8_t* buffer, const size_t size) {
  if (!toc) return 0;

  const bool last = remainingSize == size;
  remainingSize -= size;
  const int result = bb_toc_parse(toc, reinterpret_cast<const char*>(buffer), size, last, entry, this);
  if (result != BB_XML_OK) {
    LOG_DBG(logTag, "Parse error at byte %llu: %s", static_cast<unsigned long long>(bb_toc_error_offset(toc)),
            bb_xml_error_message(result));
    bb_toc_free(toc);
    toc = nullptr;
    return 0;
  }
  return size;
}
