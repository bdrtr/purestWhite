#include "htmlEntities.h"

#include <bembeyaz/html.h>

#include <cstring>

const char* lookupHtmlEntity(const char* entity, const size_t len) {
  if (entity == nullptr || len < 2 || entity[0] != '&' || entity[len - 1] != ';') return nullptr;
  return bb_html_entity(entity + 1, len - 2);
}

void htmlEntityText(const char* name, void* userData, void (*text)(void* userData, const char* s, int len)) {
  const size_t len = strlen(name);
  const char* utf8Value = bb_html_entity(name, len);
  if (utf8Value != nullptr) {
    text(userData, utf8Value, static_cast<int>(strlen(utf8Value)));
    return;
  }
  // Not an HTML entity: kept as written, in one piece when it fits.
  char reference[48];
  if (len + 2 > sizeof(reference)) {
    text(userData, "&", 1);
    text(userData, name, static_cast<int>(len));
    text(userData, ";", 1);
    return;
  }
  reference[0] = '&';
  memcpy(reference + 1, name, len);
  reference[len + 1] = ';';
  text(userData, reference, static_cast<int>(len + 2));
}
