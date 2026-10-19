#pragma once

#include <cstddef>
#include <cstdint>

struct tinfl_decompressor_tag;

// Streaming deflate decompressor; the decoder is miniz 1.15's tinfl_decompress in
// the ESP32-S3 mask ROM (RomTinfl.h), which costs no flash. Host tests link the
// same miniz version from source (test/rom_tinfl); emu/compare.sh runs the ROM's.
//
// The firmware's DEFLATE decoder for EPUB zip entries, font groups
// (FontDecompressor) and dictzip chunks (DictZip); PNG images decode with
// bembeyaz-png's own inflate, and the SDK's ContentProtection keeps its own
// miniz. The decoder state (~11KB) lives for the scope of the stream: taken
// from the lent framebuffer bytes via buildscratch::claim() when a
// FrameBufferLoan is active, heap otherwise (on the S3, malloc puts blocks
// over 4KB in PSRAM).
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
// zlib header.
class InflateStream {
 public:
  enum class Status {
    Ok,     // Output buffer full; the stream has not ended (a later read may still fail).
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
  // How the decoder stopped; its result once the decoded bytes are handed out.
  enum class End : uint8_t { None, Done, Error };

  bool refillInput();
  // One tinfl_decompress call writing at out within the buffer that starts at base; sets end
  // when the stream ends or fails, and refills the input when it runs out.
  void decode(uint8_t* base, uint8_t* out, size_t* outLen, uint32_t outFlags);
  Status readOneShot(uint8_t* dest, size_t maxLen, size_t* produced);
  Status readStreaming(uint8_t* dest, size_t maxLen, size_t* produced);
  // The status of a read that filled its buffer or not.
  Status result(bool filled) const;

  tinfl_decompressor_tag* decoder = nullptr;  // heap, or inside the claimed build scratch
  uint8_t* window = nullptr;                  // 32KB ring, streaming mode only
  uint8_t* arenaBase = nullptr;               // non-null when decoder/window live in lent framebuffer bytes

  const uint8_t* input = nullptr;  // unread input bytes
  size_t inputLen = 0;
  FillFn fill = nullptr;
  void* fillCtx = nullptr;
  bool inputEnded = false;
  uint32_t flags = 0;  // zlib header and trailer
  End end = End::None;
  uint64_t inputTotal = 0;  // bytes the decoder took and gave, against MAX_EXPANSION
  uint64_t outputTotal = 0;

  uint8_t* outStart = nullptr;  // one-shot: the first byte of the output buffer
  size_t outWritten = 0;
  size_t windowPos = 0;  // streaming: where the decoder writes next
  size_t pendingStart = 0;
  size_t pendingLen = 0;  // decoded bytes in the window not handed out yet
};
