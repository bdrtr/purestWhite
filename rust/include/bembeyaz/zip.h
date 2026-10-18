// C ABI of rust/crates/bembeyaz-ffi/src/zip.rs. Use lib/ZipFile/ZipFile.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Status codes: BB_ZIP_OK, BB_ZIP_NOT_FOUND, or a negative error
// (bb_zip_status_str describes it).
enum {
  BB_ZIP_OK = 0,
  BB_ZIP_NOT_FOUND = 1,
};

typedef struct bb_zip_source {
  void* context;
  uint64_t size;
  // Fills buffer from offset; false on error or a short read.
  bool (*read_at)(void* context, uint64_t offset, uint8_t* buffer, size_t len);
} bb_zip_source;

typedef struct bb_zip_directory {
  uint64_t offset;
  uint64_t size;
  uint64_t entries;
} bb_zip_directory;

// Where a search resumes; zero it to start from the first entry.
typedef struct bb_zip_cursor {
  uint64_t offset;
  uint64_t index;
} bb_zip_cursor;

typedef struct bb_zip_entry {
  uint16_t method;
  uint16_t flags;
  uint32_t crc32;
  uint64_t compressed_size;
  uint64_t uncompressed_size;
  uint64_t local_header_offset;
} bb_zip_entry;

int bb_zip_locate(const bb_zip_source* source, bb_zip_directory* out);
int bb_zip_find(const bb_zip_source* source, const bb_zip_directory* dir, const char* name, size_t name_len,
                bb_zip_cursor* cursor, bb_zip_entry* out);
// visit returns false to stop; name is valid only during the call. Names
// longer than 512 bytes are skipped.
int bb_zip_for_each(const bb_zip_source* source, const bb_zip_directory* dir,
                    bool (*visit)(const char* name, size_t name_len, const bb_zip_entry* entry, void* context),
                    void* context);
int bb_zip_data_offset(const bb_zip_source* source, const bb_zip_directory* dir, const bb_zip_entry* entry,
                       uint64_t* out);
const char* bb_zip_status_str(int status);

#ifdef __cplusplus
}
#endif
