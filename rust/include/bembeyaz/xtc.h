// C ABI of rust/crates/bembeyaz-ffi/src/xtc.rs. Use lib/Xtc/Xtc/XtcParser.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// What bb_xtc_open learned about an XTC or XTCH book.
typedef struct {
  uint64_t page_table_offset;
  uint64_t data_offset;
  uint16_t page_count;
  // The first page's size.
  uint16_t width;
  uint16_t height;
  // 1 for XTC, 2 for XTCH.
  uint8_t bit_depth;
  // The header says there are chapters and has room for their offset.
  bool has_chapters;
  uint8_t title_len;
  uint8_t author_len;
  // NUL-padded, not always terminated: use title_len and author_len.
  char title[127];
  char author[63];
} bb_xtc;

// The book file. read_at reads up to len bytes at offset into buf and returns how many, or -1 when
// the file cannot be opened or seeked to offset (a failed seek and a short read differ: the title
// and author fail on the first, not the second); size returns the file's size.
typedef struct {
  int64_t (*read_at)(void* context, uint64_t offset, uint8_t* buf, size_t len);
  uint64_t (*size)(void* context);
  void* context;
} bb_xtc_file;

// A page table entry.
typedef struct {
  uint64_t offset;
  uint32_t size;
  uint16_t width;
  uint16_t height;
} bb_xtc_page;

// The functions returning int return 0 or XtcError's number (6, a read error, for a NULL book,
// file or callback).

// Reads the header, the metadata and the first page's size into book (start it at 480 x 800, 1 bit:
// what a book reports before its first page is read); the fields read before an error are left set.
int bb_xtc_open(bb_xtc* book, const bb_xtc_file* file);
// Page index's table entry; false past the last page or when it cannot be read.
bool bb_xtc_page_info(const bb_xtc* book, const bb_xtc_file* file, uint32_t index, bb_xtc_page* out);
// Reads page index's bitmap into buf (len bytes; too small is a memory error) and sets loaded to
// its size, or 0 on an error.
int bb_xtc_load_page(const bb_xtc* book, const bb_xtc_file* file, uint32_t index, uint8_t* buf, size_t len,
                     size_t* loaded);
// Reads page index's bitmap through chunk (len bytes), handing each piece and its offset to
// on_chunk. A wrong page magic is a read error here.
int bb_xtc_stream_page(const bb_xtc* book, const bb_xtc_file* file, uint32_t index, uint8_t* chunk, size_t len,
                       void (*on_chunk)(void* context, const uint8_t* data, size_t size, size_t offset), void* context);
// Tells on_table (if not NULL) how many 96-byte records the chapter table has room for, then hands
// each chapter (name of len bytes, valid for the call; first and last page, 0-based and within the
// book) to on_chapter. listed is set when a chapter table was read, even an empty one;
// on an error the chapters already handed over stand.
int bb_xtc_chapters(const bb_xtc* book, const bb_xtc_file* file, void (*on_table)(void* context, size_t records),
                    void (*on_chapter)(void* context, const char* name, size_t len, uint16_t first, uint16_t last),
                    void* context, bool* listed);
// Whether bytes (len of them) start with an XTC or XTCH magic.
bool bb_xtc_is_book_magic(const uint8_t* bytes, size_t len);

#ifdef __cplusplus
}
#endif
