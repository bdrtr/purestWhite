// Host stub: a minimal String, for PersistableStore.h's text functions and the stores' JSON text
// (toJsonText/fromJsonText).
#pragma once
#include <cstddef>
#include <cstdint>
#include <string>

class Stream;

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
  // What ArduinoJson uses of Arduino's String (ARDUINOJSON_ENABLE_ARDUINO_STRING).
  String& operator=(const char* s) {
    text = s ? s : "";
    return *this;
  }
  bool concat(const char* s) {
    text += s;
    return true;
  }

 private:
  std::string text;
};
