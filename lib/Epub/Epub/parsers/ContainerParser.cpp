#include "ContainerParser.h"

#include <Logging.h>
#include <bembeyaz/xml.h>

ContainerParser::~ContainerParser() { bb_container_free(container); }

bool ContainerParser::setup() {
  bb_container_free(container);
  container = bb_container_new(BB_XML_DEFAULT_LIMIT);
  if (!container) {
    LOG_ERR("CTR", "Couldn't allocate memory for parser");
    return false;
  }
  return true;
}

size_t ContainerParser::write(const uint8_t data) { return write(&data, 1); }

size_t ContainerParser::write(const uint8_t* buffer, const size_t size) {
  if (!container) return 0;

  const bool last = remainingSize == size;
  remainingSize -= size;
  const int result = bb_container_parse(container, reinterpret_cast<const char*>(buffer), size, last);
  // Copied when it changes: a long path is not copied again for every later piece.
  if (bb_container_take_path_changed(container)) {
    size_t len = 0;
    const char* path = bb_container_full_path(container, &len);
    fullPath.assign(path, len);
  }
  if (result != BB_XML_OK) {
    LOG_ERR("CTR", "Parse error at byte %llu: %s",
            static_cast<unsigned long long>(bb_container_error_offset(container)), bb_xml_error_message(result));
    bb_container_free(container);
    container = nullptr;
    return 0;
  }
  return size;
}
