// Host stub: a minimal String, for PersistableStore.h's text functions and the stores' JSON text
// (toJsonText/fromJsonText).
#pragma once
#include <string>

class String {
 public:
  String() = default;
  explicit String(const char* s) : text(s) {}
  bool concat(const char* s, unsigned int len) {
    text.append(s, len);
    return true;
  }
  const char* c_str() const { return text.c_str(); }
  unsigned int length() const { return static_cast<unsigned int>(text.size()); }
  bool isEmpty() const { return text.empty(); }

 private:
  std::string text;
};
