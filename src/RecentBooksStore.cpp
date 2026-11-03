#include "RecentBooksStore.h"

#include <HalStorage.h>
#include <Logging.h>
#include <StoreText.h>
#include <bembeyaz/store_recent_books.h>

#include <algorithm>
#include <cstddef>

// The JSON is Rust's (bembeyaz_store::recent_books); this file passes the books to Rust and takes
// them back. Adding, moving and pruning books stays here.
namespace {
static_assert(sizeof(bb_store_recent_book) == 4 * sizeof(bb_store_text) &&
                  offsetof(bb_store_recent_book, title) == sizeof(bb_store_text) &&
                  offsetof(bb_store_recent_book, cover_bmp_path) == 3 * sizeof(bb_store_text),
              "bb_store_recent_book layout");

void addRead(const bb_store_recent_book* book, const size_t count, void* context) {
  auto* books = static_cast<std::vector<RecentBook>*>(context);
  if (books->empty()) books->reserve(count);
  books->push_back({storeString(book->path), storeString(book->title), storeString(book->author),
                    storeString(book->cover_bmp_path)});
}
}  // namespace

bool RecentBooksStore::toJsonText(String& json) const {
  std::vector<bb_store_recent_book> books;
  books.reserve(recentBooks.size());
  for (const auto& book : recentBooks) {
    books.push_back(
        {storeText(book.path), storeText(book.title), storeText(book.author), storeText(book.coverBmpPath)});
  }
  StoreJsonSink sink{json, true};
  return bb_store_recent_books_write(books.data(), books.size(), appendStoreJson, &sink) && sink.ok;
}

const char* RecentBooksStore::fromJsonText(const String& json) {
  // A missing or invalid 'books' key reads as an empty list; only a JSON parse error is fatal.
  std::vector<RecentBook> books;
  const char* error = bb_store_recent_books_read(json.c_str(), json.length(), MAX_RECENT_BOOKS, addRead, &books);
  if (error) return error;
  recentBooks = std::move(books);
  LOG_DBG("RBS", "Recent books loaded from file (%d entries)", getCount());
  return nullptr;
}

void RecentBooksStore::addBook(const std::string& path, const std::string& title, const std::string& author,
                               const std::string& coverBmpPath) {
  // Drop stale entries first so a new add can't evict a valid book in their stead.
  pruneMissing();

  auto it =
      std::find_if(recentBooks.begin(), recentBooks.end(), [&](const RecentBook& book) { return book.path == path; });
  if (it != recentBooks.end()) {
    // Move the existing entry to the front instead of copying it. The reader calls this after its first
    // render, so fresh string buffers would land among reader allocations and outlive them, splitting the
    // largest free block once the reader exits.
    std::rotate(recentBooks.begin(), it, it + 1);
    RecentBook& book = recentBooks.front();
    if (book.title != title) book.title = title;
    if (book.author != author) book.author = author;
    if (book.coverBmpPath != coverBmpPath) book.coverBmpPath = coverBmpPath;
  } else {
    recentBooks.insert(recentBooks.begin(), {path, title, author, coverBmpPath});
    if (recentBooks.size() > MAX_RECENT_BOOKS) {
      recentBooks.resize(MAX_RECENT_BOOKS);
    }
  }

  saveToFile();
}

void RecentBooksStore::updateBook(const std::string& path, const std::string& title, const std::string& author,
                                  const std::string& coverBmpPath) {
  auto it =
      std::find_if(recentBooks.begin(), recentBooks.end(), [&](const RecentBook& book) { return book.path == path; });
  if (it != recentBooks.end()) {
    RecentBook& book = *it;
    book.title = title;
    book.author = author;
    book.coverBmpPath = coverBmpPath;
    saveToFile();
  }
}

bool RecentBooksStore::removeByPath(const std::string& path) {
  auto it =
      std::find_if(recentBooks.begin(), recentBooks.end(), [&](const RecentBook& book) { return book.path == path; });
  if (it == recentBooks.end()) {
    return false;
  }
  recentBooks.erase(it);
  if (!saveToFile()) {
    LOG_ERR("RBS", "Failed to persist removal of recent book: %s", path.c_str());
  }
  return true;
}

void RecentBooksStore::updatePath(const std::string& oldPath, const std::string& newPath,
                                  const std::string& oldCachePath, const std::string& newCachePath) {
  auto it = std::find_if(recentBooks.begin(), recentBooks.end(),
                         [&](const RecentBook& book) { return book.path == oldPath; });
  if (it == recentBooks.end()) {
    return;
  }
  it->path = newPath;
  if (!oldCachePath.empty() && !it->coverBmpPath.empty() && it->coverBmpPath.rfind(oldCachePath, 0) == 0) {
    it->coverBmpPath = newCachePath + it->coverBmpPath.substr(oldCachePath.size());
  }
  saveToFile();
}

bool RecentBooksStore::isMissing(const RecentBook& book) { return !Storage.exists(book.path.c_str()); }

bool RecentBooksStore::pruneMissing() {
  const size_t before = recentBooks.size();
  recentBooks.erase(std::remove_if(recentBooks.begin(), recentBooks.end(), &isMissing), recentBooks.end());
  return recentBooks.size() != before;
}
