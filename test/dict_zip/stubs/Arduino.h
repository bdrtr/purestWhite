#pragma once

#include <cstddef>

struct DictZipTestEsp {
  size_t getMaxAllocHeap() const { return 200 * 1024; }
};
inline DictZipTestEsp ESP;
