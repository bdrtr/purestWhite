// C ABI of rust/crates/bembeyaz-ffi/src/rtc.rs (rust/crates/purestwhite-rtc): the BM8563 / PCF8563
// real-time clock. The bus is the caller's; use lib/hal/HalClock.h instead.
#pragma once

#include <purestwhite/i2c.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A UTC wall-clock time; year is the full year, weekday 0 is Sunday.
typedef struct {
  uint16_t year;
  uint8_t month;
  uint8_t day;
  uint8_t hour;
  uint8_t minute;
  uint8_t second;
  uint8_t weekday;
} pw_rtc_time;

// Whether the clock at addr answers (three tries); turns its CLKOUT output off.
bool pw_rtc_begin(const pw_i2c* bus, uint8_t addr);
// The time; false when the read fails or the oscillator stopped, leaving out as it was.
bool pw_rtc_now(const pw_i2c* bus, uint8_t addr, pw_rtc_time* out);
// Sets the time; false when the write fails.
bool pw_rtc_set(const pw_i2c* bus, uint8_t addr, const pw_rtc_time* time);
// The Unix time of a UTC time; 0 for NULL.
int64_t pw_rtc_epoch(const pw_rtc_time* time);

#ifdef __cplusplus
}
#endif
