// C ABI of rust/crates/bembeyaz-ffi/src/chapter.rs: the chapter's anchors (which page each element
// id lands on), as the chapter parser holds them (bb_chapter_anchors). Use
// lib/Epub/Epub/parsers/ChapterHtmlSlimParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A note reference's number and target keep fewer bytes than these (FootnoteEntry's arrays).
#define BB_FOOTNOTE_NUMBER_LEN 32
#define BB_FOOTNOTE_HREF_LEN 256

typedef struct bb_anchors bb_anchors;

// The anchors placed, in order. bb_anchors_get returns the i-th's name (followed by a NUL; valid
// until the anchors change) and writes its length and page, or returns NULL past the end.
size_t bb_anchors_len(const bb_anchors* anchors);
const char* bb_anchors_get(const bb_anchors* anchors, size_t i, size_t* len, uint16_t* page);
// The page of the first anchor placed named name; false when there is none.
bool bb_anchors_find(const bb_anchors* anchors, const char* name, size_t len, uint16_t* page);

#ifdef __cplusplus
}
#endif
