#pragma once

#include <cstdint>

// The X4 panel: 800x480, so covers are sized 480x800.
class HalDisplay {
 public:
  uint16_t getDisplayWidth() const { return 800; }
  uint16_t getDisplayHeight() const { return 480; }
};
inline HalDisplay display;
