// C ABI of rust/crates/bembeyaz-ffi/src/gt911.rs (rust/crates/purestwhite-gt911): the GT911 touch
// controller. Only test/gt911_touch calls it for now: the firmware's touch path stays
// freeink-sdk's InputManager until the touch state machine moves to Rust, and the bare-metal
// firmware uses the crate directly.
#pragma once

#include <purestwhite/i2c.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// How the board wires and mounts the controller, as freeink-sdk's TouchConfig.
typedef struct {
  uint8_t addr;      // tried first
  uint8_t alt_addr;  // then this one; 0 is not tried
  uint16_t raw_min_x, raw_max_x, raw_min_y, raw_max_y;
  bool swap_xy, flip_x, flip_y, coords_at_byte0;
} pw_gt911_config;

// RST and INT, both wired (otherwise pass NULL pins, as the SDK skips the reset). set_reset and
// drive_int make their pin an output at the level (INT's level set before it drives);
// release_int makes INT an input without a pull.
typedef struct {
  void* context;
  void (*set_reset)(void* context, bool high);
  void (*drive_int)(void* context, bool high);
  void (*release_int)(void* context);
} pw_gt911_pins;

// A contact in panel coordinates, 0 to raw_max - raw_min on each axis.
typedef struct {
  uint8_t id;
  uint16_t x, y;
} pw_gt911_contact;

typedef struct {
  bool home_key;
  uint8_t reported;  // the contacts the controller reports, which can be more than count
  uint8_t count;     // the contacts below
  pw_gt911_contact contacts[4];
} pw_gt911_frame;

// What pw_gt911_read found.
enum {
  PW_GT911_FAILED = 0,           // the status could not be read: keep what is known
  PW_GT911_NOT_READY = 1,        // no new frame
  PW_GT911_FRAME = 2,            // a new frame in out; one without contacts is a release
  PW_GT911_CONTACTS_FAILED = 3,  // a new frame whose contacts could not be read; out->home_key is new
};

// Resets the controller (when pins is not NULL) with INT low, then high, until one of the
// configured addresses answers; that address, or 0. The board's touch supply must be on.
uint8_t pw_gt911_begin(const pw_i2c* bus, const pw_gt911_pins* pins, const pw_gt911_config* config);

// Reads the frame at addr (what pw_gt911_begin returned; 0 reads nothing and is FAILED) and lets
// the controller fill the next when there was a new one. FRAME fills out; CONTACTS_FAILED writes
// only out->home_key; FAILED and NOT_READY leave out as it was. Every result but FAILED read the
// status (the SDK times the Home key's long press then). NULL bus, config or out is FAILED with
// no transfer, as NULL bus or config in pw_gt911_begin is 0.
uint8_t pw_gt911_read(const pw_i2c* bus, uint8_t addr, const pw_gt911_config* config, pw_gt911_frame* out);

#ifdef __cplusplus
}
#endif
