#include "InflateStream.h"

#include <BuildScratch.h>
#include <HalMemory.h>
#include <Logging.h>
#include <bembeyaz/inflate.h>

#include <cstdlib>

namespace {
constexpr size_t WINDOW_SIZE = BB_INFLATE_WINDOW_SIZE;
// Keeps the window after the state 8-aligned inside the claimed scratch.
size_t stateAligned() { return (bb_inflate_state_size() + 7) & ~size_t{7}; }
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
  // grabs: when state (~4.5KB) + window (32KB) fit inside it, a chapter-build
  // inflate costs the heap nothing. Otherwise: plain heap, freed in deinit().
  const size_t stateSize = bb_inflate_state_size();
  void* state = nullptr;
  arenaBase = buildscratch::claim(stateAligned() + (streaming ? WINDOW_SIZE : 0));
  if (arenaBase) {
    state = arenaBase;
    window = streaming ? arenaBase + stateAligned() : nullptr;
  } else {
    // Raw malloc: the state is a Rust struct that needs no destructor; both
    // blocks are freed in deinit()/the destructor. Window first, so the state
    // cannot split the only free block large enough for it.
    if (streaming) {
      window = static_cast<uint8_t*>(malloc(WINDOW_SIZE));
      if (!window) {
        logAllocationFailure("window", WINDOW_SIZE);
        return false;
      }
    }
    state = malloc(stateSize);
    if (!state) {
      logAllocationFailure("state", stateSize);
      free(window);
      window = nullptr;
      return false;
    }
  }

  stream = bb_inflate_init(state, stateSize, window, streaming ? WINDOW_SIZE : 0);
  if (!stream) {
    LOG_ERR("ZIP", "Inflate state at %p is not %zu-aligned", state, bb_inflate_state_align());
    if (!arenaBase) free(state);
    deinit();
    return false;
  }
  return true;
}

void InflateStream::deinit() {
  if (arenaBase) {
    buildscratch::release(arenaBase);
    arenaBase = nullptr;
  } else {
    free(stream);
    free(window);
  }
  stream = nullptr;
  window = nullptr;
}

void InflateStream::setSource(const uint8_t* src, const size_t len) {
  if (stream) bb_inflate_set_source(stream, src, len);
}

void InflateStream::setFill(const FillFn fn, void* ctx) {
  if (stream) bb_inflate_set_fill(stream, fn, ctx);
}

void InflateStream::setZlibWrapped() {
  if (stream) bb_inflate_set_zlib(stream);
}

InflateStream::Status InflateStream::readAtMost(uint8_t* dest, const size_t maxLen, size_t* produced) {
  *produced = 0;
  if (!stream) return Status::Error;
  const int status = bb_inflate_read(stream, dest, maxLen, produced);
  if (status == BB_INFLATE_MORE) return Status::Ok;
  if (status == BB_INFLATE_DONE) return Status::Done;
  LOG_ERR("ZIP", "Inflate failed: %s", bb_inflate_status_str(status));
  return Status::Error;
}

bool InflateStream::read(uint8_t* dest, const size_t len) {
  // readAtMost already fills dest unless the stream ends or fails first.
  size_t produced = 0;
  return readAtMost(dest, len, &produced) != Status::Error && produced == len;
}
