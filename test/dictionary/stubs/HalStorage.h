#pragma once

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <map>
#include <memory>
#include <string>
#include <vector>

// Files in memory by path, shared by every handle. A read can fail from a chosen offset on, and a
// bad cluster fails seeks and reads from a chosen offset on.
struct DictionaryTestFs {
  std::map<std::string, std::shared_ptr<std::vector<uint8_t>>> files;
  std::map<std::string, size_t> failReadsFrom;
  std::map<std::string, size_t> badFrom;
};
inline DictionaryTestFs dictionaryFs;

constexpr int O_WRITE = 1;
constexpr int O_CREAT = 2;
constexpr int O_TRUNC = 4;

class HalFile {
 public:
  HalFile() = default;
  explicit HalFile(std::string path, std::shared_ptr<std::vector<uint8_t>> data)
      : path_(std::move(path)), data_(std::move(data)) {}

  explicit operator bool() const { return data_ != nullptr; }
  bool isOpen() const { return data_ != nullptr; }
  void close() { data_.reset(); }
  bool seekSet(size_t offset) {
    if (!data_ || offset > data_->size() || offset >= limit(dictionaryFs.badFrom)) return false;
    position_ = offset;
    return true;
  }
  size_t position() const { return position_; }
  size_t fileSize() const { return data_ ? data_->size() : 0; }
  int read(void* buf, size_t count) {
    if (!data_) return -1;
    if (position_ >= std::min(limit(dictionaryFs.failReadsFrom), limit(dictionaryFs.badFrom))) return -1;
    if (position_ >= data_->size()) return 0;
    count = std::min(count, data_->size() - position_);
    std::memcpy(buf, data_->data() + position_, count);
    position_ += count;
    return static_cast<int>(count);
  }
  int read() {
    uint8_t b;
    return read(&b, 1) == 1 ? b : -1;
  }
  int write(const void* buf, size_t count) {
    if (!data_) return 0;
    if (data_->size() < position_ + count) data_->resize(position_ + count);
    std::memcpy(data_->data() + position_, buf, count);
    position_ += count;
    return static_cast<int>(count);
  }

 private:
  size_t limit(const std::map<std::string, size_t>& from) const {
    const auto it = from.find(path_);
    return it == from.end() ? SIZE_MAX : it->second;
  }

  std::string path_;
  std::shared_ptr<std::vector<uint8_t>> data_;
  size_t position_ = 0;
};

class HalStorage {
 public:
  bool openFileForRead(const char*, const std::string& path, HalFile& file) {
    const auto it = dictionaryFs.files.find(path);
    if (it == dictionaryFs.files.end()) return false;
    file = HalFile(path, it->second);
    return true;
  }
  bool openFileForWrite(const char*, const std::string& path, HalFile& file) {
    auto data = std::make_shared<std::vector<uint8_t>>();
    dictionaryFs.files[path] = data;
    file = HalFile(path, data);
    return true;
  }
  bool exists(const char* path) { return dictionaryFs.files.count(path) != 0; }
  bool remove(const char* path) { return dictionaryFs.files.erase(path) != 0; }
  HalFile open(const char* path, int) {
    HalFile file;
    openFileForWrite("", path, file);
    return file;
  }
};
inline HalStorage Storage;
