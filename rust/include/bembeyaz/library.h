// C ABI of rust/crates/bembeyaz-ffi/src/library.rs. Use lib/LibraryIndex/LibraryText.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Longest author key, in bytes (the index record's field).
#define BB_LIBRARY_AUTHOR_KEY_MAX_BYTES 12

// Each reads len bytes of text (NULL when len is 0) and passes its result to out once, valid only
// during the call. False, and out not called, when out of memory or out is NULL.
bool bb_library_fold(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                     void* context);
bool bb_library_clean_person_name(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                                  void* context);
bool bb_library_author_key(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                           void* context);
bool bb_library_surname_key(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                            void* context);

// The first letter of a folded key, or 0.
uint32_t bb_library_folded_group_initial(const char* folded, size_t len);
// Whether every word of needle prefixes a word of haystack (both folded).
bool bb_library_matches_query(const char* haystack, size_t haystack_len, const char* needle, size_t needle_len);

#ifdef __cplusplus
}
#endif
