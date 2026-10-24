// C ABI of rust/crates/bembeyaz-ffi/src/html.rs (rust/crates/bembeyaz-html).
// Use lib/Epub/Epub/htmlEntities.h and lib/Txt/TxtToHtml.h instead (ChapterHtmlSlimParser calls
// bb_html_scan_words, bb_html_element, bb_html_table_span and bb_html_internal_link directly).
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
// Bytes an initial letter holds (text has room for one more, the NUL); at most three codepoints.
#define BB_HTML_MAX_INITIAL_BYTES 12

// The initial letter (drop cap) being captured. A drop-cap span (span_depth >= 0) takes all of its
// text, a ::first-letter rule (pending) the block's leading punctuation and first letter; a letter
// that grows too long, or a span's letter that meets whitespace, goes back into the word and the
// whole state is reset (span_depth -1). lines and bold are the parser's.
typedef struct {
  char text[BB_HTML_MAX_INITIAL_BYTES + 1];
  uint8_t len;
  uint8_t codepoints;
  uint8_t lines;
  bool bold;
  bool pending;
  int32_t span_depth;
} bb_html_initial_letter;

// The chapter parser's word buffer and what it does with words. bb_html_scan_words reads and
// writes buf, *len, *offset and *continues between callbacks, which may change them too:
// flush hands the buffered word (from *offset, attached when *continues) to the paragraph, empties
// the buffer and clears *continues; suppress_indent drops the paragraph's first-line indent;
// line_break ends a preserved line, false when no paragraph is left; initial is the initial letter
// being captured, which bb_html_scan_words reads and writes too; block_empty tells whether the
// paragraph has no word yet.
typedef struct {
  char* buf;
  int* len;
  uint32_t* offset;
  bool* continues;
  void* context;
  void (*flush)(void* context);
  void (*suppress_indent)(void* context);
  bool (*line_break)(void* context);
  bb_html_initial_letter* initial;
  bool (*block_empty)(void* context);
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
// numbered from start. A pending initial letter (words->initial) takes its text before the words
// do. footnote_link: the text is a footnote link's, which never starts a ::first-letter letter.
// Returns a BB_HTML_WORDS_ code.
int bb_html_scan_words(const char* text, size_t len, bool counted, uint32_t start, bool preserve, bool footnote_link,
                       const bb_html_words* words);

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

// The most bytes a list marker takes: a sign, ten digits and a dot.
#define BB_HTML_MAX_MARKER 12

// An open list.
typedef struct {
  bool ordered;
  bool unmarked;    // list-style-type: none
  int32_t counter;  // the number of the last item, when ordered
} bb_html_list;

// Writes the marker of an item of list (NULL outside any list) to out (room for
// BB_HTML_MAX_MARKER bytes; no NUL) and returns its length: nothing when unmarked, a bullet
// (U+2022) when unordered, else the list's next number and a dot.
size_t bb_html_list_marker(bb_html_list* list, char* out);
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
