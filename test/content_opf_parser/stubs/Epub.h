#pragma once

#include <gtest/gtest.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <map>
#include <memory>
#include <string>
#include <vector>

// An SD card file held in memory: TestStorage keeps the bytes by path.
class HalFile {
 public:
  explicit operator bool() const { return open_; }
  void close() {
    EXPECT_TRUE(initialized_) << "Closing a default HalFile asserts on device";
    open_ = false;
  }
  bool seek(size_t position) {
    position_ = position;
    return true;
  }
  size_t position() const { return position_; }
  int available() const {
    return open_ && data_ && position_ < data_->size() ? static_cast<int>(data_->size() - position_) : 0;
  }
  size_t write(const uint8_t* bytes, size_t len) {
    if (!open_ || !data_) return 0;
    if (data_->size() < position_ + len) data_->resize(position_ + len);
    std::memcpy(data_->data() + position_, bytes, len);
    position_ += len;
    return len;
  }
  int read(uint8_t* bytes, size_t len) {
    const size_t n = std::min(len, static_cast<size_t>(available()));
    if (n > 0) std::memcpy(bytes, data_->data() + position_, n);
    position_ += n;
    return static_cast<int>(n);
  }
  void markOpen(std::shared_ptr<std::vector<uint8_t>> data = nullptr) {
    initialized_ = true;
    open_ = true;
    position_ = 0;
    data_ = std::move(data);
  }

 private:
  bool open_ = false;
  bool initialized_ = false;
  size_t position_ = 0;
  std::shared_ptr<std::vector<uint8_t>> data_;
};

struct TestStorage {
  int writeOpens = 0;
  int readOpens = 0;
  int overreads = 0;
  std::map<std::string, std::shared_ptr<std::vector<uint8_t>>> files;

  bool openFileForWrite(const char*, const std::string& path, HalFile& file) {
    writeOpens++;
    auto& data = files[path];
    data = std::make_shared<std::vector<uint8_t>>();
    file.markOpen(data);
    return true;
  }
  bool openFileForRead(const char*, const std::string& path, HalFile& file) {
    readOpens++;
    const auto found = files.find(path);
    if (found == files.end()) return false;
    file.markOpen(found->second);
    return true;
  }
  bool exists(const char* path) const { return files.count(path) != 0; }
  bool remove(const char* path) { return files.erase(path) != 0; }
};

inline TestStorage Storage;
