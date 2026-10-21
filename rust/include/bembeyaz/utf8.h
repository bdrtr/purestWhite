// C ABI of rust/crates/bembeyaz-ffi/src/utf8.rs. Use lib/Utf8/Utf8.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
  BB_UTF8_COMPOSE_UNCHANGED = 0,
  BB_UTF8_COMPOSE_DONE = 1,
  BB_UTF8_COMPOSE_ABORTED = 2,
};

uint32_t bb_utf8_next_codepoint(const unsigned char** string);
int bb_utf8_compose_nfc(const char* input, size_t len, bool (*on_absorbed)(uint32_t index, void* context),
                        void* absorbed_context, void (*emit)(const char* data, size_t len, void* context),
                        void* emit_context);
void bb_utf8_compose_nfc_in_place(char* buffer);
size_t bb_utf8_safe_truncate_len(const char* buffer, size_t len);
size_t bb_utf8_truncate_chars_len(const char* string, size_t len, size_t count);
// The uppercase partner of a lowercase Latin, Greek or Cyrillic letter, for synthesized small caps;
// other codepoints stay as they are.
uint32_t bb_utf8_small_caps_upper(uint32_t codepoint);

#ifdef __cplusplus
}
#endif
