#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

// One .dz file in memory, with a read that fails from a chosen offset on, and writes that can fail.
struct DictZipTestFiles {
  std::vector<uint8_t> dz;
  size_t failReadsFrom = SIZE_MAX;
  bool failWrites = false;
};
inline DictZipTestFiles dictZipFiles;

class HalFile {
 public:
  bool seekSet(size_t offset) {
    position_ = offset;
    return offset <= dictZipFiles.dz.size();
  }
  size_t position() const { return position_; }
  size_t fileSize() const { return dictZipFiles.dz.size(); }
  int read(void* buf, size_t count) {
    if (position_ >= dictZipFiles.failReadsFrom) return -1;
    if (position_ > dictZipFiles.dz.size()) return 0;
    count = std::min(count, dictZipFiles.dz.size() - position_);
    std::memcpy(buf, dictZipFiles.dz.data() + position_, count);
    position_ += count;
    return static_cast<int>(count);
  }
  int read() {
    uint8_t b;
    return read(&b, 1) == 1 ? b : -1;
  }
  size_t write(const uint8_t* buf, size_t count) {
    if (dictZipFiles.failWrites) return 0;
    written.insert(written.end(), buf, buf + count);
    return count;
  }

  std::vector<uint8_t> written;

 private:
  size_t position_ = 0;
};

class HalStorage {
 public:
  bool openFileForRead(const char*, const char*, HalFile& file) { return file.seekSet(0); }
};
inline HalStorage Storage;
