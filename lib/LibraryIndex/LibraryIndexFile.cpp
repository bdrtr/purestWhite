#include "LibraryIndexFile.h"

#include <HalStorage.h>
#include <Logging.h>

#include <cstddef>

namespace library {

// Checked against rust/crates/bembeyaz-ffi/src/library.rs too.
static_assert(sizeof(BookIdentity) == 16 && offsetof(BookIdentity, fileSize) == 8, "bb_library_identity layout");
// bembeyaz-library's format::Order numbers.
static_assert(static_cast<int>(SortOrder::RecentAsc) == 0 && static_cast<int>(SortOrder::TitleAsc) == 2 &&
                  static_cast<int>(SortOrder::AuthorDesc) == 5,
              "SortOrder numbers");

LibraryIndexFile::~LibraryIndexFile() { close(); }

bool LibraryIndexFile::open(const char* path) { return openImpl(path, false); }

bool LibraryIndexFile::openForReconciliation(const char* path) { return openImpl(path, true); }

bool LibraryIndexFile::openImpl(const char* path, const bool acceptStaleFold) {
  close();
  readFailed = false;
  if (!Storage.openFileForRead("LIBIDX", path, file)) {
    readFailed = true;
    return false;
  }

  if (file.read(&head, sizeof(head)) != static_cast<int>(sizeof(head))) {
    readFailed = true;
    lastValidity = ClixValidity::SizeMismatch;
    file.close();
    return false;
  }

  lastValidity =
      acceptStaleFold ? validateHeaderStructure(head, file.fileSize64()) : validateHeader(head, file.fileSize64());
  if (lastValidity != ClixValidity::Ok) {
    LOG_INF("LIBIDX", "index rejected: %s", clixValidityName(lastValidity));
    file.close();
    return false;
  }
  opened = true;
  return true;
}

void LibraryIndexFile::close() {
  if (file.isOpen()) file.close();
  opened = false;
}

bool LibraryIndexFile::readAt(const uint32_t offset, void* dst, const size_t len) {
  if (!opened) return false;
  // Every offset handed to this function comes from the header, and the header
  // was validated against the real file size, so a short read means the card
  // changed under us rather than a bad computation.
  if (!file.seekSet(offset) || file.read(dst, len) != static_cast<int>(len)) {
    readFailed = true;
    return false;
  }
  return true;
}

bb_library_index_file LibraryIndexFile::source() {
  const auto readAtCallback = [](void* context, const uint32_t offset, void* dst, const size_t len) {
    return static_cast<LibraryIndexFile*>(context)->readAt(offset, dst, len);
  };
  const auto failCallback = [](void* context) { static_cast<LibraryIndexFile*>(context)->readFailed = true; };
  return {readAtCallback, failCallback, this};
}

uint16_t LibraryIndexFile::ordinalForRow(const SortOrder order, const uint16_t row) {
  if (!opened) return 0xFFFF;
  const bb_library_index_file src = source();
  return bb_library_ordinal_for_row(detail::rust(head), static_cast<uint8_t>(order), row, &src);
}

bool LibraryIndexFile::recentRowsFor(const BookIdentity* books, const size_t count, uint16_t* outRows) {
  for (size_t i = 0; i < count; i++) outRows[i] = 0xFFFF;
  if (!opened) return false;
  const bb_library_index_file src = source();
  const int result = bb_library_recent_rows_for(detail::rust(head), reinterpret_cast<const bb_library_identity*>(books),
                                                outRows, count, &src);
  if (result < 0) LOG_ERR("LIBIDX", "OOM: 4096-byte lookup chunk");
  return result > 0;
}

bool LibraryIndexFile::readRecord(const uint16_t ordinal, ClixRecord& out) {
  if (!opened) return false;
  const bb_library_index_file src = source();
  return bb_library_read_record(detail::rust(head), ordinal, &src, reinterpret_cast<bb_library_record*>(&out));
}

bool LibraryIndexFile::readPathHash(const ClixRecord& record, uint64_t& out) {
  out = 0;
  if (!opened) return false;
  const bb_library_index_file src = source();
  return bb_library_read_path_hash(detail::rust(head), reinterpret_cast<const bb_library_record*>(&record), &src, &out);
}

bool LibraryIndexFile::readString(const ClixRecord& record, const uint8_t what, std::string& out) {
  out.clear();
  if (!opened) return false;
  const auto assign = [](void* context, const char* s, const size_t len) {
    static_cast<std::string*>(context)->assign(s, len);
  };
  const bb_library_index_file src = source();
  return bb_library_read_string(detail::rust(head), reinterpret_cast<const bb_library_record*>(&record), what, &src,
                                assign, &out);
}

bool LibraryIndexFile::readName(const ClixRecord& record, std::string& out) {
  return readString(record, BB_LIBRARY_NAME, out);
}

bool LibraryIndexFile::readAuthor(const ClixRecord& record, std::string& out) {
  return readString(record, BB_LIBRARY_AUTHOR, out) && !out.empty();
}

// The book's own title, after the name and the author. Absent (length 0) for a
// book that never told us one, in which case the caller shows the filename.
bool LibraryIndexFile::readTitle(const ClixRecord& record, std::string& out) {
  return readString(record, BB_LIBRARY_TITLE, out) && !out.empty();
}

bool LibraryIndexFile::readSourceAuthor(const ClixRecord& record, std::string& out) {
  return readString(record, BB_LIBRARY_SOURCE_AUTHOR, out);
}

bool LibraryIndexFile::readPath(const ClixRecord& record, std::string& out) {
  return readString(record, BB_LIBRARY_PATH, out);
}

}  // namespace library
