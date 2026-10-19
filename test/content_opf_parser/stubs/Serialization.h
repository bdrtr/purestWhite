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

inline void readString(HalFile& file, std::string& s) {
  uint32_t len = 0;
  file.read(reinterpret_cast<uint8_t*>(&len), sizeof(len));
  s.resize(len);
  file.read(reinterpret_cast<uint8_t*>(&s[0]), len);
}

}  // namespace serialization
