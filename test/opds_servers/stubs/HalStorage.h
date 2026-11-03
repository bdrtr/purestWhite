// Host stub: one file. The driver puts the next file read in storageFile and finds what the store
// wrote in storageWritten (storageExists, a missing file when false, is always true here).
#pragma once

#include <Arduino.h>

#include <string>

inline bool storageExists = true;
inline std::string storageFile;
inline std::string storageWritten;

struct HalStorageStub {
  bool exists(const char*) const { return storageExists; }
  bool mkdir(const char*, bool = true) const { return true; }
  String readFile(const char*) const { return String(storageFile.c_str()); }
  bool writeFile(const char*, const String& content) const {
    storageWritten.assign(content.c_str(), content.length());
    return true;
  }
};
inline HalStorageStub Storage;
