#include "DictZip.h"

#include <Arduino.h>  // ESP.getMaxAllocHeap for the pre-reserve heap guard
#include <InflateStream.h>
#include <Memory.h>
#include <bembeyaz/dict.h>

#include <cstddef>

namespace DictZip {
namespace {

// Slack required above the chunk table before reserving it, so a table that
// would only just fit is refused rather than left to abort inside resize().
// (bembeyaz-dict caps the table at 8192 chunks: 32KB.)
constexpr size_t CHUNK_TABLE_HEAP_HEADROOM_BYTES = 1024;

// dict.h's layouts, which bembeyaz-ffi's dict.rs checks too.
static_assert(sizeof(bb_dictzip_file) == 5 * sizeof(void*) && offsetof(bb_dictzip_file, size) == 3 * sizeof(void*),
              "bb_dictzip_file layout");
static_assert(sizeof(bb_dictzip_header) == 12 && offsetof(bb_dictzip_header, chunk_length) == 8,
              "bb_dictzip_header layout");

// The file as bembeyaz-dict reads it (dict.h).
bb_dictzip_file zipFileOf(HalFile& file) {
  return {[](void* f, uint8_t* buf, const size_t len) -> int32_t { return static_cast<HalFile*>(f)->read(buf, len); },
          [](void* f, const uint32_t pos) { return static_cast<HalFile*>(f)->seekSet(pos); },
          [](void* f) { return static_cast<uint32_t>(static_cast<HalFile*>(f)->position()); },
          static_cast<uint32_t>(file.fileSize()), &file};
}

ExtractError errorOf(const uint8_t result) {
  switch (result) {
    case BB_DICTZIP_OK:
      return ExtractError::None;
    case BB_DICTZIP_READ:
      return ExtractError::ReadError;
    case BB_DICTZIP_MEMORY:
      return ExtractError::LowMemory;
    default:
      return ExtractError::Decompress;
  }
}

uint8_t resultOf(const ExtractError error) {
  switch (error) {
    case ExtractError::None:
      return BB_DICTZIP_OK;
    case ExtractError::ReadError:
      return BB_DICTZIP_READ;
    case ExtractError::LowMemory:
      return BB_DICTZIP_MEMORY;
    default:
      return BB_DICTZIP_CORRUPT;
  }
}

// Compressed input for one chunk, pulled from the file on demand instead of
// buffered whole. A dictzip chunk is ~58KB uncompressed (measured at 18KB
// compressed for a real dictionary), so the old whole-chunk buffer was a large
// contiguous request made while the 32KB inflate window was about to be taken
// as well — two big blocks live at once, on a heap that during a reading
// session has well under 50KB free (#2744). This holds INPUT_BUF_BYTES instead.
constexpr size_t INPUT_BUF_BYTES = 2048;

struct ChunkSource {
  HalFile* file = nullptr;
  uint32_t remaining = 0;  // compressed bytes left in this chunk
  bool readFailed = false;
  uint8_t buf[INPUT_BUF_BYTES] = {};
};

// InflateStream::FillFn: refills from the file, 0 at the chunk's end.
// Never reads past compressedSize: the following bytes belong to the next
// chunk, and stopping there reproduces exactly the hard limit the whole-chunk
// buffer used to impose.
size_t chunkFill(void* ctx, const uint8_t** data) {
  auto* src = static_cast<ChunkSource*>(ctx);
  if (src->remaining == 0) return 0;

  const uint32_t want = src->remaining < INPUT_BUF_BYTES ? src->remaining : INPUT_BUF_BYTES;
  const int n = src->file->read(src->buf, want);
  if (n <= 0) {
    // Remember the cause: the decoder reports this as a truncated stream,
    // but an IO error is a ReadError, not a corrupt .dz.
    src->readFailed = true;
    return 0;
  }
  src->remaining -= static_cast<uint32_t>(n);
  *data = src->buf;
  return static_cast<size_t>(n);
}

bool extractChunkSlice(HalFile& file, uint32_t compressedOffset, uint32_t compressedSize, uint32_t discardSize,
                       uint32_t extractSize, HalFile& outFile, ExtractError* outError) {
  const auto fail = [outError](ExtractError e) {
    if (outError) *outError = e;
    return false;
  };
  if (extractSize == 0) return true;

  // Largest block FIRST. Every allocation here is carved from the same big free
  // run, so taking the ~3KB ChunkSource before the 32KB window left the window's
  // block 12 bytes short on device (largest=32756 against need=32768) even on a
  // barely fragmented heap. Ordering by size removes that failure mode: the
  // window (and the ~11KB decoder state after it, see InflateStream::init) gets
  // the big block while it is still whole, and the small buffers fit in what
  // remains — or in one of the smaller free blocks.
  InflateStream inflate;
  if (!inflate.init(true)) return fail(ExtractError::LowMemory);

  auto src = makeUniqueNoThrow<ChunkSource>();
  if (!src) return fail(ExtractError::LowMemory);
  src->file = &file;
  src->remaining = compressedSize;

  // compressedOffset comes from the untrusted .dz chunk table, so an out-of-range
  // seek is possible — guard it rather than reading from the prior position.
  if (!file.seekSet(compressedOffset)) return fail(ExtractError::ReadError);

  inflate.setFill(&chunkFill, src.get());

  auto buf = makeUniqueNoThrow<uint8_t[]>(512);
  if (!buf) return fail(ExtractError::LowMemory);

  // A decode failure after the callback hit an IO error is a read failure, not
  // a corrupt stream — keep the two distinguishable for the caller.
  const auto decodeFail = [&src, &fail] {
    return fail(src->readFailed ? ExtractError::ReadError : ExtractError::Decompress);
  };

  uint32_t batch;
  while (discardSize > 0) {
    batch = discardSize < 512 ? discardSize : 512;
    if (!inflate.read(buf.get(), batch)) return decodeFail();
    discardSize -= batch;
  }

  while (extractSize > 0) {
    batch = extractSize < 512 ? extractSize : 512;
    if (!inflate.read(buf.get(), batch)) return decodeFail();
    if (outFile.write(buf.get(), batch) != batch) return fail(ExtractError::ReadError);
    extractSize -= batch;
  }

  return true;
}

}  // namespace

bool parse(HalFile& file, Info* info, ExtractError* outError) {
  // bembeyaz-dict classifies each failure: a header/table read that comes up
  // short is a ReadError (IO / truncated file); a value that doesn't make sense
  // as dictzip is a Decompress (malformed/unsupported .dz).
  const auto fail = [outError](ExtractError e) {
    if (outError) *outError = e;
    return false;
  };
  if (outError) *outError = ExtractError::None;  // establish the out-param on every path
  if (!info) return fail(ExtractError::Decompress);
  *info = {};

  const bb_dictzip_file zip = zipFileOf(file);
  bb_dictzip_header header{};
  const uint8_t result = bb_dictzip_parse(
      &zip,
      [](void* context, const size_t len) -> uint32_t* {
        // vector growth aborts under -fno-exceptions if it can't allocate;
        // refuse up front (like Dictionary::readDefinition's guard) so a
        // fragmented heap surfaces LowMemory instead of crashing.
        if (ESP.getMaxAllocHeap() < len * sizeof(uint32_t) + CHUNK_TABLE_HEAP_HEADROOM_BYTES) return nullptr;
        auto& offsets = static_cast<Info*>(context)->chunkOffsets;
        offsets.resize(len);
        return offsets.data();
      },
      info, &header);
  if (result != BB_DICTZIP_OK) {
    *info = {};  // nothing partial, and the table's memory back
    return fail(errorOf(result));
  }
  info->dataOffset = header.data_offset;
  info->totalSize = header.total_size;
  info->chunkLength = header.chunk_length;
  info->valid = true;
  return true;
}

bool extractEntry(const char* path, uint32_t offset, uint32_t size, HalFile& outFile, ExtractError* outError) {
  const auto fail = [outError](ExtractError e) {
    if (outError) *outError = e;
    return false;
  };
  if (outError) *outError = ExtractError::None;
  if (size == 0) return true;

  HalFile file;
  if (!Storage.openFileForRead("DICTZIP", path, file)) return fail(ExtractError::ReadError);

  Info info;
  // parse() reports its own cause: a header/table read failure is ReadError,
  // a malformed/unsupported .dz is Decompress.
  if (!parse(file, &info, outError)) return false;

  // The chunks the range needs (offset/size come from the untrusted .idx, so a
  // range outside the text is a ReadError and a table that cannot cover it a
  // Decompress), each inflated in turn.
  struct Context {
    HalFile& file;
    HalFile& outFile;
    ExtractError error;
  } context{file, outFile, ExtractError::None};
  const bb_dictzip_header header{info.dataOffset, info.totalSize, info.chunkLength};
  const uint8_t result = bb_dictzip_slices(
      &header, info.chunkOffsets.data(), info.chunkOffsets.size(), offset, size,
      [](void* c, const uint32_t compOffset, const uint32_t compSize, const uint32_t skip, const uint32_t take) {
        auto& [file, outFile, error] = *static_cast<Context*>(c);
        return extractChunkSlice(file, compOffset, compSize, skip, take, outFile, &error) ? uint8_t{BB_DICTZIP_OK}
                                                                                          : resultOf(error);
      },
      &context);
  // A slice's own cause comes back through its result.
  if (result != BB_DICTZIP_OK) return fail(errorOf(result));
  return true;
}

}  // namespace DictZip
