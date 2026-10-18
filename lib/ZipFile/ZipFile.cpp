#include "ZipFile.h"

#include <HalStorage.h>
#include <InflateStream.h>
#include <Logging.h>

#include <algorithm>
#include <cstring>

struct ZipInflateCtx {
  HalFile* file = nullptr;
  size_t fileRemaining = 0;
  uint8_t* readBuf = nullptr;
  size_t readBufSize = 0;
};

namespace {
constexpr uint16_t ZIP_METHOD_STORED = 0;
constexpr uint16_t ZIP_METHOD_DEFLATED = 8;
// reserve() hint cap: the entry count comes from the file.
constexpr size_t MAX_RESERVED_ENTRIES = 4096;

bool readAt(void* context, const uint64_t offset, uint8_t* buffer, size_t len) {
  auto& file = *static_cast<HalFile*>(context);
  if (offset > SIZE_MAX || !file.seek(static_cast<size_t>(offset))) return false;
  while (len > 0) {
    const int got = file.read(buffer, len);
    if (got <= 0) return false;
    buffer += got;
    len -= static_cast<size_t>(got);
  }
  return true;
}

// Entries the device cannot address (ZIP64 sizes past 4 GiB) are rejected.
bool toFileStat(const bb_zip_entry& entry, ZipFile::FileStatSlim* stat) {
  if (entry.compressed_size > UINT32_MAX || entry.uncompressed_size > UINT32_MAX ||
      entry.local_header_offset > UINT32_MAX) {
    return false;
  }
  *stat = {entry.method, entry.flags, static_cast<uint32_t>(entry.compressed_size),
           static_cast<uint32_t>(entry.uncompressed_size), static_cast<uint32_t>(entry.local_header_offset)};
  return true;
}

// RAII zip: opens the zip if not already open, closes on destruction only if
// it performed the open.  Removes the wasOpen/close boilerplate from every method.
class ScopedOpenClose final {
 public:
  [[nodiscard]] explicit ScopedOpenClose(ZipFile& zf) : zf(zf), needsClose(!zf.isOpen()) {
    if (needsClose) ok = zf.open();
  }
  ~ScopedOpenClose() {
    if (needsClose && ok) zf.close();
  }
  ScopedOpenClose(const ScopedOpenClose&) = delete;
  ScopedOpenClose& operator=(const ScopedOpenClose&) = delete;
  ScopedOpenClose(ScopedOpenClose&&) = delete;
  ScopedOpenClose& operator=(ScopedOpenClose&&) = delete;
  explicit operator bool() const { return ok || !needsClose; }

 private:
  ZipFile& zf;
  bool needsClose = false;
  bool ok = true;  // true when zip was already open (no open() call needed)
};

size_t zipFillCallback(void* vctx, const uint8_t** data) {
  auto* ctx = static_cast<ZipInflateCtx*>(vctx);
  if (ctx->fileRemaining == 0) return 0;

  const size_t toRead = ctx->fileRemaining < ctx->readBufSize ? ctx->fileRemaining : ctx->readBufSize;
  const int result = ctx->file->read(ctx->readBuf, toRead);
  // HalFile::read() returns a negative int on error. Treat it as end-of-stream
  // rather than letting the negative-to-size_t conversion underflow fileRemaining
  // and report a huge bytesRead, which would have the inflate library read past
  // the end of readBuf.
  if (result < 0) {
    LOG_ERR("ZIP", "Failed to read compressed data: %d", result);
    return 0;
  }
  const size_t bytesRead = static_cast<size_t>(result);
  ctx->fileRemaining -= bytesRead;

  *data = ctx->readBuf;
  return bytesRead;
}
}  // namespace

bb_zip_source ZipFile::source() { return {&file, file.size(), readAt}; }

bool ZipFile::loadZipDetails() {
  if (directoryLoaded) return true;

  const ScopedOpenClose zip{*this};
  if (!zip) return false;

  const bb_zip_source src = source();
  const int status = bb_zip_locate(&src, &directory);
  if (status != BB_ZIP_OK) {
    LOG_ERR("ZIP", "%s: %s", filePath.c_str(), bb_zip_status_str(status));
    return false;
  }
  directoryLoaded = true;
  return true;
}

