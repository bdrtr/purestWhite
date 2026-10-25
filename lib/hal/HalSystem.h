#pragma once

#include <cstdint>
#include <string>

namespace HalSystem {
struct StackFrame {
  uint32_t sp;
  uint32_t spp[8];
};

void begin();

// Dump panic info to SD card if necessary
void checkPanic();
void clearPanic();

// The bare-metal firmware's smoke test (rust/firmware) runs once from the other OTA slot, selects
// this firmware for the next boot, leaves a text report in RTC slow memory and resets. Writes that
// report to /purestwhite-bare-metal.txt and the log; call after the SD card is up.
void saveBareMetalReport();

std::string getPanicInfo(bool full = false);
bool isRebootFromPanic();
}  // namespace HalSystem
