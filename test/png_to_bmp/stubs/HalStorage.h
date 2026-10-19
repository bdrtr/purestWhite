#pragma once

#include <Print.h>

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <vector>

// A file in memory that reads and seeks as SdFat does: a seek past the end fails and leaves the
// position where it was.
class HalFile : public Print {
 public:
  explicit HalFile(const std::vector<uint8_t>& data) : data_(&data) {}
  size_t size() const { return data_->size(); }
  bool seekSet(size_t offset) {
    if (offset > data_->size()) return false;
    position_ = offset;
    return true;
  }
  bool seekCur(int64_t offset) {
    const int64_t to = static_cast<int64_t>(position_) + offset;
    return to >= 0 && seekSet(static_cast<size_t>(to));
  }
  int read(void* buf, size_t count) {
    count = std::min(count, data_->size() - position_);
    if (count > 0) std::memcpy(buf, data_->data() + position_, count);
    position_ += count;
    return static_cast<int>(count);
  }
  size_t write(uint8_t) override { return 0; }

 private:
  const std::vector<uint8_t>* data_;
  size_t position_ = 0;
};