bool ZipFile::forEachEntry(const EntryVisitor visit, void* context) {
  const ScopedOpenClose zip{*this};
  if (!zip || !loadZipDetails()) return false;

  const bb_zip_source src = source();
  const int status = bb_zip_for_each(&src, &directory, visit, context);
  if (status != BB_ZIP_OK) {
    LOG_ERR("ZIP", "%s: %s", filePath.c_str(), bb_zip_status_str(status));
    return false;
  }
  return true;
}

bool ZipFile::loadAllFileStatSlims() {
  const ScopedOpenClose zip{*this};
  if (!zip || !loadZipDetails()) return false;

  fileStatSlimCache.clear();
  fileStatSlimCache.reserve(static_cast<size_t>(std::min<uint64_t>(directory.entries, MAX_RESERVED_ENTRIES)));
  const auto insert = [](const char* name, const size_t nameLen, const bb_zip_entry* entry, void* context) {
    FileStatSlim stat;
    if (toFileStat(*entry, &stat)) {
      static_cast<ZipFile*>(context)->fileStatSlimCache.emplace(std::string(name, nameLen), stat);
    }
    return true;
  };
  return forEachEntry(insert, this);
}

bool ZipFile::loadFileStatSlim(const char* filename, FileStatSlim* fileStat) {
  if (!fileStatSlimCache.empty()) {
    const auto it = fileStatSlimCache.find(filename);
    if (it != fileStatSlimCache.end()) {
      *fileStat = it->second;
      return true;
    }
    return false;
  }

  if (lastLookupValid && lastLookupName == filename) {
    *fileStat = lastLookup;
    return true;
  }

  const ScopedOpenClose zip{*this};
  if (!zip || !loadZipDetails()) return false;

  const bb_zip_source src = source();
  bb_zip_entry entry;
  const int status = bb_zip_find(&src, &directory, filename, strlen(filename), &cursor, &entry);
  if (status != BB_ZIP_OK) {
    if (status != BB_ZIP_NOT_FOUND) LOG_ERR("ZIP", "%s: %s", filePath.c_str(), bb_zip_status_str(status));
    return false;
  }
  if (!toFileStat(entry, fileStat)) {
    LOG_ERR("ZIP", "Entry %s is too large", filename);
    return false;
  }
  lastLookupName = filename;
  lastLookup = *fileStat;
  lastLookupValid = true;
  return true;
}

long ZipFile::getDataOffset(const FileStatSlim& fileStat) {
  const ScopedOpenClose zip{*this};
  if (!zip || !loadZipDetails()) return -1;

  const bb_zip_source src = source();
  const bb_zip_entry entry = {fileStat.method,         fileStat.flags, 0, fileStat.compressedSize,
                              fileStat.uncompressedSize, fileStat.localHeaderOffset};
  uint64_t offset = 0;
  const int status = bb_zip_data_offset(&src, &directory, &entry, &offset);
  if (status != BB_ZIP_OK) {
    LOG_ERR("ZIP", "%s: %s", filePath.c_str(), bb_zip_status_str(status));
    return -1;
  }
  return static_cast<long>(offset);
}

bool ZipFile::open() {
  if (!Storage.openFileForRead("ZIP", filePath, file)) {
    return false;
  }
  return true;
}

bool ZipFile::close() {
  if (file) {
    // Explicit close() required: member variable persists beyond function scope
    file.close();
  }
  cursor = {};
  // the archive may change before the next open
  lastLookupValid = false;
  return true;
}

bool ZipFile::getInflatedFileSize(const char* filename, size_t* size) {
  FileStatSlim fileStat = {};
  if (!loadFileStatSlim(filename, &fileStat)) {
    return false;
  }

  *size = static_cast<size_t>(fileStat.uncompressedSize);
  return true;
}

