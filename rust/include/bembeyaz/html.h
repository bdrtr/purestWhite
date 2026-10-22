// C ABI of rust/crates/bembeyaz-ffi/src/html.rs (rust/crates/bembeyaz-html).
// Use lib/Epub/Epub/htmlEntities.h and lib/Txt/TxtToHtml.h instead (ChapterHtmlSlimParser calls
// bb_html_scan_words directly).
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

// Bytes a chapter parser's word buffer holds before a word is cut (buf has room for one more, the
// NUL).
#define BB_HTML_MAX_WORD 200

// The chapter parser's word buffer and what it does with words. bb_html_scan_words reads and
// writes buf, *len, *offset and *continues between callbacks, which may change them too:
// flush hands the buffered word (from *offset, attached when *continues) to the paragraph, empties
// the buffer and clears *continues; suppress_indent drops the paragraph's first-line indent;
// line_break ends a preserved line, false when no paragraph is left; initial offers rest (len
// bytes, the text from the current byte) to a pending initial letter: the bytes it took, 0 for
// none, -1 when none is pending any more.
typedef struct {
  char* buf;
  int* len;
  uint32_t* offset;
  bool* continues;
  void* context;
  void (*flush)(void* context);
  void (*suppress_indent)(void* context);
  bool (*line_break)(void* context);
  int (*initial)(void* context, const char* rest, size_t len);
} bb_html_words;

enum {
  BB_HTML_WORDS_DONE = 0,
  // A line break left no paragraph.
  BB_HTML_WORDS_STOPPED = 1,
};

// Cuts len bytes of a chapter's text into words: whitespace ends a word; a no-break space (U+00A0,
// U+202F) is a space word attached to both neighbours; a BOM is dropped; a full buffer is cut on a
// codepoint boundary and the rest attaches. preserve (white-space: pre) keeps each space as a word,
// a tab as four and a newline as a line break. counted: the text is visible, its codepoints
// numbered from start. initial: an initial letter may be pending (false spares the per-byte initial
// call). Returns a BB_HTML_WORDS_ code.
int bb_html_scan_words(const char* text, size_t len, bool counted, uint32_t start, bool preserve, bool initial,
                       const bb_html_words* words);

#ifdef __cplusplus
}
#endif
