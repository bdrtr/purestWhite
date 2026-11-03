#include "BookmarkFile.h"

#include <HalStorage.h>
#include <Logging.h>
#include <PersistableStore.h>
#include <StoreText.h>
#include <bembeyaz/store_bookmarks.h>

#include <algorithm>
#include <cstddef>

#include "BookmarkUtil.h"

// The JSON is Rust's (bembeyaz_store::bookmarks); this file cuts long names when it loads, refuses to
// save a bookmark whose name is longer, and does the file steps through PersistableStoreBase.
namespace {
static_assert(offsetof(bb_store_bookmark, percentage) == 3 * sizeof(bb_store_text) &&
                  offsetof(bb_store_bookmark, has_visible_text_offset) == 3 * sizeof(bb_store_text) + 10 &&
                  offsetof(bb_store_bookmark, visible_text_offset) == 3 * sizeof(bb_store_text) + 12 &&
                  sizeof(bb_store_bookmark) == 3 * sizeof(bb_store_text) + 16,
              "bb_store_bookmark layout");

void addRead(const bb_store_bookmark* stored, const size_t count, void* context) {
  auto& bookmarks = *static_cast<std::vector<BookmarkEntry>*>(context);
  if (bookmarks.empty()) bookmarks.reserve(count);
  BookmarkEntry& bookmark = bookmarks.emplace_back();
  bookmark.xpath = storeString(stored->xpath);
  bookmark.percentage = stored->percentage;
  bookmark.summary = storeString(stored->summary);
  bookmark.name = storeString(stored->name);
  if (bookmark.name.size() > BookmarkEntry::MAX_NAME_LENGTH) {
    bookmark.name.resize(BookmarkEntry::MAX_NAME_LENGTH);
  }
  bookmark.computedSpineIndex = stored->spine_index;
  bookmark.computedChapterPageCount = stored->chapter_page_count;
  bookmark.computedChapterProgress = stored->chapter_progress;
  bookmark.hasVisibleTextOffset = stored->has_visible_text_offset;
  bookmark.visibleTextOffset = stored->visible_text_offset;
}
}  // namespace

bool BookmarkFile::load(const std::string& bookPath, std::vector<BookmarkEntry>& bookmarks) {
  bookmarks.clear();

  const std::string path = BookmarkUtil::getBookmarkPath(bookPath);
  String json;
  if (!PersistableStoreBase::readTextFromFile(path.c_str(), json)) {
    return false;
  }
  const char* error = bb_store_bookmarks_read(json.c_str(), json.length(), addRead, &bookmarks);
  if (error) {
    PersistableStoreBase::logParseError(path.c_str(), error);
    return false;
  }

  LOG_DBG("BKM", "Loaded %zu bookmarks from file", bookmarks.size());
  return true;
}

bool BookmarkFile::save(const std::string& bookPath, const std::vector<BookmarkEntry>& bookmarks) {
  for (const auto& bookmark : bookmarks) {
    if (bookmark.name.size() > BookmarkEntry::MAX_NAME_LENGTH) {
      LOG_ERR("BKM", "Bookmark name exceeds %zu bytes", BookmarkEntry::MAX_NAME_LENGTH);
      return false;
    }
  }

  LOG_DBG("BKM", "Saving %zu bookmarks to file", bookmarks.size());
  std::vector<bb_store_bookmark> stored(bookmarks.size());
  for (size_t i = 0; i < bookmarks.size(); i++) {
    const BookmarkEntry& b = bookmarks[i];
    stored[i] = {storeText(b.xpath),   storeText(b.summary),       storeText(b.name),         b.percentage,
                 b.computedSpineIndex, b.computedChapterPageCount, b.computedChapterProgress, b.hasVisibleTextOffset,
                 b.visibleTextOffset};
  }

  String json;
  StoreJsonSink sink{json, true};
  if (!bb_store_bookmarks_write(stored.data(), stored.size(), appendStoreJson, &sink) || !sink.ok) {
    LOG_ERR("BKM", "Failed to make the bookmarks file");
    return false;
  }
  // writeTextToFile ensures /.crosspoint; the bookmarks subdirectory is ours.
  Storage.mkdir(BookmarkUtil::getBookmarksDir().c_str());
  const std::string path = BookmarkUtil::getBookmarkPath(bookPath);
  return PersistableStoreBase::writeTextToFile(path.c_str(), json);
}
