#pragma once

#include <cstddef>

inline unsigned long millis() { return 0; }
struct DictionaryTestEsp {
  size_t getMaxAllocHeap() const { return 200 * 1024; }
};
inline DictionaryTestEsp ESP;
