#pragma once

// A book's metadata cache, book.bin, and the two temporary files it is built from
// (rust/crates/bembeyaz-book, cache module): the records, the header and metadata, the LUT reads,
// the cumulative-size windows, writing book.bin, and finding a TOC href's spine item. lib/Epub's
// BookMetadataCache keeps the files and their buffers, the zip sizes, the index's sort, the
// locking and NFC titles.

#include <bembeyaz/io.h>

#ifdef __cplusplus
extern "C" {
#endif

#define BB_BOOK_WINDOW 32

// The string fields a read hands over.
enum {
  BB_BOOK_TITLE = 0,
  BB_BOOK_AUTHOR = 1,
  BB_BOOK_LANGUAGE = 2,
  BB_BOOK_COVER = 3,
  BB_BOOK_TEXT_REFERENCE = 4,
  BB_BOOK_HREF = 5,
  BB_BOOK_TOC_TITLE = 6,
  BB_BOOK_ANCHOR = 7,
  BB_BOOK_FIELD_COUNT = 8,
};

// A file of records, read through buf (buf_len bytes); count is how many records it has where a
// call says so (book.bin's spine items for bb_book_fill_window).
typedef struct {
  const bb_io_file* file;
  uint8_t* buf;
  size_t buf_len;
  uint16_t count;
} bb_book_records;

// Holds what a read finds: text gives room for a string field's len bytes (only for one that is
// not empty), NULL when it has none.
typedef struct {
  void* context;
  char* (*text)(void* context, uint8_t field, size_t len);
} bb_book_target;

typedef struct {
  uint32_t lut;
  uint16_t spine_count;
  uint16_t toc_count;
} bb_book_header;

enum {
  BB_BOOK_LOADED = 0,
  BB_BOOK_OTHER_VERSION = 1,
  // The header or the metadata cannot be read, or there is no room for it.
  BB_BOOK_DAMAGED = 2,
};

// Reads book.bin's header and metadata (into target). Returns a BB_BOOK_ code; on
// BB_BOOK_OTHER_VERSION, *version is the version found.
int bb_book_load(const bb_book_records* book, const bb_book_target* target, bb_book_header* out, uint8_t* version);
// Reads the cumulative size before each window of spine items into starts (count: the spine count
// divided by BB_BOOK_WINDOW, rounded up). Returns the book's size, the last item's. A spine record
// that cannot be read gives zero sizes from it on.
uint32_t bb_book_window_starts(const bb_book_records* book, const bb_book_header* header, uint32_t* starts,
                               size_t count);
// Spine item index (its href into target): its cumulative size and TOC entry. A field that cannot
// be read reads as empty or zero.
void bb_book_spine_entry(const bb_book_records* book, uint32_t lut, uint16_t index, const bb_book_target* target,
                         uint32_t* cumulative, int16_t* toc);
// TOC entry index (its title, href and anchor into target): its level and spine item. As above.
void bb_book_toc_entry(const bb_book_records* book, uint32_t lut, uint16_t spine_count, uint16_t index,
                       const bb_book_target* target, uint8_t* level, int16_t* spine);
// The cumulative sizes of the window of spine items from first into window (book's count is the
// spine count); false when any of them cannot be read.
bool bb_book_fill_window(const bb_book_records* book, uint32_t lut, uint16_t first, uint32_t window[BB_BOOK_WINDOW]);

// A loaded book's cumulative sizes in memory: the size before each window, and one window
// (window_first is its first item, -1 for none) that fill reads from the file.
typedef struct {
  uint16_t spine_count;
  uint32_t book_size;
  const uint32_t* starts;
  size_t start_count;
  int32_t* window_first;
  uint32_t* window;
} bb_book_sizes;

typedef bool (*bb_book_fill)(void* context, int32_t first, uint32_t window[BB_BOOK_WINDOW]);
// The cumulative size up to and including spine item index (0 out of range); a window that
// cannot be read answers the total before it.
uint32_t bb_book_cumulative_size(const bb_book_sizes* sizes, int32_t index, void* context, bb_book_fill fill);
// The first spine item whose cumulative size reaches size (-1 past the book or without items);
// a window that cannot be read answers its last item.
int32_t bb_book_index_for_size(const bb_book_sizes* sizes, uint32_t size, void* context, bb_book_fill fill);

// The temporary files' records, written at *at, which moves past them; false when a write failed.
bool bb_book_write_spine_record(const bb_io_sink* sink, uint32_t* at, const char* href, size_t href_len,
                                uint32_t cumulative, int16_t toc);
bool bb_book_write_toc_record(const bb_io_sink* sink, uint32_t* at, const char* title, size_t title_len,
                              const char* href, size_t href_len, const char* anchor, size_t anchor_len, uint8_t level,
                              int16_t spine);

// A spine href in the hash index.
typedef struct {
  uint64_t hash;
  uint16_t len;
  int16_t spine;
} bb_book_index_entry;

// Fills one entry per record of the temporary spine file with its href's FNV-1a hash and length
// (for bb_book_find_in_index once sorted by hash, then length). False when a record cannot be
// read.
bool bb_book_index_spine(const bb_book_records* spine, bb_book_index_entry* entries);
// The spine item of href in the sorted entries, -1 for none.
int16_t bb_book_find_in_index(const bb_book_index_entry* entries, size_t count, const char* href, size_t len);
// The first record of the temporary spine file whose href is href, -1 for none.
int16_t bb_book_find_spine(const bb_book_records* spine, const char* href, size_t len);

// The spine items as their records are read: href gives room for the next href's len bytes (also
// when 0; NULL when it has none), then size gives spine item index's size, its href in that room.
typedef struct {
  void* context;
  char* (*href)(void* context, size_t len);
  uint32_t (*size)(void* context, uint16_t index);
} bb_book_items;

// Hands each record of the temporary spine file to items (its answer to size is not used), in
// order. False when a record cannot be read or there is no room.
bool bb_book_spine_hrefs(const bb_book_records* spine, const bb_book_items* items);

enum {
  BB_BOOK_BUILT = 0,
  // A temporary file's record cannot be read.
  BB_BOOK_TEMP_DAMAGED = 1,
  // No room for an href (or, from the caller, for the working room).
  BB_BOOK_NO_ROOM = 2,
  BB_BOOK_WRITE_FAILED = 3,
};

// Writes book.bin to out, from offset 0, from the two temporary files: the header and the five
// metadata strings, the LUTs, the spine records with their cumulative sizes (from items) and TOC
// entries, and the TOC records. toc_map (one per spine item) is working room. Returns a BB_BOOK_
// code.
int bb_book_build(const bb_book_records* spine, const bb_book_records* toc, const bb_io_sink* out,
                  const char* const metadata[5], const size_t metadata_len[5], int16_t* toc_map,
                  const bb_book_items* items);

#ifdef __cplusplus
}
#endif
