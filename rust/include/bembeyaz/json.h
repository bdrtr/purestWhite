// C ABI of rust/crates/bembeyaz-ffi/src/json.rs. Use lib/JsonSax/StreamingJsonParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The token buffer (NUL included: at most 2047 bytes of text) and the nesting limit; the Rust side
// checks both.
#define BB_JSON_TOKEN_BUF_SIZE 2048
#define BB_JSON_MAX_NESTING 32

// One JSON document being read (about 2.1 KB, no heap). bb_json_init sets it up; it needs no
// release and must not be copied while in use.
typedef struct {
  uint32_t opaque[524];
} bb_json_parser;

// Laid out as JsonCallbacks (StreamingJsonParser.h); StreamingJsonParser.cpp checks it. A NULL
// function skips its event. Keys, strings and numbers arrive decoded, followed by a NUL, valid
// during the call.
typedef struct {
  void* ctx;
  void (*on_key)(void* ctx, const char* key, size_t len);
  void (*on_string)(void* ctx, const char* value, size_t len);
  void (*on_number)(void* ctx, const char* value, size_t len);
  void (*on_bool)(void* ctx, bool value);
  void (*on_null)(void* ctx);
  void (*on_object_start)(void* ctx);
  void (*on_object_end)(void* ctx);
  void (*on_array_start)(void* ctx);
  void (*on_array_end)(void* ctx);
} bb_json_callbacks;

void bb_json_init(bb_json_parser* parser);
void bb_json_reset(bb_json_parser* parser);
void bb_json_feed(bb_json_parser* parser, const bb_json_callbacks* callbacks, const char* data, size_t len);
bool bb_json_has_error(const bb_json_parser* parser);

#ifdef __cplusplus
}
#endif
