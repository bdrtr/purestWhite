// C ABI of rust/crates/bembeyaz-ffi/src/path.rs. Use lib/FsHelpers/FsHelpers.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Both rewrite the len bytes at path in place and return the new length.
size_t bb_path_decode_uri_escapes(char* path, size_t len);
size_t bb_path_normalise(char* path, size_t len);

// Paths that arrive over the network (rust/crates/bembeyaz-path/src/web.rs). Use
// src/network/ProtectedPaths.h instead.

// "System Volume Information" or "XTCache", which the web interfaces never list or touch. Names are
// compared up to their first NUL.
bool bb_path_is_system_name(const char* name, size_t len);
// A name starting with a dot or a system name.
bool bb_path_is_hidden_name(const char* name, size_t len);
// Whether any component of path (all len bytes, split at '/') is a hidden name.
bool bb_path_has_hidden_component(const char* path, size_t len);
// Whether path (up to a NUL) reaches a credential store (/.crosspoint/wifi.json, opds.json,
// koreader.json, or a name they start), however it is spelled: "//", "." and ".." resolved, case
// ignored, names compared as SdFat opens them (leading spaces, trailing dots and spaces dropped),
// and an 8.3 alias ("CROSSP~1") in the first two components refused outright.
bool bb_path_is_sensitive(const char* path, size_t len);
// A path a plugin may name: absolute, not "/" alone, no ".." anywhere, not a credential store.
bool bb_path_is_plugin_path(const char* path, size_t len);
// Turns the len bytes at path into the absolute path a request names: normalised as above, with a
// leading '/' and no trailing one ("/" alone for the root). path holds len + 1 bytes; returns the
// new length.
size_t bb_path_web(char* path, size_t len);
// Where the path starts in a request URL ("http://host/path"); false when it names a host and no
// path. A value without "://" is a path already (start 0).
bool bb_path_url_path_start(const char* url, size_t len, size_t* start);
// Writes path into out as a URL path, with a space, '%', '#', '?', '&' and non-ASCII bytes as
// upper-case %XX, and returns the length. out holds 3 * len bytes and does not overlap path.
size_t bb_path_encode(const char* path, size_t len, char* out);

#ifdef __cplusplus
}
#endif
