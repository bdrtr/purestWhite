// C ABI of rust/crates/bembeyaz-ffi/src/path.rs. Use lib/FsHelpers/FsHelpers.h instead.
#pragma once

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// Both rewrite the len bytes at path in place and return the new length.
size_t bb_path_decode_uri_escapes(char* path, size_t len);
size_t bb_path_normalise(char* path, size_t len);

#ifdef __cplusplus
}
#endif
