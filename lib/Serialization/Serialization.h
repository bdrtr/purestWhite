#pragma once
#include <HalStorage.h>

#include <cstring>
#include <iostream>
#include <limits>
#include <type_traits>

namespace serialization {
template <typename T>
void writePod(std::ostream& os, const T& value) {
  os.write(reinterpret_cast<const char*>(&value), sizeof(T));
}

template <typename T>
void writePod(HalFile& file, const T& value) {
  file.write(reinterpret_cast<const uint8_t*>(&value), sizeof(T));
}

template <typename T>
bool tryWritePod(HalFile& file, const T& value) {
  return file.write(reinterpret_cast<const uint8_t*>(&value), sizeof(T)) == sizeof(T);
}

template <typename T>
void readPod(std::istream& is, T& value) {
  is.read(reinterpret_cast<char*>(&value), sizeof(T));
}

// False when the file ended first. The value is then zero, not whatever the variable held, so a
// count read from a cut or damaged file stays small.
template <typename T>
bool readPod(HalFile& file, T& value) {
  static_assert(std::is_trivially_copyable_v<T>, "readPod copies bytes");
  if (file.read(reinterpret_cast<uint8_t*>(&value), sizeof(T)) == static_cast<int>(sizeof(T))) return true;
  std::memset(&value, 0, sizeof(T));
  return false;
}

template <typename T>
bool tryReadPod(HalFile& file, T& value) {
  return file.read(reinterpret_cast<uint8_t*>(&value), sizeof(T)) == sizeof(T);
}

inline void writeString(std::ostream& os, const std::string& s) {
  const uint32_t len = s.size();
  writePod(os, len);
  os.write(s.data(), len);
}

inline void writeString(HalFile& file, const std::string& s) {
  const uint32_t len = s.size();
  writePod(file, len);
  file.write(reinterpret_cast<const uint8_t*>(s.data()), len);
}

inline bool tryWriteString(HalFile& file, const std::string& s) {
  const uint32_t len = s.size();
  return tryWritePod(file, len) && (len == 0 || file.write(reinterpret_cast<const uint8_t*>(s.data()), len) == len);
}

inline void readString(std::istream& is, std::string& s) {
  uint32_t len;
  readPod(is, len);
  s.resize(len);
  is.read(&s[0], len);
}

// False, with s empty, when the file ends first. A length past the end means the file is damaged
// (allocating it would abort, as there are no exceptions): the read skips to the end, so the reads
// after it fail too.
inline bool readString(HalFile& file, std::string& s) {
  uint32_t len = 0;
  const bool haveLength = readPod(file, len);
  const int remaining = file.available();
  if (!haveLength || remaining < 0 || len > static_cast<uint32_t>(remaining)) {
    s.clear();
    file.seek(file.size());
    return false;
  }
  s.resize(len);
  return len == 0 || file.read(&s[0], len) == static_cast<int>(len);
}

inline bool tryReadString(HalFile& file, std::string& s, const size_t maxLength) {
  uint32_t len = 0;
  if (!tryReadPod(file, len)) return false;
  const int remaining = file.available();
  if (static_cast<size_t>(len) > maxLength || static_cast<size_t>(len) > s.max_size() ||
      len > static_cast<uint32_t>(std::numeric_limits<int>::max()) || remaining < 0 ||
      len > static_cast<uint32_t>(remaining)) {
    return false;
  }
  s.resize(len);
  return len == 0 || file.read(&s[0], len) == static_cast<int>(len);
}
}  // namespace serialization
