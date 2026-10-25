// C ABI of rust/crates/bembeyaz-ffi/src/gauge.rs (rust/crates/purestwhite-gauge): the CW2017 fuel
// gauge. The bus is the caller's; use lib/hal/HalPowerManager.h instead.
#pragma once

#include <purestwhite/i2c.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// What is known about the gauge between readings, opaque; zero it before the first.
typedef struct {
  uint32_t opaque[2];
} pw_cw2017;

// The state of charge (0 to 100) at now_ms (a millisecond clock that may wrap); false when the
// gauge cannot give one now, leaving out as it was. The first reading checks the battery profile
// (uploading it when it differs) and restarts the gauge when needed, which can take a few
// seconds; after a failed first reading it is tried again no sooner than a second later.
bool pw_cw2017_soc(pw_cw2017* gauge, const pw_i2c* bus, uint8_t addr, uint32_t now_ms, uint8_t* out);

#ifdef __cplusplus
}
#endif
