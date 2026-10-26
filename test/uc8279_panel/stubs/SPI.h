#pragma once
// SPI for test/uc8279_panel: every byte goes to the test's onSpiWrite.
#include <cstddef>
#include <cstdint>
#include <functional>

constexpr int SPI_MODE0 = 0;
inline std::function<void(const uint8_t* bytes, size_t len)> onSpiWrite;

struct SPISettings {
  SPISettings() = default;
  SPISettings(uint32_t, int, int) {}
};

class SPIClass {
 public:
  void begin(int, int, int, int) {}
  void beginTransaction(const SPISettings&) {}
  void endTransaction() {}
  uint8_t transfer(uint8_t b) {
    onSpiWrite(&b, 1);
    return 0;
  }
  void writeBytes(const uint8_t* b, uint32_t len) { onSpiWrite(b, len); }
};

inline SPIClass SPI;
