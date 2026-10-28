// C ABI of rust/crates/bembeyaz-ffi/src/touch.rs (rust/crates/purestwhite-touch): what a contact on
// the GT911 means, as freeink-sdk's InputManager reads it. Only test/gt911_touch calls it, to
// compare the two: the firmware's touch path stays the SDK's, and the bare-metal firmware uses the
// crate directly.
#pragma once

#include <purestwhite/gt911.h>
#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct pw_touch pw_touch;

typedef struct {
  uint16_t x, y;
} pw_touch_point;

// What one poll ended, as the SDK's was* queries after an update(); each point means something
// only when its flag is set, and is 0 otherwise.
typedef struct {
  pw_touch_point pressed_at, tap_at, swipe_start, swipe_end, long_press_at;
  bool pressed, released, tap, swipe, long_press;
  bool home_pressed, home_tapped, home_long_press, activity;
} pw_touch_events;

// A new state, nothing touched; free it with pw_touch_free.
pw_touch* pw_touch_new(void);
void pw_touch_free(pw_touch* touch);

// Takes a poll's result at time now: kind is what pw_gt911_read returned, and frame what it
// filled (read for PW_GT911_FRAME, its home_key for PW_GT911_CONTACTS_FAILED). An unknown kind is
// PW_GT911_FAILED, as is a NULL frame with PW_GT911_FRAME or PW_GT911_CONTACTS_FAILED. NULL touch
// or out does nothing.
void pw_touch_update(pw_touch* touch, uint32_t now, uint8_t kind, const pw_gt911_frame* frame, pw_touch_events* out);

// The SDK's suppressTouchContact, isTouchPressed, isTouchTapCandidate, isTouchHeldAt and
// lastTouchHeldMs. The getters give false, or 0, for NULL; their out-parameters may be NULL. A
// suppress changes only later updates, not the events an update already returned.
void pw_touch_suppress(pw_touch* touch);
bool pw_touch_is_pressed(const pw_touch* touch);
bool pw_touch_tap_candidate(const pw_touch* touch, uint32_t now, pw_touch_point* at, uint32_t* held_ms);
bool pw_touch_held_at(const pw_touch* touch, pw_touch_point* at);
uint32_t pw_touch_last_held_ms(const pw_touch* touch);

#ifdef __cplusplus
}
#endif
