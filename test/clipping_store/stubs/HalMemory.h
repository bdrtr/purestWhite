#pragma once
#include <HalStorage.h>

#include <cstddef>
#include <cstdint>
#include <memory>

// PSRAM as new[]; fake::failAlloc fails it like any other allocation.
class HalMemory {
 public:
  using PsramBuffer = std::unique_ptr<uint8_t[]>;
  static PsramBuffer allocatePsram(const size_t bytes) {
    if (fake::fail(fake::failAlloc)) return nullptr;
    return std::make_unique<uint8_t[]>(bytes);
  }
};
