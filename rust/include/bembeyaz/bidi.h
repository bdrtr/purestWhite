// C ABI of rust/crates/bembeyaz-ffi/src/bidi.rs. Use lib/MiniBidi/BidiUtils.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Characters in one line; longer text is left as it is.
#define BB_BIDI_MAX_LINE 128
// Bytes of UTF-8 bb_bidi_visual() writes at most.
#define BB_BIDI_OUT_SIZE (4 * BB_BIDI_MAX_LINE)

enum {
  BB_BIDI_DONE = 0,
  BB_BIDI_EMPTY = 1,
  BB_BIDI_TOO_LONG = 2,
};

// Storage for one line (1536 bytes). Zero it before first use (a static is); one call at a time.
typedef struct {
  uint32_t opaque[384];
} bb_bidi_workspace;

bool bb_bidi_is_transparent_mark(uint32_t cp);
int bb_bidi_paragraph_level(const char* text, size_t len, int fallback, int max_strong);
int bb_bidi_visual(const char* text, size_t len, int level, bb_bidi_workspace* workspace, char* out,
                   size_t* out_len);
bool bb_bidi_word_order(size_t count, const char* (*word)(size_t index, size_t* len, const void* context),
                        const void* word_context, bool rtl, bb_bidi_workspace* workspace, uint16_t* order);

#ifdef __cplusplus
}
#endif
