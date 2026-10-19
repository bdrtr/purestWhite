// C ABI of rust/crates/bembeyaz-ffi/src/hyphenation.rs. Use lib/Epub/Epub/hyphenation/Hyphenator.h
// instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The patterns for a BCP 47 or ISO 639-2 tag ("en", "en-US", "ger"); -1 when there are none.
// The number holds for this build only: never store it.
int bb_hyph_language(const char* tag, size_t len);

// Calls emit for each place word (len bytes, or up to a NUL) may break, in increasing offset:
// where the text after the break starts, and whether a hyphen must be drawn there. language is
// from bb_hyph_language; -1 uses no patterns. fallback adds every break the minimum prefix and
// suffix allow where the patterns find none (per part around apostrophes, and then only with a
// language; never for a word with explicit hyphens). A NULL emit does nothing.
void bb_hyph_breaks(int language, const char* word, size_t len, bool fallback,
                    void (*emit)(void* context, size_t offset, bool inserted_hyphen), void* context);

// Hyphens and dashes a word may already break at (and the underscore and ellipsis).
bool bb_hyph_is_explicit_hyphen(uint32_t cp);

#ifdef __cplusplus
}
#endif
