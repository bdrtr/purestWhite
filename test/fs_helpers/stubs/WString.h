#pragma once

// Native stand-in for the parts of the Arduino-ESP32 String that FsHelpers.h and the old web path
// code (test/web_paths) use, with the same semantics: searches and comparisons stop at the first
// NUL, as Arduino's strstr/strchr/strcmp-based members do.
#include <cstring>
#include <string>
#include <utility>

class String {
 public:
  String() = default;
  String(const char* s) : value_(s ? s : "") {}
  String(const std::string& s) : value_(s) {}

  const char* c_str() const { return value_.c_str(); }
  unsigned int length() const { return static_cast<unsigned int>(value_.length()); }
  bool isEmpty() const { return value_.empty(); }
  char charAt(unsigned int i) const { return i < value_.length() ? value_[i] : 0; }

  int indexOf(const char* s, unsigned int from = 0) const {
    if (from >= value_.length()) return -1;
    const char* found = strstr(c_str() + from, s);
    return found ? static_cast<int>(found - c_str()) : -1;
  }
  int indexOf(char ch, unsigned int from = 0) const {
    if (from >= value_.length()) return -1;
    const char* found = strchr(c_str() + from, ch);
    return found ? static_cast<int>(found - c_str()) : -1;
  }
  String substring(unsigned int left, unsigned int right) const {
    if (left > right) std::swap(left, right);
    if (left >= value_.length()) return String();
    if (right > value_.length()) right = static_cast<unsigned int>(value_.length());
    return String(value_.substr(left, right - left));
  }
  String substring(unsigned int left) const { return substring(left, length()); }
  bool startsWith(const char* s) const {
    const size_t n = strlen(s);
    return value_.length() >= n && strncmp(c_str(), s, n) == 0;
  }
  bool endsWith(const char* s) const {
    const size_t n = strlen(s);
    return value_.length() >= n && strcmp(c_str() + value_.length() - n, s) == 0;
  }
  bool equals(const char* s) const {
    if (value_.empty()) return s == nullptr || *s == 0;
    return strcmp(c_str(), s) == 0;
  }
  bool operator==(const char* s) const { return equals(s); }
  String& operator+=(char c) {
    value_ += c;
    return *this;
  }
  String& operator+=(const char* s) {
    value_ += s;
    return *this;
  }
  friend String operator+(const char* a, const String& b) { return String(std::string(a) + b.value_); }

 private:
  std::string value_;
};
