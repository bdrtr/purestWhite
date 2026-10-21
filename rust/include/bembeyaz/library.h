// C ABI of rust/crates/bembeyaz-ffi/src/library.rs. Use lib/LibraryIndex/LibraryText.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Longest author key, in bytes (the index record's field).
#define BB_LIBRARY_AUTHOR_KEY_MAX_BYTES 12

// Each reads len bytes of text (NULL when len is 0) and passes its result to out once, valid only
// during the call. False, and out not called, when out of memory or out is NULL.
bool bb_library_fold(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                     void* context);
bool bb_library_clean_person_name(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                                  void* context);
bool bb_library_author_key(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                           void* context);
bool bb_library_surname_key(const char* text, size_t len, void (*out)(void* context, const char* s, size_t len),
                            void* context);

// The first letter of a folded key, or 0.
uint32_t bb_library_folded_group_initial(const char* folded, size_t len);
// Whether every word of needle prefixes a word of haystack (both folded).
bool bb_library_matches_query(const char* haystack, size_t haystack_len, const char* needle, size_t needle_len);

// The Library index file (CLX1): LibraryFormat.h's CLIX_FORMAT_VERSION, CLIX_FOLD_VERSION and
// CLIX_MAX_RECORDS.
#define BB_LIBRARY_FORMAT_VERSION 2
#define BB_LIBRARY_FOLD_VERSION 4
#define BB_LIBRARY_MAX_RECORDS 4096

// Strings bb_library_read_string reads.
enum {
  BB_LIBRARY_NAME = 0,
  BB_LIBRARY_AUTHOR = 1,
  BB_LIBRARY_TITLE = 2,
  BB_LIBRARY_SOURCE_AUTHOR = 3,
  BB_LIBRARY_PATH = 4,
};

// The index file (CLX1). The header, record and identity are lib/LibraryIndex/LibraryFormat.h's
// ClixHeader, ClixRecord and BookIdentity, passed as these opaque types.
typedef struct bb_library_header bb_library_header;
typedef struct bb_library_record bb_library_record;
typedef struct bb_library_identity bb_library_identity;

// The index file. read_at fills len bytes of buf from offset and returns false when it cannot (and
// counts the read as failed); fail marks the index failed without a read.
typedef struct {
  bool (*read_at)(void* context, uint32_t offset, void* buf, size_t len);
  void (*fail)(void* context);
  void* context;
} bb_library_index_file;

// Fills in every section offset and the expected file size from the counts.
void bb_library_layout_sections(bb_library_header* header, uint32_t folder_bytes, uint32_t name_bytes);
// ClixValidity's number for the header against the file's real size; structure_only skips the fold
// version.
uint8_t bb_library_validate_header(const bb_library_header* header, uint64_t file_size, bool structure_only);
// FNV-1a 64 of a complete path.
uint64_t bb_library_path_hash(const char* path, size_t len);

// The record ordinal at display row row in SortOrder order; 0xFFFF past the end or when unreadable.
uint16_t bb_library_ordinal_for_row(const bb_library_header* header, uint8_t order, uint16_t row,
                                    const bb_library_index_file* file);
// Reads and clamps record ordinal into out; false past the end, when unreadable, or for an unknown
// metadata status (out then holds the record with clamped lengths).
bool bb_library_read_record(const bb_library_header* header, uint16_t ordinal, const bb_library_index_file* file,
                            bb_library_record* out);
// The record's path hash; false (and the index failed) when it points outside the names, or when
// unreadable.
bool bb_library_read_path_hash(const bb_library_header* header, const bb_library_record* record,
                               const bb_library_index_file* file, uint64_t* out);
// what: one of BB_LIBRARY_NAME ... BB_LIBRARY_PATH (the absolute path). The string read (empty out
// of memory) goes to out either way; true when it is the field.
bool bb_library_read_string(const bb_library_header* header, const bb_library_record* record, uint8_t what,
                            const bb_library_index_file* file, void (*out)(void* context, const char* s, size_t len),
                            void* context);
// The RecentAsc row of each of count books (0xFFFF when absent; with more than 16 books none is
// looked up) into rows: 1
// when read, 0 when the index cannot be read, -1 out of memory.
int bb_library_recent_rows_for(const bb_library_header* header, const bb_library_identity* books, uint16_t* rows,
                               size_t count, const bb_library_index_file* file);

#ifdef __cplusplus
}
#endif
