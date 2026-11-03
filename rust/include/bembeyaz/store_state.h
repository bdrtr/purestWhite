// C ABI of rust/crates/bembeyaz-ffi/src/store_state.rs. Use src/CrossPointState.h instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A ring of the sleep images shown last: pos is where the next goes, fill how many it holds.
typedef struct {
  uint16_t images[16];
  uint8_t pos;
  uint8_t fill;
} bb_store_state_ring;

// CrossPointState's fields except the open book's path.
typedef struct {
  bb_store_state_ring recent_sleep;
  bb_store_state_ring recent_overlay_sleep;
  uint8_t reader_activity_load_count;
  bool last_sleep_from_reader;
  bool show_boot_screen;
} bb_store_state;

// Hands write the state.json text for state and path (the open book, path_len bytes), in one call.
void bb_store_state_write(const bb_store_state* state, const char* path, size_t path_len, bb_store_write write,
                          void* context);
// Reads state.json's text (len bytes, read up to a NUL as a C string). NULL when it was read (the
// path handed to path, the rest in out), else ArduinoJson's name for why not (a static string),
// and out is left as it was.
const char* bb_store_state_read(const char* text, size_t len, bb_store_state* out, bb_store_write path,
                                void* context);

// Whether image is among the ring's last count (at most fill) images.
bool bb_store_state_is_recent(const bb_store_state_ring* ring, uint16_t image, uint8_t count);
// Remembers image as the ring's last.
void bb_store_state_push_recent(bb_store_state_ring* ring, uint16_t image);

#ifdef __cplusplus
}
#endif
