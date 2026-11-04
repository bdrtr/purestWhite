// C ABI of rust/crates/bembeyaz-ffi/src/store_opds_servers.rs. Use src/OpdsServerStore.h instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// How many servers the store keeps (OpdsServerStore::MAX_SERVERS), Rust's
// bembeyaz_store::opds_servers::MOST_SERVERS.
#define BB_STORE_OPDS_MOST_SERVERS 8

// A server to save: the password and header values obfuscated; the first header_count headers.
typedef struct {
  bb_store_text name;
  bb_store_text url;
  bb_store_text username;
  bb_store_text password_obf;
  bb_store_header headers[BB_STORE_HEADERS];
  size_t header_count;
} bb_store_opds_server;

// A server as the store holds it after loading the file: the password and header values plain; the
// first header_count headers (the slots past them empty).
typedef struct {
  bb_store_text name;
  bb_store_text url;
  bb_store_text username;
  bb_store_text password;
  bb_store_plain_header headers[BB_STORE_HEADERS];
  size_t header_count;
} bb_store_opds_loaded;

// Receives one server loaded, and how many there are; its texts are valid during the call only.
// It runs inside Rust: it must return normally (no exception, no longjmp).
typedef void (*bb_store_opds_loaded_server)(const bb_store_opds_loaded* server, size_t count, void* context);

// Hands write the opds.json text for the count servers at servers, in one call; false when it could
// not be made (memory ran out) and write was not called.
bool bb_store_opds_servers_write(const bb_store_opds_server* servers, size_t count, bb_store_write write,
                                 void* context);
// Reads opds.json's text (len bytes, read up to a NUL as a C string) and makes what the store holds
// of it (bembeyaz_store::opds_servers::load): at most BB_STORE_OPDS_MOST_SERVERS servers, each
// password decoded with decode (else a legacy plain one, which sets *resave), each header value
// decoded; each server handed to server, in order (decode and server both given context); *resave
// set on success (resave may be NULL). NULL when it was read, else ArduinoJson's name for why not
// (a static string; "NoMemory" when memory ran out, decoding included), and only decode may have
// been called. The plain secrets are wiped after the calls.
const char* bb_store_opds_servers_load(const char* text, size_t len, bb_store_decode decode,
                                       bb_store_opds_loaded_server server, void* context, bool* resave);

#ifdef __cplusplus
}
#endif
