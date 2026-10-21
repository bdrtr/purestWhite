#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <map>
#include <string>
#include <vector>

// Files in memory that read and seek as SdFat does: a seek past the end fails and leaves the
// position where it was.
class HalFile {
 public:
  bool isOpen() const { return data_ != nullptr; }
  explicit operator bool() const { return isOpen(); }
  void close() { data_ = nullptr; }
  uint64_t fileSize64() const { return data_ ? data_->size() : 0; }
  bool seek(size_t offset) { return seek64(offset); }
  bool seek64(uint64_t offset) {
    if (!data_ || offset > data_->size()) return false;
    position_ = static_cast<size_t>(offset);
    return true;
  }
  int read(void* buf, size_t count) {
    if (!data_) return -1;
    count = std::min(count, data_->size() - position_);
    if (count > 0) std::memcpy(buf, data_->data() + position_, count);
    position_ += count;
    return static_cast<int>(count);
  }

 private:
  friend class HalStorage;
  const std::vector<uint8_t>* data_ = nullptr;
  size_t position_ = 0;
};

class HalStorage {
 public:
  static HalStorage& getInstance() {
    static HalStorage storage;
    return storage;
  }
  std::map<std::string, std::vector<uint8_t>> files;
  // Opens left before failing; -1 never fails.
  int opensLeft = -1;
  bool openFileForRead(const char*, const std::string& path, HalFile& file) {
    if (opensLeft == 0) return false;
    if (opensLeft > 0) opensLeft--;
    const auto it = files.find(path);
    if (it == files.end()) return false;
    file.data_ = &it->second;
    file.position_ = 0;
    return true;
  }
  bool openFileForRead(const char* tag, const char* path, HalFile& file) {
    return openFileForRead(tag, std::string(path), file);
  }
};

#define Storage HalStorage::getInstance()
