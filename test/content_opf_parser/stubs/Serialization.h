#pragma once

#include <string>

#include "Epub.h"

// As lib/Serialization: a uint32_t length, then the bytes.
namespace serialization {

inline void writeString(HalFile& file, const std::string& s) {
  const uint32_t len = s.size();
  file.write(reinterpret_cast<const uint8_t*>(&len), sizeof(len));
  file.write(reinterpret_cast<const uint8_t*>(s.data()), len);
}

// A length past the end of the file is counted in Storage.overreads and read as nothing, where
// the device would try to allocate it.
inline void readString(HalFile& file, std::string& s) {
  uint32_t len = 0;
  file.read(reinterpret_cast<uint8_t*>(&len), sizeof(len));
  if (len > static_cast<uint32_t>(file.available())) {
    Storage.overreads++;
    s.clear();
    return;
  }
  s.resize(len);
  file.read(reinterpret_cast<uint8_t*>(&s[0]), len);
}

}  // namespace serialization