int ZipFile::fillUncompressedSizes(std::deque<SizeTarget>& targets, std::deque<uint32_t>& sizes) {
  if (targets.empty()) {
    return 0;
  }

  struct Batch {
    std::deque<SizeTarget>& targets;
    std::deque<uint32_t>& sizes;
    int matched;
  } batch{targets, sizes, 0};

  const auto match = [](const char* name, const size_t nameLen, const bb_zip_entry* entry, void* context) {
    auto& batch = *static_cast<Batch*>(context);
    if (entry->uncompressed_size > UINT32_MAX) return true;
    const uint64_t hash = fnvHash64(name, nameLen);
    const SizeTarget key = {hash, static_cast<uint16_t>(nameLen), 0};
    auto it = std::lower_bound(batch.targets.begin(), batch.targets.end(), key,
                               [](const SizeTarget& a, const SizeTarget& b) {
                                 return a.hash < b.hash || (a.hash == b.hash && a.len < b.len);
                               });
    while (it != batch.targets.end() && it->hash == hash && it->len == nameLen) {
      if (it->index < batch.sizes.size()) {
        batch.sizes[it->index] = static_cast<uint32_t>(entry->uncompressed_size);
        batch.matched++;
      }
      ++it;
    }
    return batch.matched < static_cast<int>(batch.targets.size());
  };
  forEachEntry(match, &batch);
  return batch.matched;
}

uint8_t* ZipFile::readFileToMemory(const char* filename, size_t* size, const bool trailingNullByte) {
  const ScopedOpenClose zip{*this};
  if (!zip) return nullptr;

  FileStatSlim fileStat = {};
  if (!loadFileStatSlim(filename, &fileStat)) return nullptr;

  const long fileOffset = getDataOffset(fileStat);
  if (fileOffset < 0) return nullptr;

  file.seek(fileOffset);

  const auto deflatedDataSize = fileStat.compressedSize;
  const auto inflatedDataSize = fileStat.uncompressedSize;
  // A size field of UINT32_MAX would wrap the terminator byte to a 0-byte buffer.
  if (trailingNullByte && inflatedDataSize == UINT32_MAX) {
    LOG_ERR("ZIP", "Entry %s is too large", filename);
    return nullptr;
  }
  const size_t dataSize = trailingNullByte ? size_t{inflatedDataSize} + 1 : inflatedDataSize;
  const auto data = static_cast<uint8_t*>(malloc(dataSize));
  if (data == nullptr) {
    LOG_ERR("ZIP", "Failed to allocate memory for output buffer (%zu bytes)", dataSize);
    return nullptr;
  }

  if (fileStat.method == ZIP_METHOD_STORED) {
    // no deflation, just read content
    const size_t dataRead = file.read(data, inflatedDataSize);

    if (dataRead != inflatedDataSize) {
      LOG_ERR("ZIP", "Failed to read data");
      free(data);
      return nullptr;
    }

    // Continue out of block with data set
  } else if (fileStat.method == ZIP_METHOD_DEFLATED) {
    auto* fileReadBuffer = static_cast<uint8_t*>(malloc(1024));
    if (!fileReadBuffer) {
      LOG_ERR("ZIP", "Failed to allocate memory for zip file read buffer");
      free(data);
      return nullptr;
    }

    ZipInflateCtx ctx;
    ctx.file = &file;
    ctx.fileRemaining = deflatedDataSize;
    ctx.readBuf = fileReadBuffer;
    ctx.readBufSize = 1024;

    // One-shot mode: `data` holds the entire output, so back-references
    // resolve inside it and no 32KB window is allocated.
    InflateStream inflate;
    if (!inflate.init(false)) {
      LOG_ERR("ZIP", "Failed to init inflate stream for %s", filename);
      free(fileReadBuffer);
      free(data);
      return nullptr;
    }
    inflate.setFill(zipFillCallback, &ctx);

    if (!inflate.read(data, inflatedDataSize)) {
      LOG_ERR("ZIP", "Failed to inflate file");
      free(fileReadBuffer);
      free(data);
      return nullptr;
    }
    free(fileReadBuffer);

    // Continue out of block with data set
  } else {
    LOG_ERR("ZIP", "Unsupported compression method");
    free(data);
    return nullptr;
  }

  if (trailingNullByte) data[inflatedDataSize] = '\0';
  if (size) *size = inflatedDataSize;
  return data;
}

