// C ABI of rust/crates/bembeyaz-ffi/src/html.rs (rust/crates/bembeyaz-html).
// Use lib/Epub/Epub/htmlEntities.h and lib/Txt/TxtToHtml.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// The UTF-8 value of the HTML entity name (len bytes, without '&' and ';'), as a static C string;
// NULL when there is none. Case-sensitive.
const char* bb_html_entity(const char* name, size_t len);

// Plain text (a .txt or .md book) being converted to an XHTML body; set it up with
// bb_text_html_init.
typedef struct {
  bool started;
  bool at_line_start;
  size_t pending_spaces;
} bb_text_html;

void bb_text_html_init(bb_text_html* state);
// Converts the next len bytes of text (fed in the pieces they are read in: a byte order mark is
// dropped from the first non-empty one), handing the XHTML to out in pieces valid only during the call.
void bb_text_html_feed(bb_text_html* state, const uint8_t* text, size_t len,
                       void (*out)(void* context, const uint8_t* data, size_t len), void* context);

#ifdef __cplusplus
}
#endif
