#include "InflateStream.h"

#include <BuildScratch.h>
#include <HalMemory.h>
#include <Logging.h>

#include <algorithm>
#include <cstdlib>
#include <cstring>

#include "RomTinfl.h"

namespace {
constexpr size_t WINDOW_SIZE = TINFL_LZ_DICT_SIZE;
static_assert(WINDOW_SIZE == 32768);
// Keeps the window after the state 8-aligned inside the claimed scratch.
constexpr size_t STATE_ALIGNED = (sizeof(tinfl_decompressor) + 7) & ~size_t{7};
// The most output a byte of deflate input can give: a 258-byte match from a 1-bit length code and a
// 1-bit distance code, 129 bytes a bit.
constexpr uint64_t MAX_EXPANSION = 1032;
void logAllocationFailure(const char* allocation, size_t bytes) {
  const auto heap = HalMemory::getDefaultHeap();
  const auto psram = HalMemory::getPsramHeap();
  LOG_ERR("ZIP", "Inflate %s OOM (%zu bytes): heap %zu free/%zu max, PSRAM %zu free/%zu total", allocation, bytes,
          heap.freeBytes, heap.largestBlockBytes, psram.freeBytes, psram.totalBytes);
}
}  // namespace

InflateStream::~InflateStream() { deinit(); }

bool InflateStream::init(const bool streaming) {
  // Every consumer constructs a fresh stream per operation, so acquire storage
  // from scratch each init (releasing any prior backing first).
  deinit();

  // During a framebuffer loan the lent framebuffer (48KB on X4) is up for
  // grabs: when state (~11KB) + window (32KB) fit inside it, a chapter-build
  // inflate costs the heap nothing. Otherwise: plain heap, freed in deinit().
  arenaBase = buildscratch::claim(STATE_ALIGNED + (streaming ? WINDOW_SIZE : 0));
  if (arenaBase) {
    decoder = reinterpret_cast<tinfl_decompressor*>(arenaBase);
    window = streaming ? arenaBase + STATE_ALIGNED : nullptr;
  } else {
    // Window first, so the state cannot split the only free block large enough for it.
    if (streaming) {
      window = static_cast<uint8_t*>(malloc(WINDOW_SIZE));
      if (!window) {
        logAllocationFailure("window", WINDOW_SIZE);
        return false;
      }
    }
    decoder = static_cast<tinfl_decompressor*>(malloc(sizeof(tinfl_decompressor)));
    if (!decoder) {
      logAllocationFailure("state", sizeof(tinfl_decompressor));
      free(window);
      window = nullptr;
      return false;
    }
  }
  tinfl_init(decoder);
  // In streaming mode tinfl does not check a distance against the bytes written: corrupt data
  // that refers back past the start reads 0, not old memory.
  if (window) memset(window, 0, WINDOW_SIZE);
  return true;
}

void InflateStream::deinit() {
  if (arenaBase) {
    buildscratch::release(arenaBase);
    arenaBase = nullptr;
  } else {
    free(decoder);
    free(window);
  }
  decoder = nullptr;
  window = nullptr;
  input = nullptr;
  inputLen = 0;
  fill = nullptr;
  fillCtx = nullptr;
  inputEnded = false;
  flags = 0;
  end = End::None;
  inputTotal = 0;
  outputTotal = 0;
  outStart = nullptr;
  outWritten = 0;
  windowPos = 0;
  pendingStart = 0;
  pendingLen = 0;
}

void InflateStream::setSource(const uint8_t* src, const size_t len) {
  input = src;
  inputLen = src ? len : 0;
  fill = nullptr;
  inputEnded = true;
}

void InflateStream::setFill(const FillFn fn, void* ctx) {
  input = nullptr;
  inputLen = 0;
  fill = fn;
  fillCtx = ctx;
  inputEnded = fn == nullptr;
}

void InflateStream::setZlibWrapped() { flags |= TINFL_FLAG_PARSE_ZLIB_HEADER; }

bool InflateStream::refillInput() {
  if (inputLen > 0) return true;
  if (inputEnded || !fill) {
    inputEnded = true;
    return false;
  }
  const uint8_t* data = nullptr;
  const size_t len = fill(fillCtx, &data);
  if (len == 0 || !data) {
    inputEnded = true;
    return false;
  }
  input = data;
  inputLen = len;
  return true;
}

