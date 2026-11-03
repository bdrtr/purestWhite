// What the settings stores' C ABIs share (rust/include/bembeyaz/store_*.h).
#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Bytes (not NUL-terminated); NULL data with len 0 is empty text.
typedef struct {
  const char* data;
  size_t len;
} bb_store_text;

// What a stored number field holds, as ArduinoJson's isNull() and is<uint32_t>() see it.
enum {
  BB_STORE_MISSING = 0,  // missing, or null
  BB_STORE_U32 = 1,      // an integer within uint32_t: value holds it
  BB_STORE_OTHER = 2,    // anything else (a float, a string, an integer out of range...)
};

// A number field whose presence and type the C++ store decides on: value is 0 unless kind is
// BB_STORE_U32; byte is ArduinoJson's as<uint8_t>() of it, whatever its kind (a float, bool or
// string converted too, 0 when it cannot be), for a field the store reads with as<uint8_t>()
// (KOReader's serverType).
typedef struct {
  uint8_t kind;
  uint8_t byte;
  uint32_t value;
} bb_store_number;

// The extra request headers a server keeps (OpdsServer::MAX_CUSTOM_HEADERS and
// KOREADER_MAX_CUSTOM_HEADERS).
#define BB_STORE_HEADERS 2

// An extra request header: its name and its value as stored (obfuscated).
typedef struct {
  bb_store_text name;
  bb_store_text value_obf;
} bb_store_header;

// Receives len bytes at data (not NUL-terminated). It runs inside Rust: it must return normally
// (no exception, no longjmp).
typedef void (*bb_store_write)(const char* data, size_t len, void* context);

#ifdef __cplusplus
}
#endif
