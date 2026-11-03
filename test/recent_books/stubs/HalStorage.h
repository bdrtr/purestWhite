// Host stub: every book's file is on the card (the store's pruning is not part of its JSON).
#pragma once

struct HalStorageStub {
  bool exists(const char*) const { return true; }
};
inline HalStorageStub Storage;
