#pragma once

// A file as the crates that read and write file formats see it (rust/crates/bembeyaz-io).

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// A file: read_at fills len bytes from offset, false when it cannot fill them all.
typedef struct {
  void* context;
  uint32_t size;
  bool (*read_at)(void* context, uint32_t offset, uint8_t* buf, size_t len);
} bb_io_file;

// A file being written: write_at writes len bytes at offset, false when it cannot write them all.
typedef struct {
  void* context;
  bool (*write_at)(void* context, uint32_t offset, const uint8_t* bytes, size_t len);
} bb_io_sink;

#ifdef __cplusplus
}
#endif
