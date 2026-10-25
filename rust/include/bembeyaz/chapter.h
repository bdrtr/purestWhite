// C ABI of rust/crates/bembeyaz-ffi/src/chapter.rs: the chapter parser's anchors (which page each
// element id lands on) and note references waiting for their page. Use
// lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <bembeyaz/pager.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Ids beyond this many are not recorded, except the table of contents' ones.
#define BB_ANCHORS_MAX 1024
// A reference's number and target keep fewer bytes than these (FootnoteEntry's arrays).
#define BB_FOOTNOTE_NUMBER_LEN 32
#define BB_FOOTNOTE_HREF_LEN 256

// The anchors and the note references (bembeyaz-chapter's). Rust owns the heap blocks they point
// to: set each up once with its init, release them with its free before the storage goes away,
// and never copy it.
typedef struct {
  uint64_t opaque[14];
} bb_anchors;
typedef struct {
  uint64_t opaque[14];
} bb_footnotes;

void bb_anchors_init(bb_anchors* anchors);
// Frees the anchors; the store stays usable, empty.
void bb_anchors_free(bb_anchors* anchors);
// Adds an id the table of contents names; false when it cannot be stored.
bool bb_anchors_add_toc(bb_anchors* anchors, const char* name, size_t len);
// An element with id name opened. Ids on elements that cannot be link targets (not navigable) and
// ids past BB_ANCHORS_MAX are dropped, unless the table of contents names them. One still waiting
// is placed first. False when it cannot be stored. The page callbacks must not use the anchors.
bool bb_anchors_open(bb_anchors* anchors, bb_pager* pager, const bb_pages* pages, const char* name, size_t len,
                     bool navigable);
// The block before the waiting id is on a page: it lands on the page now current, a new one if the
// table of contents starts a chapter there. False when it cannot be stored.
bool bb_anchors_place_pending(bb_anchors* anchors, bb_pager* pager, const bb_pages* pages);
// The waiting id, if any, lands on page; false when it cannot be stored.
bool bb_anchors_record_pending(bb_anchors* anchors, uint16_t page);
// The anchors placed, in order. bb_anchors_get returns the i-th's name (followed by a NUL; valid
// until the anchors change) and writes its length and page, or returns NULL past the end.
size_t bb_anchors_len(const bb_anchors* anchors);
const char* bb_anchors_get(const bb_anchors* anchors, size_t i, size_t* len, uint16_t* page);
// The page of the first anchor placed named name; false when there is none.
bool bb_anchors_find(const bb_anchors* anchors, const char* name, size_t len, uint16_t* page);

void bb_footnotes_init(bb_footnotes* notes);
// Frees the references; the store stays usable, empty.
void bb_footnotes_free(bb_footnotes* notes);
// A reference opens with target href (len bytes): empty, or one the block keeps (fewer than
// BB_FOOTNOTE_HREF_LEN bytes). False when the target cannot be stored; the link then has none.
bool bb_footnotes_open(bb_footnotes* notes, const char* href, size_t len);
// The open reference's target, or "" (valid until the next bb_footnotes_open).
const char* bb_footnotes_href(const bb_footnotes* notes);
// Text inside the open reference: its number, without the spaces and square brackets around each
// piece, up to BB_FOOTNOTE_NUMBER_LEN - 1 bytes.
void bb_footnotes_text(bb_footnotes* notes, const char* text, size_t len);
// The open reference closes after the block's first word words: it waits for its page if it has a
// number and a target. False when it cannot be stored.
bool bb_footnotes_close(bb_footnotes* notes, int32_t word);
// Calls add with each waiting reference's number and target (NUL-terminated) that waits for no
// more than words of its block's words, in order, and forgets them. add must not use the notes.
void bb_footnotes_take(bb_footnotes* notes, int32_t words, void (*add)(void* context, const char* number,
                                                                         const char* href),
                       void* context);
bool bb_footnotes_has_pending(const bb_footnotes* notes);

#ifdef __cplusplus
}
#endif
