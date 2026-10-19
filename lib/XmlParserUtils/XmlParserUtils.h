#pragma once

#include <cstring>

inline const char* xmlLocalName(const char* qName) {
  if (!qName) return "";
  const char* const separator = std::strchr(qName, ':');
  return separator ? separator + 1 : qName;
}

inline bool xmlLocalNameEquals(const char* qName, const char* expected) {
  return std::strcmp(xmlLocalName(qName), expected) == 0;
}
