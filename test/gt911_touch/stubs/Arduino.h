#pragma once
// Arduino for test/gt911_touch: what freeink-sdk's InputManager and lib/hal/WireBus.h call, with
// the pin modes, pin writes and pauses reported to the test.
#include <climits>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <initializer_list>

#define IRAM_ATTR
constexpr int HIGH = 1, LOW = 0, INPUT = 1, OUTPUT = 3, INPUT_PULLUP = 5, INPUT_PULLDOWN = 6, FALLING = 2,
              ADC_11db = 3;

inline unsigned long hostMillis = 1000;
inline std::function<void(int pin, int level)> onDigitalWrite;
inline std::function<void(int pin, int mode)> onPinMode;
inline std::function<void(unsigned long ms)> onDelay;

inline void pinMode(int pin, int mode) {
  if (onPinMode) onPinMode(pin, mode);
}
inline void digitalWrite(int pin, int level) {
  if (onDigitalWrite) onDigitalWrite(pin, level);
}
inline int digitalRead(int) { return HIGH; }
inline void delay(unsigned long ms) {
  hostMillis += ms;
  if (onDelay) onDelay(ms);
}
inline void delayMicroseconds(unsigned int) {}
inline unsigned long millis() { return hostMillis; }
inline void analogSetAttenuation(int) {}
inline int analogReadMilliVolts(int) { return 3300; }
inline int analogRead(int) { return 4095; }
inline int digitalPinToInterrupt(int p) { return p; }
inline void attachInterrupt(int, void (*)(), int) {}
inline void detachInterrupt(int) {}
struct SerialStub {
  explicit operator bool() const { return false; }
};
inline SerialStub Serial;
