// The caller-owned I2C bus the purestwhite drivers use (rust/crates/purestwhite-i2c);
// lib/hal/WireBus.h makes one from an Arduino TwoWire.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// An I2C bus. write sends len bytes to addr in one transfer; write_read sends out_len bytes (a
// register address, of any width; 1 to 255) and, after a repeated start, reads len bytes (up to
// 255). Both are false when the transfer fails.
typedef struct {
  void* context;
  bool (*write)(void* context, uint8_t addr, const uint8_t* bytes, size_t len);
  bool (*write_read)(void* context, uint8_t addr, const uint8_t* out, size_t out_len, uint8_t* buf, size_t len);
  void (*pause_ms)(void* context, uint32_t ms);
} pw_i2c;

#ifdef __cplusplus
}
#endif
