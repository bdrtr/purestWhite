// C ABI of rust/crates/bembeyaz-ffi/src/image.rs. Use lib/Epub's ImageDimsProbe instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A JPEG or PNG size probe part way through a file (bembeyaz-image's, 28 bytes): storage that
// bb_image_probe_init sets up. It holds no memory of its own.
typedef struct {
  uint32_t opaque[7];
} bb_image_probe;

// Starts a probe in probe's storage.
void bb_image_probe_init(bb_image_probe* probe);

// Reads the next len bytes of the file until the size is known or the file is known not to have
// one, and returns how many it took before the byte that settled it (len if neither happened).
size_t bb_image_probe_feed(bb_image_probe* probe, const uint8_t* data, size_t len);
// The width and height, once found.
bool bb_image_probe_size(const bb_image_probe* probe, uint32_t* width, uint32_t* height);

#ifdef __cplusplus
}
#endif
