// C ABI of rust/crates/bembeyaz-ffi/src/store_bookmarks.rs. Use src/util/BookmarkFile.h instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A bookmark (BookmarkEntry's fields): an empty name is none, and visible_text_offset counts only
// when has_visible_text_offset is set.
typedef struct {
  bb_store_text xpath;
  bb_store_text summary;
  bb_store_text name;
  float percentage;
  uint16_t spine_index;
  uint16_t chapter_page_count;
  uint16_t chapter_progress;
  bool has_visible_text_offset;
  uint32_t visible_text_offset;
} bb_store_bookmark;

// Receives one bookmark read, and how many were read; its texts are valid during the call only.
// It runs inside Rust: it must return normally (no exception, no longjmp).
typedef void (*bb_store_bookmark_read)(const bb_store_bookmark* bookmark, size_t count, void* context);

// Hands write a bookmarks file's text for the count bookmarks at bookmarks, in one call; false
// when it could not be made (memory ran out) and write was not called.
bool bb_store_bookmarks_write(const bb_store_bookmark* bookmarks, size_t count, bb_store_write write, void* context);
// Reads a bookmarks file's text (len bytes, read up to a NUL as a C string), each field with its
// plain fallback and the name not yet cut to MAX_NAME_LENGTH. NULL when it was read (each bookmark handed to bookmark,
// in order), else ArduinoJson's name for why not (a static string), and bookmark is not called.
const char* bb_store_bookmarks_read(const char* text, size_t len, bb_store_bookmark_read bookmark, void* context);

#ifdef __cplusplus
}
#endif
