#pragma once
#include <HalStorage.h>
#include <Memory.h>

#include <cstring>

namespace serialization {

// Sequential buffered wrappers over HalFile.
//
// SdFat keeps ONE shared 512-byte sector cache per volume, so interleaving small
// reads/writes across two or more files evicts and reloads that sector on nearly
// every call -- each 4-byte pod becomes a full SD transaction (measured: 31s to
// stream ~200KB through BookMetadataCache::buildBookBin on a 1,732-spine EPUB).
// Batching into chunk-sized transfers keeps each file at sequential SD speed.
//
// When buffered, the buffer is written only when full, so every write but the
// last is capacity bytes long. With a capacity that is a multiple of 512 and a
// file that starts at offset 0, the writes stay sector-aligned (until an explicit
// flush()), and SdFat writes them straight to the card, bypassing its sector
// cache. Measured on a 4 MiB clipping store rewrite in the emulator: flushing
// whenever a write did not fit gained almost nothing; whole buffers cut ~30%.
//
// Heap: one fixed buffer per wrapper, allocated once at construction and freed at
// scope exit. If the allocation fails the wrapper degrades to unbuffered
// passthrough -- correct, just slow -- so callers never need an OOM path.
//
// Constraint: the wrapper must be the file's ONLY accessor while alive (it tracks
// the underlying position itself); mixing direct HalFile calls in desynchronizes it.

class BufferedFileWriter {
 public:
  BufferedFileWriter(HalFile& file, const size_t capacity)
      : file(file), buf(makeUniqueNoThrow<uint8_t[]>(capacity)), cap(buf ? capacity : 0), pos(file.position()) {}
  ~BufferedFileWriter() { flush(); }
  BufferedFileWriter(const BufferedFileWriter&) = delete;
  BufferedFileWriter& operator=(const BufferedFileWriter&) = delete;

  void write(const void* src, const size_t len) {
    pos += len;
    const auto* p = static_cast<const uint8_t*>(src);
    if (cap == 0) {
      okFlag &= file.write(p, len) == len;
      return;
    }
    // Typed local: cppcheck misreads unique_ptr<uint8_t[]>::get() arithmetic as void*.
    uint8_t* const data = buf.get();
    size_t left = len;
    while (left > 0) {
      const size_t n = left < cap - fill ? left : cap - fill;
      memcpy(data + fill, p, n);
      fill += n;
      p += n;
      left -= n;
      if (fill == cap) flushBuffer();
    }
  }

  // Logical write position (bytes written since the file was opened).
  size_t position() const { return pos; }

  // Flush buffered bytes; returns false if any write so far has failed short.
  bool flush() {
    flushBuffer();
    return okFlag;
  }

 private:
  void flushBuffer() {
    if (fill == 0) return;
    okFlag &= file.write(buf.get(), fill) == fill;
    fill = 0;
  }

  HalFile& file;
  std::unique_ptr<uint8_t[]> buf;
  const size_t cap;
  size_t fill = 0;
  size_t pos;
  bool okFlag = true;
};

}  // namespace serialization