void InflateStream::decode(uint8_t* base, uint8_t* out, size_t* outLen, const uint32_t outFlags) {
  size_t inLen = inputLen;
  // Always "more input may follow": with the flag clear, the ROM's tinfl reads past the end of
  // the input as zeros, so a cut stream could decode to garbage. The end of the input shows up
  // as TINFL_STATUS_NEEDS_MORE_INPUT instead, and is handled below.
  const tinfl_status status =
      tinfl_decompress(decoder, input, &inLen, base, out, outLen, flags | outFlags | TINFL_FLAG_HAS_MORE_INPUT);
  input += inLen;
  inputLen -= inLen;
  inputTotal += inLen;
  outputTotal += *outLen;
  if (outputTotal > MAX_EXPANSION * inputTotal) {
    // Output that no input accounts for: miniz 1.15 accepts a literal/length code with one symbol
    // or none, then writes output endlessly without reading input. Drop it and fail the stream.
    LOG_ERR("ZIP", "Inflate failed: output past what the input can hold");
    *outLen = 0;
    end = End::Error;
  } else if (status == TINFL_STATUS_DONE) {
    end = End::Done;
  } else if (status == TINFL_STATUS_NEEDS_MORE_INPUT) {
    if (!refillInput()) {
      LOG_ERR("ZIP", "Inflate failed: the input ended inside the stream");
      end = End::Error;
    }
  } else if (status < 0) {
    LOG_ERR("ZIP", "Inflate failed: %s",
            status == TINFL_STATUS_ADLER32_MISMATCH ? "Adler-32 mismatch" : "corrupt stream");
    end = End::Error;
  }
}

InflateStream::Status InflateStream::readOneShot(uint8_t* dest, const size_t maxLen, size_t* produced) {
  // Back-references resolve in the bytes in front of dest, so they must be this stream's
  // earlier output.
  if (!outStart) outStart = dest;
  if (dest != outStart + outWritten) {
    LOG_ERR("ZIP", "Inflate failed: one-shot output is not the next slice of its buffer");
    return Status::Error;
  }
  while (*produced < maxLen && end == End::None) {
    size_t outLen = maxLen - *produced;
    decode(outStart, dest + *produced, &outLen, TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF);
    *produced += outLen;
    outWritten += outLen;
  }
  return result(*produced == maxLen);
}

InflateStream::Status InflateStream::readStreaming(uint8_t* dest, const size_t maxLen, size_t* produced) {
  while (*produced < maxLen) {
    if (pendingLen > 0) {
      const size_t n = std::min(pendingLen, maxLen - *produced);
      memcpy(dest + *produced, window + pendingStart, n);
      pendingStart += n;
      pendingLen -= n;
      *produced += n;
      continue;
    }
    if (end != End::None) break;
    // Decode into the window up to its end; the bytes are handed out from there.
    size_t outLen = WINDOW_SIZE - windowPos;
    decode(window, window + windowPos, &outLen, 0);
    pendingStart = windowPos;
    pendingLen = outLen;
    windowPos = (windowPos + outLen) & (WINDOW_SIZE - 1);
  }
  return result(*produced == maxLen);
}

InflateStream::Status InflateStream::result(const bool filled) const {
  // Done or Error is reported only once every decoded byte is handed out, and Error only to a
  // read the failure left short: a read filled before the failure is Ok (a dictzip chunk has no
  // final block, so its input runs out right after its last byte).
  if (pendingLen > 0 || end == End::None) return Status::Ok;
  if (end == End::Done) return Status::Done;
  return filled ? Status::Ok : Status::Error;
}

InflateStream::Status InflateStream::readAtMost(uint8_t* dest, const size_t maxLen, size_t* produced) {
  *produced = 0;
  if (!decoder) return Status::Error;
  if (maxLen == 0) return result(false);
  return window ? readStreaming(dest, maxLen, produced) : readOneShot(dest, maxLen, produced);
}

bool InflateStream::read(uint8_t* dest, const size_t len) {
  // readAtMost already fills dest unless the stream ends or fails first.
  size_t produced = 0;
  return readAtMost(dest, len, &produced) != Status::Error && produced == len;
}
