// C ABI of rust/crates/bembeyaz-ffi/src/store_opds_servers.rs. Use src/OpdsServerStore.h instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// A server as stored: the password obfuscated, or a legacy plain password (read only; empty when
// written). The first header_count headers are the server's.
typedef struct {
  bb_store_text name;
  bb_store_text url;
  bb_store_text username;
  bb_store_text password_obf;
  bb_store_text password;
  bb_store_header headers[BB_STORE_HEADERS];
  size_t header_count;
} bb_store_opds_server;

// Receives one server read, and how many servers were read; its texts are valid during the call
// only. It runs inside Rust: it must return normally (no exception, no longjmp).
typedef void (*bb_store_opds_server_read)(const bb_store_opds_server* server, size_t count, void* context);

// Hands write the opds.json text for the count servers at servers, in one call; false when it could
// not be made (memory ran out) and write was not called.
bool bb_store_opds_servers_write(const bb_store_opds_server* servers, size_t count, bb_store_write write,
                                 void* context);
// Reads opds.json's text (len bytes, read up to a NUL as a C string), at most `most` servers. NULL
// when it was read (each server handed to server, in order), else ArduinoJson's name for why not
// (a static string), and server is not called.
const char* bb_store_opds_servers_read(const char* text, size_t len, size_t most, bb_store_opds_server_read server,
                                       void* context);

#ifdef __cplusplus
}
#endif
