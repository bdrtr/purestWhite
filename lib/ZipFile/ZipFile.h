#pragma once
#include <HalStorage.h>
#include <bembeyaz/zip.h>

#include <deque>
#include <string>
#include <string_view>
#include <unordered_map>

// ZIP container access for EPUB files. The container format is parsed in
// Rust (rust/crates/bembeyaz-zip); this class adds the HAL file, lookup caches
// and decompression.
class ZipFile {
 public:
  struct FileStatSlim {
    uint16_t method;             // Compression method
    uint16_t flags;              // General purpose bit flags
    uint32_t compressedSize;     // Compressed size
    uint32_t uncompressedSize;   // Uncompressed size
    uint32_t localHeaderOffset;  // Offset of local file header
  };

  // Target for batch uncompressed size lookup (sorted by hash, then len)
  struct SizeTarget {
    uint64_t hash;   // FNV-1a 64-bit hash of normalized path
    uint16_t len;    // Length of path for collision reduction
    uint16_t index;  // Caller's index (e.g. spine index)
  };

  // FNV-1a 64-bit hash computed from char buffer (no std::string allocation)
  static uint64_t fnvHash64(const char* s, size_t len) {
    uint64_t hash = 14695981039346656037ull;
    for (size_t i = 0; i < len; i++) {
      hash ^= static_cast<uint8_t>(s[i]);
      hash *= 1099511628211ull;
    }
    return hash;
  }

 private:
  using EntryVisitor = bool (*)(const char* name, size_t nameLen, const bb_zip_entry* entry, void* context);

  const std::string& filePath;
  HalFile file;
  bb_zip_directory directory = {};
  bool directoryLoaded = false;
  std::unordered_map<std::string, FileStatSlim> fileStatSlimCache;

  // Sequential lookups resume where the previous one matched.
  bb_zip_cursor cursor = {};
  // Reuse the lookup when extraction follows a size query for the same entry
  // while the archive stays open
  std::string lastLookupName;
  FileStatSlim lastLookup = {};
  bool lastLookupValid = false;

  bb_zip_source source();
  bool loadFileStatSlim(const char* filename, FileStatSlim* fileStat);
  long getDataOffset(const FileStatSlim& fileStat);
  bool loadZipDetails();
  // Visits every central directory entry; false when the archive cannot be
  // opened or its directory is malformed.
  bool forEachEntry(EntryVisitor visit, void* context);

 public:
  explicit ZipFile(const std::string& filePath) : filePath(filePath) {}
  ~ZipFile() = default;
  // Zip file can be opened and closed by hand in order to allow for quick calculation of inflated file size
  // It is NOT recommended to pre-open it for any kind of inflation due to memory constraints
  bool isOpen() const { return !!file; }
  bool open();
  bool close();
  bool loadAllFileStatSlims();
  bool getInflatedFileSize(const char* filename, size_t* size);
  // Batch lookup: scan ZIP central dir once and fill sizes for matching targets.
  // targets must be sorted by (hash, len). sizes[target.index] receives uncompressedSize.
  // Returns number of targets matched.
  int fillUncompressedSizes(std::deque<SizeTarget>& targets, std::deque<uint32_t>& sizes);
  // Due to the memory required to run each of these, it is recommended to not preopen the zip file for multiple
  // These functions will open and close the zip as needed
  uint8_t* readFileToMemory(const char* filename, size_t* size = nullptr, bool trailingNullByte = false);
  // allowEarlyStop: a short write from `out` is treated as the sink asking to
  // stop (returns true) instead of a write failure — used by header probes
  // that only need the first bytes of an entry.
  bool readFileToStream(const char* filename, Print& out, size_t chunkSize, bool allowEarlyStop = false);

  template <typename F>
  bool enumerateFilePaths(F&& callback) {
    if (!fileStatSlimCache.empty()) {
      for (const auto& entry : fileStatSlimCache) {
        callback(std::string_view{entry.first});
      }
      return true;
    }

    return enumerateFileEntries([&callback](std::string_view path, uint32_t, uint32_t) { callback(path); });
  }

  // Callback receives (path, crc32, compressedSize) for each central-directory
  // entry. Always scans the central directory: the slim-stat cache does not
  // hold CRCs.
  template <typename F>
  bool enumerateFileEntries(F&& callback) {
    auto invoke = [&callback](const char* name, const size_t nameLen, const bb_zip_entry* entry) {
      callback(std::string_view{name, nameLen}, entry->crc32, static_cast<uint32_t>(entry->compressed_size));
    };
    return forEachEntry(
        [](const char* name, const size_t nameLen, const bb_zip_entry* entry, void* context) {
          (*static_cast<decltype(invoke)*>(context))(name, nameLen, entry);
          return true;
        },
        &invoke);
  }
};