bool ZipFile::readFileToStream(const char* filename, Print& out, const size_t chunkSize, const bool allowEarlyStop) {
  const ScopedOpenClose zip{*this};
  if (!zip) return false;

  FileStatSlim fileStat = {};
  if (!loadFileStatSlim(filename, &fileStat)) return false;

  const long fileOffset = getDataOffset(fileStat);
  if (fileOffset < 0) return false;

  file.seek(fileOffset);
  const auto deflatedDataSize = fileStat.compressedSize;
  const auto inflatedDataSize = fileStat.uncompressedSize;

  if (fileStat.method == ZIP_METHOD_STORED) {
    // no deflation, just read content
    const auto buffer = static_cast<uint8_t*>(malloc(chunkSize));
    if (!buffer) {
      LOG_ERR("ZIP", "Failed to allocate memory for buffer");
      return false;
    }

    size_t remaining = inflatedDataSize;
    while (remaining > 0) {
      const size_t dataRead = file.read(buffer, remaining < chunkSize ? remaining : chunkSize);
      if (dataRead == 0) {
        LOG_ERR("ZIP", "Could not read more bytes");
        free(buffer);
        return false;
      }

      if (out.write(buffer, dataRead) != dataRead) {
        free(buffer);
        if (allowEarlyStop) return true;  // sink has what it needs
        LOG_ERR("ZIP", "Failed to write all output bytes to stream");
        return false;
      }
      remaining -= dataRead;
    }

    free(buffer);
    return true;
  }

  if (fileStat.method == ZIP_METHOD_DEFLATED) {
    auto* fileReadBuffer = static_cast<uint8_t*>(malloc(chunkSize));
    if (!fileReadBuffer) {
      LOG_ERR("ZIP", "Failed to allocate memory for zip file read buffer");
      return false;
    }

    auto* outputBuffer = static_cast<uint8_t*>(malloc(chunkSize));
    if (!outputBuffer) {
      LOG_ERR("ZIP", "Failed to allocate memory for output buffer");
      free(fileReadBuffer);
      return false;
    }

    ZipInflateCtx ctx;
    ctx.file = &file;
    ctx.fileRemaining = deflatedDataSize;
    ctx.readBuf = fileReadBuffer;
    ctx.readBufSize = chunkSize;

    InflateStream inflate;
    if (!inflate.init(true)) {
      LOG_ERR("ZIP", "Failed to init inflate stream for %s", filename);
      free(outputBuffer);
      free(fileReadBuffer);
      return false;
    }
    inflate.setFill(zipFillCallback, &ctx);

    bool success = false;
    size_t totalProduced = 0;

    while (true) {
      size_t produced;
      const InflateStream::Status status = inflate.readAtMost(outputBuffer, chunkSize, &produced);

      totalProduced += produced;
      if (totalProduced > static_cast<size_t>(inflatedDataSize)) {
        LOG_ERR("ZIP", "Decompressed size exceeds expected (%zu > %zu)", totalProduced,
                static_cast<size_t>(inflatedDataSize));
        break;
      }

      if (produced > 0) {
        if (out.write(outputBuffer, produced) != produced) {
          if (allowEarlyStop) {
            success = true;  // sink has what it needs
          } else {
            LOG_ERR("ZIP", "Failed to write all output bytes to stream");
          }
          break;
        }
      }

      if (status == InflateStream::Status::Done) {
        if (totalProduced != static_cast<size_t>(inflatedDataSize)) {
          LOG_ERR("ZIP", "Decompressed size mismatch (expected %zu, got %zu)", static_cast<size_t>(inflatedDataSize),
                  totalProduced);
          break;
        }
        LOG_DBG("ZIP", "Decompressed %d bytes into %d bytes", deflatedDataSize, inflatedDataSize);
        success = true;
        break;
      }

      if (status == InflateStream::Status::Error) {
        LOG_ERR("ZIP", "Decompression failed");
        break;
      }
      // InflateStream::Status::Ok: output buffer full, continue
    }

    free(outputBuffer);
    free(fileReadBuffer);
    return success;  // inflate destructor frees the decompressor state + window
  }

  LOG_ERR("ZIP", "Unsupported compression method");
  return false;
}
