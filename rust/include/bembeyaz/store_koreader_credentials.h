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

// KOReaderServerType, DocumentMatchMethod and KOReaderSyncBehavior's values.
#define BB_STORE_KOREADER_SERVER_CROSSPOINT 0
#define BB_STORE_KOREADER_SERVER_KOSYNC 1
#define BB_STORE_KOREADER_SERVER_OTHER 2
#define BB_STORE_KOREADER_MATCH_FILENAME 0
#define BB_STORE_KOREADER_MATCH_BINARY 1
#define BB_STORE_KOREADER_SYNC_ASK_EVERY_TIME 0
#define BB_STORE_KOREADER_SYNC_SMART 1

// What the store makes of a koreader.json file (bembeyaz_store::koreader_settings): its settings,
// the password and header values decoded; the file's first header_count headers (the slots past
// them are empty here; the store keeps what its own held); whether to save it again; and, for the
// debug log, whether the old default server was pinned, and each setting reset because the value
// read (kept here) was out of range (0 when it was not reset).
typedef struct {
  bb_store_text username;
  bb_store_text password;
  bb_store_text server_url;
  bb_store_plain_header headers[BB_STORE_HEADERS];
  size_t header_count;
  uint8_t server_type;
  uint8_t match_method;
  uint8_t sync_behavior;
  bool send_metadata;
  bool sync_clippings;
  bool resave;
  bool pinned_legacy_url;
  uint8_t server_type_reset;
  uint8_t match_method_reset;
  uint8_t sync_behavior_reset;
} bb_store_koreader_settings;

// What the store saves: the password and the header values obfuscated; the first header_count
// headers. Rust stamps it with the current config version.
typedef struct {
  bb_store_text username;
  bb_store_text password_obf;
  bb_store_text server_url;
  bb_store_header headers[BB_STORE_HEADERS];
  size_t header_count;
  uint8_t server_type;
  uint8_t match_method;
  uint8_t sync_behavior;
  bool send_metadata;
  bool sync_clippings;
} bb_store_koreader_saved;

// Receives the settings; their texts are valid during the call only, and the plain ones are wiped
// after it. It runs inside Rust: it must return normally (no exception, no longjmp).
typedef void (*bb_store_koreader_settled)(const bb_store_koreader_settings* settings, void* context);

// Hands write the koreader.json text for saved, in one call; false when it could not be made
// (memory ran out) and write was not called.
bool bb_store_koreader_credentials_write(const bb_store_koreader_saved* saved, bb_store_write write, void* context);
// Reads koreader.json's text (len bytes, read up to a NUL as a C string) and hands settled the
// settings it gives, its secrets decoded with decode (both given context). NULL when it was read,
// else ArduinoJson's name for why not (a static string; "NoMemory" when memory ran out, decoding
// included), and settled is not called.
const char* bb_store_koreader_credentials_settle(const char* text, size_t len, bb_store_decode decode,
                                                 bb_store_koreader_settled settled, void* context);
// Hands write the base URL for a server URL (len bytes; getBaseUrl), in one or two pieces: the
// default server when it is empty, http:// put before one with no "://" in it, the trailing slashes
// taken off.
void bb_store_koreader_base_url(const char* url, size_t len, bb_store_write write, void* context);

#ifdef __cplusplus
}
#endif
