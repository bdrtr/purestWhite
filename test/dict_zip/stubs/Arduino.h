#pragma once

#include <cstddef>

// The largest free block, which a test can shrink.
struct DictZipTestEsp {
  size_t maxAllocHeap = 200 * 1024;
  size_t getMaxAllocHeap() const { return maxAllocHeap; }
};
inline DictZipTestEsp ESP;
