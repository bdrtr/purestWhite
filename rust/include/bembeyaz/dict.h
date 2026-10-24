// C ABI of rust/crates/bembeyaz-ffi/src/dict.rs. Use src/util/Dictionary instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// StarDict dictionaries: sampled-offset sidecars (.qidx over .idx, .sidx over .syn: the offset of
// every 256th entry, after a 24-byte header) and the lookups that bisect them.
#define BB_DICT_QIDX_MAGIC 0x58444951u
#define BB_DICT_SIDX_MAGIC 0x58444953u
#define BB_DICT_HEADER_LEN 24
// Words longer than this are cut (their rest skipped); a headword buffer holds it and a NUL.
#define BB_DICT_WORD_MAX 255
// The fixed bytes after an entry's word: in .idx the definition's offset and size (big-endian), in
// .syn the ordinal of its .idx entry.
#define BB_DICT_IDX_SUFFIX 8u
#define BB_DICT_SYN_SUFFIX 4u

// Reads len bytes of file from pos into buf: how many (0 at the end), -1 when seeking to pos fails
// (past the end, or a broken cluster chain), or -2 when the read fails.
typedef int32_t (*bb_dict_read_fn)(void* file, uint32_t pos, uint8_t* buf, size_t len);
typedef struct {
  bb_dict_read_fn read;
  void* file;
} bb_dict_source;

// An index (.idx or .syn) and its sidecar, with the source's size, the sidecar's sample count (0
// without a usable sidecar: lookups scan from the start) and its entry count (0 if unknown).
typedef struct {
  bb_dict_source source;
  bb_dict_source sidecar;
  uint32_t source_size;
  uint32_t sample_count;
  uint32_t entry_count;
} bb_dict_index;

// Where a definition is in the .dict data; read_error when the seek to where the scan starts failed.
typedef struct {
  uint32_t offset;
  uint32_t size;
  bool found;
  bool read_error;
} bb_dict_location;

// The facts read from the start of an .ifo (as a C string): idxoffsetbits=64, and
// sametypesequence=h or x.
void bb_dict_ifo(const uint8_t* text, size_t len, bool* offsets64, bool* html);

// A sidecar header's counts, when it has magic, the current version and the sample interval.
bool bb_dict_header(const uint8_t raw[BB_DICT_HEADER_LEN], uint32_t magic, uint32_t* samples, uint32_t* size,
                    uint32_t* entries);
void bb_dict_header_bytes(uint32_t magic, uint32_t samples, uint32_t size, uint32_t entries,
                          uint8_t out[BB_DICT_HEADER_LEN]);

// A sidecar's scan of its source (entries: a NUL-terminated word and suffix bytes), as storage
// that bb_dict_scan_init sets up; it holds no memory of its own.
typedef struct {
  uint32_t opaque[6];
} bb_dict_scan;
void bb_dict_scan_init(bb_dict_scan* scan, uint32_t suffix, uint32_t source_size);
// Reads the next bytes of the source, handing sample each sampled entry's offset (after the
// first, at 0, which the caller writes); false as soon as sample is.
bool bb_dict_scan_feed(bb_dict_scan* scan, const uint8_t* bytes, size_t len,
                       bool (*sample)(void* context, uint32_t offset), void* context);
// Entries seen, and samples taken (the first included).
void bb_dict_scan_counts(const bb_dict_scan* scan, uint32_t* entries, uint32_t* samples);

// Looks target (len bytes, ASCII case aside) up in an .idx, reading words into headword
// (BB_DICT_WORD_MAX + 1 bytes; NULL finds nothing): after a hit it holds the headword as stored,
// NUL-terminated.
void bb_dict_locate(const bb_dict_index* idx, const char* target, size_t len, char* headword, bb_dict_location* out);
// Looks target up in a .syn and follows a hit's ordinal into idx.
void bb_dict_locate_synonym(const bb_dict_index* syn, const bb_dict_index* idx, const char* target, size_t len,
                            char* headword, bb_dict_location* out);

// Writes word (len bytes) without the punctuation around it, ASCII lower-cased, to out (out_len
// bytes); returns its length.
size_t bb_dict_clean_word(const char* word, size_t len, char* out, size_t out_len);
// Hands variant each English stem variant of word (its first stem bytes, then ending), in the
// order to try them, without repeats.
void bb_dict_stem_variants(const char* word, size_t len,
                           void (*variant)(void* context, size_t stem, const char* ending, size_t ending_len),
                           void* context);

// Dictzip (.dict.dz) files: the gzip header's RA chunk table, and which chunks a range of the
// text needs. Results:
enum {
  BB_DICTZIP_OK = 0,
  BB_DICTZIP_READ = 1,     // a read or seek failed, the file ended, or the range is outside the text
  BB_DICTZIP_CORRUPT = 2,  // not dictzip, or a table that does not add up
  BB_DICTZIP_MEMORY = 3,   // no room for the chunk table
};

// A file read in order: read fills len bytes (returning how many, negative on failure), seek and
// position as HalFile's; size is its size.
typedef struct {
  int32_t (*read)(void* file, uint8_t* buf, size_t len);
  bool (*seek)(void* file, uint32_t pos);
  uint32_t (*position)(void* file);
  uint32_t size;
  void* file;
} bb_dictzip_file;

typedef struct {
  uint32_t data_offset;  // where the first chunk starts
  uint32_t total_size;   // the text's size (ISIZE)
  uint16_t chunk_length;
} bb_dictzip_header;

// Reads the header from the file's start, asking table for room for chunk_count + 1 offsets (NULL:
// no memory) and filling it with each chunk's compressed offset from the first.
uint8_t bb_dictzip_parse(const bb_dictzip_file* file, uint32_t* (*table)(void* context, size_t len), void* context,
                         bb_dictzip_header* out);
// Hands slice, in order, each chunk slice that size bytes of the text from offset need: the chunk's
// compressed bytes (offset from the file's start, size), inflated, skipping skip and taking take.
// slice returns a BB_DICTZIP_* result; a failure stops it and is returned.
uint8_t bb_dictzip_slices(const bb_dictzip_header* header, const uint32_t* table, size_t len, uint32_t offset,
                          uint32_t size,
                          uint8_t (*slice)(void* context, uint32_t offset, uint32_t size, uint32_t skip, uint32_t take),
                          void* context);

#ifdef __cplusplus
}
#endif
