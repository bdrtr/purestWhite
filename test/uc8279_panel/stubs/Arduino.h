#pragma once
// Arduino for test/uc8279_panel: what freeink-sdk's EpdBus and Uc8279X4Driver call, with pin
// writes, BUSY reads and pauses reported to the test.
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>

#define IRAM_ATTR
#define DRAM_ATTR
constexpr int HIGH = 1, LOW = 0, INPUT = 1, OUTPUT = 3, INPUT_PULLUP = 5, CHANGE = 3, MSBFIRST = 1;

inline unsigned long hostMillis = 0;
inline std::function<void(int pin, int level)> onDigitalWrite;
inline std::function<int(int pin)> onDigitalRead;
inline std::function<void(unsigned long ms)> onDelay;

inline void pinMode(int, int) {}
inline void digitalWrite(int pin, int level) {
  if (onDigitalWrite) onDigitalWrite(pin, level);
}
inline int digitalRead(int pin) { return onDigitalRead ? onDigitalRead(pin) : HIGH; }
inline void delay(unsigned long ms) {
  hostMillis += ms;
  if (onDelay) onDelay(ms);
}
inline void delayMicroseconds(unsigned int) {}
inline unsigned long millis() { return hostMillis; }
inline int digitalPinToInterrupt(int p) { return p; }
inline void attachInterrupt(int, void (*)(), int) {}
inline void detachInterrupt(int) {}
struct SerialStub {
  explicit operator bool() const { return false; }
  template <typename... A>
  int printf(const char*, A...) {
    return 0;
  }
};
inline SerialStub Serial;
