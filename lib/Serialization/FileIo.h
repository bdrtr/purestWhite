#pragma once

#include <BufferedFile.h>
#include <HalStorage.h>
#include <bembeyaz/io.h>

#include <cstddef>
#include <cstdint>

// A HalFile as the format crates read and write it: at any offset.
namespace file_io {

inline bool readAt(void* context, const uint32_t offset, uint8_t* buf, const size_t len) {
  auto& f = *static_cast<HalFile*>(context);
  return len == 0 || (f.seek(offset) && static_cast<size_t>(f.read(buf, len)) == len);
}

inline bool writeAt(void* context, const uint32_t offset, const uint8_t* bytes, const size_t len) {
  auto& f = *static_cast<HalFile*>(context);
  return f.seek(offset) && f.write(bytes, len) == len;
}

inline bb_io_file fileOf(HalFile& f) { return {&f, static_cast<uint32_t>(f.size()), readAt}; }

inline bb_io_sink sinkOf(HalFile& f) { return {&f, writeAt}; }

// A BufferedFileWriter accepts only its next offset.
inline bool writeBuffered(void* context, const uint32_t offset, const uint8_t* bytes, const size_t len) {
  auto& out = *static_cast<serialization::BufferedFileWriter*>(context);
  if (offset != out.position()) return false;
  out.write(bytes, len);
  return true;
}

inline bb_io_sink sinkOf(serialization::BufferedFileWriter& out) { return {&out, writeBuffered}; }

}  // namespace file_io
