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

// The limits the store keeps to (WifiCredentialStore::MAX_NETWORKS, MAX_PASSWORD_LENGTH), Rust's
// bembeyaz_store::wifi_credentials::MOST_NETWORKS and MOST_PASSWORD.
#define BB_STORE_WIFI_MOST_NETWORKS 8
#define BB_STORE_WIFI_MOST_PASSWORD 64

// A network kept: its name and plain password.
typedef struct {
  bb_store_text ssid;
  bb_store_text password;
} bb_store_wifi_kept;

// bb_store_wifi_dropped's why.
enum {
  BB_STORE_WIFI_INVALID_LENGTH = 0,      // password_len is not an integer within uint32_t
  BB_STORE_WIFI_OVERSIZED_LENGTH = 1,    // password_len (length) is over the limit
  BB_STORE_WIFI_OVERSIZED_PASSWORD = 2,  // the password is longer than the limit
  BB_STORE_WIFI_CORRUPTED = 3,           // the password does not match its length or checksum
};

// bb_store_wifi_dropped's checksum, for BB_STORE_WIFI_CORRUPTED.
enum {
  BB_STORE_WIFI_CHECKSUM_FINE = 0,
  BB_STORE_WIFI_CHECKSUM_MISMATCH = 1,  // the CRC-32 does not match the password
  BB_STORE_WIFI_CHECKSUM_INVALID = 2,   // the stored CRC-32 is not an integer within uint32_t
};

// A network dropped, for the store's log: its name and why; for BB_STORE_WIFI_OVERSIZED_LENGTH the
// length stored (length); for BB_STORE_WIFI_CORRUPTED whether the length stored (length) differs
// from the one decoded (decoded), and what is wrong with the checksum.
typedef struct {
  bb_store_text ssid;
  uint8_t why;
  uint8_t checksum;
  bool length_mismatch;
  uint32_t length;
  size_t decoded;
} bb_store_wifi_dropped;

// What bb_store_wifi_credentials_keep calls, all given its context (a NULL callback is not called);
// each runs inside Rust: it must return normally (no exception, no longjmp). Texts are valid during
// the call only.
typedef struct {
  bb_store_decode decode;                                                // a stored password
  bb_store_write last_connected;                                         // the network connected last
  void (*kept)(const bb_store_wifi_kept* kept, void* context);           // each network kept
  void (*dropped)(const bb_store_wifi_dropped* dropped, void* context);  // each one dropped
} bb_store_wifi_keep_calls;

// A credential to save: the password obfuscated, and plain, for the length and CRC-32 Rust saves
// with it.
typedef struct {
  bb_store_text ssid;
  bb_store_text password_obf;
  bb_store_text password;
} bb_store_wifi_saved;

// Hands write the wifi.json text for last_connected_ssid and the count credentials at saved, in one
// call; false when it could not be made (memory ran out) and write was not called.
bool bb_store_wifi_credentials_write(bb_store_text last_connected_ssid, const bb_store_wifi_saved* saved, size_t count,
                                     bb_store_write write, void* context);
// Reads wifi.json's text (len bytes, read up to a NUL as a C string) and decides on it as the store
// does (bembeyaz_store::wifi_credentials::keep, within BB_STORE_WIFI_MOST_*): calls' dropped hears
// of each network dropped as it is judged, then last_connected of the network connected last and
// kept of each network kept, in the file's order; *resave is set (resave may be NULL). NULL when it
// was read, else ArduinoJson's name for why not (a static string; "NoMemory" when memory ran out,
// decoding included; "InvalidInput" when calls is NULL), and only decode and dropped may have been
// called. The plain passwords are wiped after the calls.
const char* bb_store_wifi_credentials_keep(const char* text, size_t len, const bb_store_wifi_keep_calls* calls,
                                           void* context, bool* resave);

#ifdef __cplusplus
}
#endif
