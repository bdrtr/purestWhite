// What the settings stores' C ABIs share (rust/include/bembeyaz/store_*.h).
#pragma once

#include <stdbool.h>
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

// The extra request headers a server keeps (OpdsServer::MAX_CUSTOM_HEADERS and
// KOREADER_MAX_CUSTOM_HEADERS).
#define BB_STORE_HEADERS 2

// An extra request header: its name and its value as stored (obfuscated).
typedef struct {
  bb_store_text name;
  bb_store_text value_obf;
} bb_store_header;

// An extra request header with its plain value (as the store holds it).
typedef struct {
  bb_store_text name;
  bb_store_text value;
} bb_store_plain_header;

// Receives len bytes at data (not NUL-terminated). It runs inside Rust: it must return normally
// (no exception, no longjmp).
typedef void (*bb_store_write)(const char* data, size_t len, void* context);

// Decodes a stored secret, len bytes at data, of at most most bytes
// (obfuscation::deobfuscateFromBase64, the length checked before anything is held): true after
// handing its plain text to put (with put_context) in one call; false when it does not decode, or
// is too long, which sets *too_long. It runs inside Rust: it must return normally (no exception,
// no longjmp).
typedef bool (*bb_store_decode)(const char* data, size_t len, size_t most, bb_store_write put, void* put_context,
                                bool* too_long, void* context);

#ifdef __cplusplus
}
#endif
