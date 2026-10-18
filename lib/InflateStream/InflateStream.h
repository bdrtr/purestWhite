#pragma once

#include <cstddef>
#include <cstdint>

struct bb_inflate;

// Streaming deflate decompressor; the decoder is our own Rust (rust/crates/bembeyaz-inflate)
// behind rust/include/bembeyaz/inflate.h.
//
// The firmware's DEFLATE decoder for EPUB zip entries, PNG IDAT, font groups
// (FontDecompressor) and dictzip chunks (DictZip); the SDK's ContentProtection
// keeps its own miniz. The decoder state (~4.5KB)
// lives for the scope of the stream: taken from the lent framebuffer bytes via
// buildscratch::claim() when a FrameBufferLoan is active, heap otherwise (on
// the S3, malloc puts blocks over 4KB in PSRAM).
//
// Two modes:
//   init(false) -- one-shot: the destination buffer holds the ENTIRE output,
//                  so back-references resolve inside it and no 32KB window is
//                  allocated. read()/readAtMost() must be driven with
//                  contiguous, forward-only slices of that one buffer
//                  (a single read(dest, totalSize) is the common case); any
//                  other slice is an Error.
//   init(true)  -- streaming: allocates a 32KB window; output can go to any
//                  buffer in any-sized chunks across calls.
//
// Input is either a single contiguous buffer (setSource) or pulled on demand
// through a fill callback (setFill): return the number of bytes available and
// point *data at them (valid until the next fill call); return 0 at end of
// input. Call setZlibWrapped() before the first read when the stream has a
// zlib header (e.g. PNG IDAT).
class InflateStream {
 public:
  enum class Status {
    Ok,     // Output buffer full; more decompressed data remains.
    Done,   // Stream ended cleanly. produced may be < maxLen.
    Error,  // Corrupt/truncated stream, or decompression failed.
  };

  using FillFn = size_t (*)(void* ctx, const uint8_t** data);

  InflateStream() = default;
  ~InflateStream();
  InflateStream(const InflateStream&) = delete;
  InflateStream& operator=(const InflateStream&) = delete;

  // Allocate decompressor state (and the 32KB window when streaming) and reset
  // stream state. Releases prior storage first; leaves no allocation on OOM.
  // The window is allocated before the state, so the state cannot split the
  // only free block large enough for the window.
  bool init(bool streaming);

  // Free the decompressor state and window.
  void deinit();

  // Provide the entire compressed input as one contiguous buffer.
  void setSource(const uint8_t* src, size_t len);

  // Provide compressed input on demand. ctx is passed back to fn verbatim.
  void setFill(FillFn fn, void* ctx);

  // Declare the input zlib-wrapped (2-byte header + trailing adler32).
  void setZlibWrapped();

  // Decompress exactly len bytes into dest. Returns false if the stream ends
  // or errors before producing len bytes.
  bool read(uint8_t* dest, size_t len);

  // Decompress up to maxLen bytes into dest; *produced gets the byte count,
  // which on Error is the output decoded before the failure.
  Status readAtMost(uint8_t* dest, size_t maxLen, size_t* produced);

 private:
  bb_inflate* stream = nullptr;  // decoder state: heap, or inside the claimed build scratch
  uint8_t* window = nullptr;     // 32KB ring, streaming mode only
  uint8_t* arenaBase = nullptr;  // non-null when stream/window live in lent framebuffer bytes
};
