// What the settings stores' C ABIs share (rust/include/bembeyaz/store_*.h).
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Bytes (not NUL-terminated); NULL data with len 0 is empty text.
typedef struct {
  const char* data;
  size_t len;
} bb_store_text;

// Receives len bytes at data (not NUL-terminated). It runs inside Rust: it must return normally
// (no exception, no longjmp).
typedef void (*bb_store_write)(const char* data, size_t len, void* context);

#ifdef __cplusplus
}
#endif
