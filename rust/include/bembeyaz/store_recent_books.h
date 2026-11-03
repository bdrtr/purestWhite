// C ABI of rust/crates/bembeyaz-ffi/src/store_recent_books.rs. Use src/RecentBooksStore.h instead.
#pragma once

#include <bembeyaz/store.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

// A recent book: its file, title, author and cover image path.
typedef struct {
  bb_store_text path;
  bb_store_text title;
  bb_store_text author;
  bb_store_text cover_bmp_path;
} bb_store_recent_book;

// Receives one book read, and how many books were read; its texts are valid during the call only.
// It runs inside Rust: it must return normally (no exception, no longjmp).
typedef void (*bb_store_recent_book_read)(const bb_store_recent_book* book, size_t count, void* context);

// Hands write the recent.json text for the count books at books, in one call; false when it could
// not be made (memory ran out) and write was not called.
bool bb_store_recent_books_write(const bb_store_recent_book* books, size_t count, bb_store_write write, void* context);
// Reads recent.json's text (len bytes, read up to a NUL as a C string), at most `most` books. NULL
// when it was read (each book handed to book, in order), else ArduinoJson's name for why not (a
// static string), and book is not called.
const char* bb_store_recent_books_read(const char* text, size_t len, size_t most, bb_store_recent_book_read book,
                                       void* context);

#ifdef __cplusplus
}
#endif
