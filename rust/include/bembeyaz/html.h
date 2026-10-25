// C ABI of rust/crates/bembeyaz-ffi/src/html.rs (rust/crates/bembeyaz-html).
// Use lib/Epub/Epub/htmlEntities.h and lib/Txt/TxtToHtml.h instead (the chapter parser's host tests
// call bb_html_element, bb_html_table_span, bb_html_internal_link and bb_html_collapse_space directly).
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

// An element's kind, from bb_html_element: the heading level (1 to 6 for h1 to h6, else 0),
// flags and a name. Names match exactly and case-sensitively, except that BB_HTML_HIDDEN takes
// any case.
enum {
  BB_HTML_HEADING_LEVEL = 0x7,
  BB_HTML_BLOCK = 1 << 3,         // p, li, div, br, blockquote, ul, ol, pre, figure
  BB_HTML_BOLD = 1 << 4,          // b, strong
  BB_HTML_ITALIC = 1 << 5,        // i, em
  BB_HTML_UNDERLINE = 1 << 6,     // u, ins
  BB_HTML_LINE_THROUGH = 1 << 7,  // del, s, strike
  BB_HTML_IMAGE = 1 << 8,         // img, image
  BB_HTML_TABLE = 1 << 9,         // table, tr, td, th
  BB_HTML_HIDDEN = 1 << 10,       // head, style, script, title, rp: text not shown
};

// The bits of a kind that name the element, for the names the parser checks one by one (0 for
// others), and those names.
enum {
  BB_HTML_NAME = 0x1f << 11,
  BB_HTML_TAG_A = 1 << 11,
  BB_HTML_TAG_HR = 2 << 11,
  BB_HTML_TAG_SPAN = 3 << 11,
  BB_HTML_TAG_SUP = 4 << 11,
  BB_HTML_TAG_SUB = 5 << 11,
  BB_HTML_TAG_RUBY = 6 << 11,
  BB_HTML_TAG_RT = 7 << 11,
  BB_HTML_TAG_HTML = 8 << 11,
  BB_HTML_TAG_BODY = 9 << 11,
  BB_HTML_TAG_LI = 10 << 11,
  BB_HTML_TAG_OL = 11 << 11,
  BB_HTML_TAG_UL = 12 << 11,
  BB_HTML_TAG_P = 13 << 11,
  BB_HTML_TAG_PRE = 14 << 11,
  BB_HTML_TAG_BR = 15 << 11,
  BB_HTML_TAG_TABLE = 16 << 11,
  BB_HTML_TAG_TR = 17 << 11,
  BB_HTML_TAG_TD = 18 << 11,
  BB_HTML_TAG_TH = 19 << 11,
};

// The kind of the element named by the len bytes at name.
uint16_t bb_html_element(const char* name, size_t len);
// A colspan or rowspan value of len bytes: digits only, else 1; 0, or digits passing UINT16_MAX
// before any other byte, mean the rest of the table (UINT16_MAX).
uint16_t bb_html_table_span(const char* value, size_t len);
// Whether the len-byte href points inside the book: not empty, and not starting with http://,
// https://, mailto:, ftp://, tel: or javascript: (case-sensitive).
bool bb_html_internal_link(const char* href, size_t len);

// Trims the len bytes of text of ASCII whitespace and collapses each run inside it to one space,
// in place; returns the new length.
size_t bb_html_collapse_space(char* text, size_t len);

// Writes a StarDict HTML definition (tag soup, len bytes) as a well-formed XHTML document to out in
// pieces: wrapped in <html><body>, tag names lower-cased (cut at 15 characters), void elements
// self-closed and their closers dropped, comments, doctypes and processing instructions dropped, and
// '<' and '&' outside markup or an entity reference escaped. False as soon as out is.
bool bb_html_normalize_xhtml(const char* html, size_t len, bool (*out)(void* context, const char* piece, size_t len),
                             void* context);

// Writes an HTML fragment (len bytes) as plain text into out (out_len bytes; len always suffices)
// and returns its length: tags dropped, p, h1-h6 and hr ending a paragraph and br, div, li and tr a
// line (breaks never stacking), tabs and CRs as single spaces, entities decoded, trailing spaces and
// line ends dropped.
size_t bb_html_to_plain(const char* html, size_t len, char* out, size_t out_len);

#ifdef __cplusplus
}
#endif
