// C ABI of rust/crates/bembeyaz-ffi/src/store_koreader_credentials.rs. Use
// lib/KOReaderSync/KOReaderCredentialStore.h instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// What a koreader.json file holds, for C++ to decide on: the password obfuscated, or a legacy plain
// password; each setting with its kind; sendMetadata and syncClippings read with `| false`; the
// first header_count headers.
typedef struct {
  bb_store_text username;
  bb_store_text password_obf;
  bb_store_text password;
  bb_store_text server_url;
  bb_store_header headers[BB_STORE_HEADERS];
  size_t header_count;
  bb_store_number cfg_version;
  bb_store_number server_type;
  bb_store_number match_method;
  bb_store_number sync_behavior;
  bool send_metadata;
  bool sync_clippings;
} bb_store_koreader_stored;

// What C++ saves: the password and the header values obfuscated; the first header_count headers.
typedef struct {
  bb_store_text username;
  bb_store_text password_obf;
  bb_store_text server_url;
  bb_store_header headers[BB_STORE_HEADERS];
  size_t header_count;
  uint8_t cfg_version;
  uint8_t server_type;
  uint8_t match_method;
  uint8_t sync_behavior;
  bool send_metadata;
  bool sync_clippings;
} bb_store_koreader_saved;

// Receives what a file holds; its texts are valid during the call only. It runs inside Rust: it
// must return normally (no exception, no longjmp).
typedef void (*bb_store_koreader_stored_read)(const bb_store_koreader_stored* stored, void* context);

// Hands write the koreader.json text for saved, in one call; false when it could not be made
// (memory ran out) and write was not called.
bool bb_store_koreader_credentials_write(const bb_store_koreader_saved* saved, bb_store_write write, void* context);
// Reads koreader.json's text (len bytes, read up to a NUL as a C string). NULL when it was read
// (what it holds handed to stored), else ArduinoJson's name for why not (a static string), and
// stored is not called.
const char* bb_store_koreader_credentials_read(const char* text, size_t len, bb_store_koreader_stored_read stored,
                                               void* context);

#ifdef __cplusplus
}
#endif
