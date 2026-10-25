#pragma once

// A pw_i2c over an Arduino TwoWire, for the purestwhite drivers. The bus stays the TwoWire's,
// because freeink-sdk's drivers on the same pins (touch, fuel gauge) use it too; the transfers
// are the ones the SDK's Rtc driver made.
#include <Arduino.h>
#include <Wire.h>
#include <purestwhite/i2c.h>

static_assert(sizeof(pw_i2c) == 4 * sizeof(void*), "pw_i2c layout");

namespace wirebus {

inline bool write(void* context, const uint8_t addr, const uint8_t* bytes, const size_t len) {
  auto& wire = *static_cast<TwoWire*>(context);
  wire.beginTransmission(addr);
  for (size_t i = 0; i < len; ++i) wire.write(bytes[i]);
  return wire.endTransmission() == 0;
}

// The register address, then a repeated start for the read.
inline bool writeRead(void* context, const uint8_t addr, const uint8_t* out, const size_t outLen, uint8_t* buf,
                      const size_t len) {
  auto& wire = *static_cast<TwoWire*>(context);
  // Before the transmission takes the bus lock; TwoWire would drop bytes past its buffer.
  if (outLen == 0 || outLen > UINT8_MAX || len > UINT8_MAX) return false;
  wire.beginTransmission(addr);
  for (size_t i = 0; i < outLen; ++i) wire.write(out[i]);
  if (wire.endTransmission(false) != 0) return false;
  const auto count = static_cast<uint8_t>(len);
  if (wire.requestFrom(addr, count, static_cast<uint8_t>(true)) < count) return false;
  for (size_t i = 0; i < len; ++i) buf[i] = static_cast<uint8_t>(wire.read());
  return true;
}

inline void pauseMs(void*, const uint32_t ms) { delay(ms); }

}  // namespace wirebus

inline pw_i2c wireBus(TwoWire& wire) { return {&wire, wirebus::write, wirebus::writeRead, wirebus::pauseMs}; }
