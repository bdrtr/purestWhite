// C ABI of rust/crates/bembeyaz-ffi/src/store_wifi_credentials.rs. Use src/WifiCredentialStore.h
// instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A credential as a file holds it, for C++ to decide on: the password obfuscated, or a legacy
// plain password, and the length and CRC-32 kept with it.
typedef struct {
  bb_store_text ssid;
  bb_store_text password_obf;
  bb_store_text password;
  bb_store_number password_len;
  bb_store_number password_crc32;
} bb_store_wifi_stored;

// A credential to save: the password obfuscated, its length and its CRC-32.
typedef struct {
  bb_store_text ssid;
  bb_store_text password_obf;
  uint64_t password_len;
  uint32_t password_crc32;
} bb_store_wifi_saved;

// Receives one item of a file's "credentials" array, and how many items it has; its texts are
// valid during the call only. It runs inside Rust: it must return normally (no exception, no
// longjmp).
typedef void (*bb_store_wifi_stored_read)(const bb_store_wifi_stored* stored, size_t count, void* context);

// Hands write the wifi.json text for last_connected_ssid and the count credentials at saved, in one
// call; false when it could not be made (memory ran out) and write was not called.
bool bb_store_wifi_credentials_write(bb_store_text last_connected_ssid, const bb_store_wifi_saved* saved, size_t count,
                                     bb_store_write write, void* context);
// Reads wifi.json's text (len bytes, read up to a NUL as a C string). NULL when it was read (the
// network connected last handed to last_connected, then every item of "credentials" to stored, in
// order), else ArduinoJson's name for why not (a static string), and neither is called.
const char* bb_store_wifi_credentials_read(const char* text, size_t len, bb_store_write last_connected,
                                           bb_store_wifi_stored_read stored, void* context);

#ifdef __cplusplus
}
#endif
