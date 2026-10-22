#include "ClippingStore.h"

#include <BufferedFile.h>
#include <Epub/FileIo.h>
#include <HalMemory.h>
#include <HalStorage.h>
#include <Logging.h>
#include <Memory.h>
#include <Utf8.h>
#include <bembeyaz/clippings.h>
#include <esp_random.h>

#include <algorithm>
#include <array>
#include <cstddef>
#include <cstring>
#include <ctime>
#include <functional>

#include "clippings/ClippingPreview.h"

namespace {
constexpr char CLIPPINGS_DIR[] = "/.crosspoint/clippings";
constexpr size_t HEADER_STRING_MAX = CLIPPING_TEXT_MAX;
// Read buffer for loading the store; larger than one version 4 record (156 bytes).
constexpr size_t LOAD_BUFFER_SIZE = 256;
// Writes to the temporary file in whole, sector-aligned 8 KiB blocks, so reads from the old file
// keep SdFat's single sector cache. Above the framework's 4 KiB always-internal limit, malloc tries
// PSRAM first.
constexpr size_t WRITE_BUFFER_SIZE = 8192;

static_assert(BB_CLIP_TEXT_MAX == CLIPPING_TEXT_MAX && BB_CLIP_ID_LEN == sizeof(Clipping::id) &&
              BB_CLIP_CHAPTER_LEN == sizeof(Clipping::chapterTitle));
// bembeyaz-ffi asserts the same layout for its Record.
static_assert(sizeof(bb_clip_record) == 168 && offsetof(bb_clip_record, sync_revision) == 32 &&
              offsetof(bb_clip_record, pending_upload) == 105 && offsetof(bb_clip_record, chapter) == 106 &&
              offsetof(bb_clip_record, text_offset) == 156 && offsetof(bb_clip_record, text_length) == 160);

void toRecord(const Clipping& c, bb_clip_record& r) {
  r.spine = c.spineIndex;
  r.start_page = c.startPage;
  r.end_page = c.endPage;
  r.page_count = c.pageCount;
  r.start_word = c.startWordIndex;
  r.end_word = c.endWordIndex;
  r.word_count = c.wordCount;
  r.paragraph = c.paragraphIndex;
  r.timestamp = c.timestamp;
  r.layout = c.layoutSignature;
  r.start_offset = c.startOffset;
  r.end_offset = c.endOffset;
  r.sync_revision = c.syncRevision;
  memcpy(r.id, c.id, sizeof(r.id));
  r.pending_upload = c.pendingUpload;
  memcpy(r.chapter, c.chapterTitle, sizeof(r.chapter));
  r.text_offset = c.textOffset;
  r.text_length = c.textLength;
}

void fromRecord(const bb_clip_record& r, Clipping& c) {
  c.spineIndex = r.spine;
  c.startPage = r.start_page;
  c.endPage = r.end_page;
  c.pageCount = r.page_count;
  c.startWordIndex = r.start_word;
  c.endWordIndex = r.end_word;
  c.wordCount = r.word_count;
  c.paragraphIndex = r.paragraph;
  c.timestamp = r.timestamp;
  c.layoutSignature = r.layout;
  c.startOffset = r.start_offset;
  c.endOffset = r.end_offset;
  c.syncRevision = r.sync_revision;
  memcpy(c.id, r.id, sizeof(c.id));
  c.pendingUpload = r.pending_upload;
  memcpy(c.chapterTitle, r.chapter, sizeof(c.chapterTitle));
  c.textOffset = r.text_offset;
  c.textLength = r.text_length;
}

bool truncateDeletionTail(HalFile& journal) {
  const size_t size = journal.size();
  const size_t tail = size % sizeof(Clipping::id);
  if (tail == 0) return true;
  if (!journal.truncate(size - tail)) {
    LOG_ERR("CLIP", "Failed to recover clipping deletion journal");
    return false;
  }
  return true;
}

std::string storeFilePathForBook(const std::string& filePath, const std::string& bookType) {
  return std::string(CLIPPINGS_DIR) + "/" + bookType + "_" + std::to_string(std::hash<std::string>{}(filePath)) +
         ".bin";
}

void copyBounded(char* dst, const size_t dstSize, const char* src) {
  if (dstSize == 0) return;
  if (!src) src = "";
  snprintf(dst, dstSize, "%s", src);
  dst[utf8SafeTruncateBuffer(dst, static_cast<int>(strlen(dst)))] = '\0';
}

constexpr const char* STORE_SUFFIXES[] = {"", ".bak", ".tmp", ".deleted", ".migrate.bak"};
enum class MovePhase { Check, Move, Rollback };

bool moveStoreFiles(const std::string& from, const std::string& to, const MovePhase phase) {
  const std::string source = storeFilePathForBook(from, "epub");
  const std::string target = storeFilePathForBook(to, "epub");
  if (source == target) return true;
  return std::all_of(std::begin(STORE_SUFFIXES), std::end(STORE_SUFFIXES), [&](const char* suffix) {
    const std::string oldPath = source + suffix;
    const std::string newPath = target + suffix;
    if (phase == MovePhase::Check) {
      if (Storage.exists(newPath.c_str())) {
        LOG_ERR("CLIP", "Move would overwrite saved clippings: %s", to.c_str());
        return false;
      }
    } else {
      const auto& src = phase == MovePhase::Move ? oldPath : newPath;
      const auto& dst = phase == MovePhase::Move ? newPath : oldPath;
      if (Storage.exists(src.c_str()) && !Storage.rename(src.c_str(), dst.c_str())) {
        LOG_ERR("CLIP", "Failed moving clipping state: %s -> %s", src.c_str(), dst.c_str());
        return false;
      }
    }
    return true;
  });
}

struct MoveDirectory {
  HalFile file;
  std::string from;
  std::string to;
};
// ponytail: cap directory depth at 32; grow this checked work stack if deeper book folders are needed.
constexpr size_t MAX_MOVE_DEPTH = 32;

bool moveTreeStores(const std::string& from, const std::string& to, const MovePhase phase, MoveDirectory* stack,
                    char* name) {
  for (size_t i = 0; i < MAX_MOVE_DEPTH; ++i) {
    if (stack[i].file) stack[i].file.close();
  }
  HalFile root = Storage.open(from.c_str());
  if (!root || !root.isDirectory()) return false;
  stack[0] = {std::move(root), from, to};
  size_t depth = 1;
  while (depth) {
    auto& directory = stack[depth - 1];
    HalFile entry = directory.file.openNextFile();
    if (!entry) {
      directory.file.close();
      --depth;
      continue;
    }
    const size_t length = entry.getName(name, 1024);
    if (length == 0 || length >= 1024) {
      LOG_ERR("CLIP", "Invalid name in folder move");
      return false;
    }
    if (!strcmp(name, ".") || !strcmp(name, "..")) continue;
    const std::string oldPath = directory.from + (directory.from.back() == '/' ? "" : "/") + name;
    const std::string newPath = directory.to + (directory.to.back() == '/' ? "" : "/") + name;
    if (entry.isDirectory()) {
      if (depth == MAX_MOVE_DEPTH) {
        LOG_ERR("CLIP", "Folder move exceeds directory depth limit");
        return false;
      }
      stack[depth++] = {std::move(entry), oldPath, newPath};
    } else if (!moveStoreFiles(oldPath, newPath, phase)) {
      return false;
    }
  }
  return true;
}
}  // namespace

ClippingStore ClippingStore::instance;

bool ClippingStore::loadForBook(const std::string& filePath, const std::string& title, const std::string& author,
                                const std::string& bookType) {
  dirty = false;
  loaded = false;
  for (auto& block : clippingBlocks) block.reset();
  clippingSize = clippingCapacity = 0;
  storeFilePath.clear();
  if (bookType != "epub" || filePath.empty()) {
    LOG_ERR("CLIP", "Invalid clipping book type or path: %s", bookType.c_str());
    return false;
  }

  bookFilePath = filePath;
  bookTitle = title;
  bookAuthor = author;

  storeFilePath = storeFilePathForBook(filePath, bookType);
  const std::string backup = storeFilePath + ".bak";
  if (!Storage.exists(storeFilePath.c_str()) && Storage.exists(backup.c_str()) &&
      !Storage.rename(backup.c_str(), storeFilePath.c_str())) {
    LOG_ERR("CLIP", "Failed to recover clipping backup");
    return false;
  }
  if (!Storage.exists(storeFilePath.c_str())) {
    loaded = true;
    return true;
  }
  loaded = readFromFile();
  if (!loaded) {
    for (auto& block : clippingBlocks) block.reset();
    clippingSize = clippingCapacity = 0;
  }
  return loaded;
}

void ClippingStore::unload() {
  if (dirty) saveToFile();
  for (auto& block : clippingBlocks) block.reset();
  clippingSize = clippingCapacity = 0;
  loaded = false;
  bookFilePath.clear();
  bookTitle.clear();
  bookAuthor.clear();
  storeFilePath.clear();
  dirty = false;
}

ClippingStore::AddResult ClippingStore::addClipping(const uint16_t spineIndex, const uint16_t startPage,
                                                    const uint16_t endPage, const uint16_t pageCount,
                                                    const uint16_t startWordIndex, const uint16_t endWordIndex,
                                                    const uint16_t wordCount, const char* chapterTitle,
                                                    const uint16_t paragraphIndex, const std::string& text,
                                                    const uint32_t layoutSignature, const uint32_t startOffset,
                                                    const uint32_t endOffset) {
  if (clippingSize >= CLIPPING_MAX_PER_BOOK) {
    LOG_ERR("CLIP", "Clipping limit (%u) reached", CLIPPING_MAX_PER_BOOK);
    return AddResult::LimitReached;
  }

  if (!loaded || text.size() > CLIPPING_TEXT_MAX || !reserveClippings(clippingSize + 1)) {
    LOG_ERR("CLIP", "Cannot add clipping: invalid store, text length, or allocation failure");
    return AddResult::SaveFailed;
  }
  Clipping clipping;
  clipping.spineIndex = spineIndex;
  clipping.startPage = startPage;
  clipping.endPage = endPage;
  clipping.pageCount = std::max<uint16_t>(1, pageCount);
  clipping.startWordIndex = startWordIndex;
  clipping.endWordIndex = endWordIndex;
  clipping.wordCount = wordCount;
  clipping.paragraphIndex = paragraphIndex;
  const time_t now = time(nullptr);
  clipping.timestamp = now > 1577836800 ? static_cast<uint32_t>(now) : 0;
  clipping.layoutSignature = layoutSignature;
  clipping.startOffset = startOffset;
  clipping.endOffset = endOffset;
  copyBounded(clipping.chapterTitle, sizeof(clipping.chapterTitle), chapterTitle);
  clipping.textLength = static_cast<uint16_t>(std::min(text.size(), CLIPPING_TEXT_MAX));

  clippingRef(clippingSize++) = clipping;
  dirty = true;
  if (!writeToFile(&text, clippingSize - 1)) {
    --clippingSize;
    dirty = true;
    return AddResult::SaveFailed;
  }
  dirty = false;
  return AddResult::Added;
}

bool ClippingStore::removeClippingAt(const size_t index) {
  if (!loaded || index >= clippingSize) return false;
  if (clippingRef(index).id[0]) {
    HalFile journal = Storage.open((storeFilePath + ".deleted").c_str(), O_WRONLY | O_CREAT | O_APPEND);
    if (!journal || !truncateDeletionTail(journal)) {
      LOG_ERR("CLIP", "Failed to open clipping deletion journal");
      return false;
    }
    const size_t originalSize = journal.size();
    if (journal.write(reinterpret_cast<const uint8_t*>(clippingRef(index).id), sizeof(clippingRef(index).id)) !=
        sizeof(clippingRef(index).id)) {
      if (!journal.truncate(originalSize)) LOG_ERR("CLIP", "Failed to roll back clipping deletion journal");
      LOG_ERR("CLIP", "Failed to queue clipping deletion");
      return false;
    }
    journal.flush();
  }
  Clipping clipping = std::move(clippingRef(index));
  for (size_t i = index; i + 1 < clippingSize; ++i) clippingRef(i) = clippingRef(i + 1);
  --clippingSize;
  dirty = true;
  if (!saveToFile()) {
    for (size_t i = clippingSize; i > index; --i) clippingRef(i) = clippingRef(i - 1);
    clippingRef(index) = clipping;
    ++clippingSize;
    dirty = true;
    return false;
  }
  return true;
}

const Clipping* ClippingStore::clippingAt(const size_t index) const {
  if (index >= clippingSize) return nullptr;
  return &clippingBlocks[index / CLIPPINGS_PER_BLOCK][index % CLIPPINGS_PER_BLOCK];
}

bool ClippingStore::readClippingPreview(const size_t index, char* out, const size_t outSize, size_t& outLength) const {
  outLength = 0;
  if (out && outSize > 0) out[0] = '\0';
  const Clipping* clipping = clippingAt(index);
  if (!out || outSize == 0 || !clipping || storeFilePath.empty()) {
    LOG_ERR("CLIP", "Invalid clipping preview index: %u", static_cast<unsigned>(index));
    return false;
  }
  if (clipping->textLength == 0) return true;

  HalFile f;
  if (!Storage.openFileForRead("CLIP", storeFilePath, f)) return false;
  if (!f.seek(clipping->textOffset)) {
    LOG_ERR("CLIP", "Failed to seek clipping preview at %u", clipping->textOffset);
    return false;
  }
  const bool ok = clippingPreview::read(f, clipping->textLength, out, outSize, outLength);
  if (!ok) LOG_ERR("CLIP", "Failed to read clipping preview at %u", clipping->textOffset);
  return ok;
}

bool ClippingStore::readClippingText(const size_t index, std::string& out) const {
  const Clipping* clipping = clippingAt(index);
  if (!clipping) return false;
  return readClippingText(*clipping, out);
}

bool ClippingStore::readClippingText(const Clipping& clipping, std::string& out) const {
  out.clear();
  if (clipping.textLength == 0) return true;
  if (storeFilePath.empty()) return false;

  HalFile f;
  if (!Storage.openFileForRead("CLIP", storeFilePath, f)) {
    return false;
  }
  if (!f.seek(clipping.textOffset)) {
    LOG_ERR("CLIP", "Failed to seek clipping text at %u: %s", clipping.textOffset, storeFilePath.c_str());
    return false;
  }
  out.resize(clipping.textLength);
  const int expected = static_cast<int>(clipping.textLength);
  const bool ok = f.read(&out[0], clipping.textLength) == expected;
  if (!ok) {
    out.clear();
    LOG_ERR("CLIP", "Failed to read clipping text at %u: %s", clipping.textOffset, storeFilePath.c_str());
  }
  return ok;
}

bool ClippingStore::saveToFile() {
  if (!loaded) return false;
  if (!dirty) return true;
  if (writeToFile()) {
    dirty = false;
    return true;
  }
  return false;
}

bool ClippingStore::prepareSync() {
  for (size_t index = 0; index < clippingSize; ++index) {
    Clipping& clipping = clippingRef(index);
    if (clipping.id[0]) continue;
    uint8_t bytes[16];
    esp_fill_random(bytes, sizeof(bytes));
    static constexpr char HEX_DIGITS[] = "0123456789abcdef";
    for (size_t i = 0; i < sizeof(bytes); ++i) {
      clipping.id[i * 2] = HEX_DIGITS[bytes[i] >> 4];
      clipping.id[i * 2 + 1] = HEX_DIGITS[bytes[i] & 15];
    }
    clipping.id[32] = '\0';
    dirty = true;
  }
  return saveToFile();
}

bool ClippingStore::finishUploads() {
  for (size_t index = 0; index < clippingSize; ++index) {
    Clipping& clipping = clippingRef(index);
    if (!clipping.pendingUpload) continue;
    clipping.pendingUpload = false;
    dirty = true;
  }
  return saveToFile();
}

bool ClippingStore::nextDeletion(uint32_t& offset, char (&id)[65]) const {
  id[0] = '\0';
  const std::string path = storeFilePath + ".deleted";
  if (!Storage.exists(path.c_str())) return true;
  HalFile journal = Storage.open(path.c_str(), O_RDWR);
  if (!journal || !truncateDeletionTail(journal)) return false;
  const bb_io_file file = file_io::fileOf(journal);
  const auto known = [](void* context, const char* stored) {
    const auto& store = *static_cast<const ClippingStore*>(context);
    for (size_t i = 0; i < store.clippingSize; ++i) {
      if (strcmp(store.clippingAt(i)->id, stored) == 0) return true;
    }
    return false;
  };
  if (!bb_clip_next_deletion(&file, &offset, id, const_cast<ClippingStore*>(this), known)) {
    LOG_ERR("CLIP", "Invalid or unreadable clipping deletion journal");
    return false;
  }
  return true;
}

bool ClippingStore::finishDeletions() {
  const std::string path = storeFilePath + ".deleted";
  if (!Storage.exists(path.c_str()) || Storage.remove(path.c_str())) return true;
  LOG_ERR("CLIP", "Failed to clear acknowledged deletions");
  return false;
}

bool ClippingStore::applyRemote(const Clipping& clipping, const std::string& text, const bool deleted) {
  if (!loaded || (!deleted && text.size() > CLIPPING_TEXT_MAX)) return false;
  size_t index = 0;
  while (index < clippingSize && strcmp(clippingRef(index).id, clipping.id) != 0) ++index;
  if (index == clippingSize) {
    if (deleted) return true;
    if (clippingSize >= CLIPPING_MAX_PER_BOOK || !reserveClippings(clippingSize + 1)) {
      LOG_ERR("CLIP", "Remote clipping exceeds local capacity");
      return false;
    }
    clippingRef(clippingSize++) = clipping;
    if (!writeToFile(&text, index)) {
      --clippingSize;
      return false;
    }
    return true;
  }
  if (!deleted && clippingRef(index).syncRevision == clipping.syncRevision && !clippingRef(index).pendingUpload)
    return true;
  const Clipping previous = clippingRef(index);
  if (deleted) {
    for (size_t i = index; i + 1 < clippingSize; ++i) clippingRef(i) = clippingRef(i + 1);
    --clippingSize;
  } else {
    clippingRef(index) = clipping;
  }
  if (writeToFile(deleted ? nullptr : &text, index)) return true;
  if (deleted) {
    for (size_t i = clippingSize; i > index; --i) clippingRef(i) = clippingRef(i - 1);
    ++clippingSize;
  }
  clippingRef(index) = previous;
  return false;
}

bool ClippingStore::reserveClippings(const size_t count) {
  if (count > CLIPPING_MAX_PER_BOOK) return false;
  while (clippingCapacity < count) {
    auto block = makeUniqueNoThrow<Clipping[]>(CLIPPINGS_PER_BLOCK);
    if (!block) {
      LOG_ERR("CLIP", "OOM: clipping block (%u bytes)", static_cast<unsigned>(CLIPPINGS_PER_BLOCK * sizeof(Clipping)));
      return false;
    }
    clippingBlocks[clippingCapacity / CLIPPINGS_PER_BLOCK] = std::move(block);
    clippingCapacity += CLIPPINGS_PER_BLOCK;
  }
  return true;
}

bool ClippingStore::readFromFile() {
  const auto& path = storeFilePath;
  clippingSize = 0;
  HalFile f;
  if (!Storage.openFileForRead("CLIP", path, f)) {
    return false;
  }

  struct Loading {
    ClippingStore& store;
    std::string title;
    std::string author;
  } loading{*this};
  const bb_clip_target target{
      &loading,
      [](void* context, const uint8_t field, const size_t len) -> char* {
        auto& l = *static_cast<Loading*>(context);
        // Only an empty title or author takes the stored one.
        if (field == BB_CLIP_PATH || !(field == BB_CLIP_TITLE ? l.store.bookTitle : l.store.bookAuthor).empty()) {
          return nullptr;
        }
        auto& text = field == BB_CLIP_TITLE ? l.title : l.author;
        text.resize(len);
        return text.data();
      },
      [](void* context, const uint16_t count) { return static_cast<Loading*>(context)->store.reserveClippings(count); },
      [](void* context, const bb_clip_record* record) {
        auto& store = static_cast<Loading*>(context)->store;
        if (store.clippingSize == store.clippingCapacity) return;
        Clipping clipping;
        fromRecord(*record, clipping);
        store.clippingRef(store.clippingSize++) = clipping;
      }};
  const bb_io_file file = file_io::fileOf(f);
  uint8_t buf[LOAD_BUFFER_SIZE];
  const int result = bb_clip_load(&file, buf, sizeof(buf), CLIPPING_MAX_PER_BOOK, &target);
  if (result != BB_CLIP_LOADED) {
    if (result == BB_CLIP_DAMAGED) LOG_ERR("CLIP", "Clipping file damaged or cut: %s", path.c_str());
    return false;
  }
  if (bookTitle.empty()) bookTitle = std::move(loading.title);
  if (bookAuthor.empty()) bookAuthor = std::move(loading.author);
  return true;
}

bool ClippingStore::writeToFile(const std::string* replacementText, const size_t replacementIndex) {
  if (!loaded || storeFilePath.empty() || bookTitle.size() > HEADER_STRING_MAX ||
      bookAuthor.size() > HEADER_STRING_MAX || bookFilePath.size() > HEADER_STRING_MAX ||
      clippingSize > CLIPPING_MAX_PER_BOOK || (replacementText && replacementText->size() > CLIPPING_TEXT_MAX)) {
    LOG_ERR("CLIP", "Refusing to write invalid or incompletely loaded clipping store");
    return false;
  }
  const uint16_t count = static_cast<uint16_t>(clippingSize);
  auto newTextOffsets = makeUniqueNoThrow<uint32_t[]>(count);
  if (count && !newTextOffsets) {
    LOG_ERR("CLIP", "OOM: clipping text offsets");
    return false;
  }
  Storage.mkdir("/.crosspoint");
  Storage.mkdir(CLIPPINGS_DIR);

  const std::string tmpPath = storeFilePath + ".tmp";
  const std::string backupPath = storeFilePath + ".bak";
  if (!Storage.exists(storeFilePath.c_str()) && Storage.exists(backupPath.c_str())) {
    if (!Storage.rename(backupPath.c_str(), storeFilePath.c_str())) {
      LOG_ERR("CLIP", "Failed to recover clipping backup: %s", backupPath.c_str());
      return false;
    }
    LOG_INF("CLIP", "Recovered clipping backup: %s", storeFilePath.c_str());
  }
  if (Storage.exists(tmpPath.c_str())) Storage.remove(tmpPath.c_str());
  if (Storage.exists(backupPath.c_str()) && Storage.exists(storeFilePath.c_str())) Storage.remove(backupPath.c_str());

  HalFile source;
  const std::string& sourcePath = storeFilePath;
  const bool hasTextSource = Storage.exists(sourcePath.c_str());
  const bool hasDestination = Storage.exists(storeFilePath.c_str());
  if (hasTextSource && !Storage.openFileForRead("CLIP", sourcePath, source)) {
    LOG_ERR("CLIP", "Failed to open clipping source for rewrite: %s", sourcePath.c_str());
    return false;
  }

  // Each old text is copied in one read and one write: small chunks make SdFat's single sector
  // cache swap between the two files on every chunk. PSRAM is fine, as the SD driver copies
  // through its own DMA buffer; without it the copy is slower, not failed.
  const auto copyBuffer = source ? HalMemory::allocatePsram(CLIPPING_TEXT_MAX) : nullptr;
  uint8_t smallBuffer[128];
  uint8_t* const copy = copyBuffer ? copyBuffer.get() : smallBuffer;
  const size_t copyLen = copyBuffer ? CLIPPING_TEXT_MAX : sizeof(smallBuffer);

  HalFile f = Storage.open(tmpPath.c_str(), O_WRONLY | O_CREAT | O_TRUNC);
  if (!f) {
    if (source) source.close();
    LOG_ERR("CLIP", "Failed to open clipping temp file for write: %s", tmpPath.c_str());
    return false;
  }

  const bb_clip_records records{this, [](void* context, const uint16_t index, bb_clip_record* out) {
                                  toRecord(static_cast<ClippingStore*>(context)->clippingRef(index), *out);
                                }};
  const char* const header[3] = {bookTitle.data(), bookAuthor.data(), bookFilePath.data()};
  const size_t headerLen[3] = {bookTitle.size(), bookAuthor.size(), bookFilePath.size()};
  const bb_io_file sourceFile = source ? file_io::fileOf(source) : bb_io_file{};
  const std::string* replacement = replacementIndex < count ? replacementText : nullptr;
  bool written = false;
  {
    serialization::BufferedFileWriter buffered(f, WRITE_BUFFER_SIZE);
    const bb_io_sink sink = file_io::sinkOf(buffered);
    written = bb_clip_save(&sink, source ? &sourceFile : nullptr, copy, copyLen, header, headerLen, count, &records,
                           static_cast<uint16_t>(replacement ? replacementIndex : 0),
                           replacement ? replacement->data() : nullptr, replacement ? replacement->size() : 0,
                           newTextOffsets.get()) &&
              buffered.flush();
  }
  if (!written) {
    LOG_ERR("CLIP", "Failed to write clipping file: %s", tmpPath.c_str());
    f.close();
    if (source) source.close();
    Storage.remove(tmpPath.c_str());
    return false;
  }

  f.flush();
  f.close();
  if (source) source.close();

  if (hasDestination && !Storage.rename(storeFilePath.c_str(), backupPath.c_str())) {
    LOG_ERR("CLIP", "Failed to back up clipping file: %s", storeFilePath.c_str());
    Storage.remove(tmpPath.c_str());
    return false;
  }
  if (!Storage.rename(tmpPath.c_str(), storeFilePath.c_str())) {
    LOG_ERR("CLIP", "Failed to replace clipping file: %s", storeFilePath.c_str());
    Storage.remove(tmpPath.c_str());
    if (hasDestination) Storage.rename(backupPath.c_str(), storeFilePath.c_str());
    return false;
  }
  if (hasDestination && Storage.exists(backupPath.c_str())) {
    Storage.remove(backupPath.c_str());
  }
  for (uint16_t i = 0; i < count; ++i) {
    clippingRef(i).textOffset = newTextOffsets[i];
  }
  return true;
}

bool ClippingStore::deleteForFilePath(const std::string& filePath, const std::string& bookType) {
  const std::string path = storeFilePathForBook(filePath, bookType);
  bool ok = true;
  for (const char* suffix : STORE_SUFFIXES) {
    const std::string sidecar = path + suffix;
    if (Storage.exists(sidecar.c_str()) && !Storage.remove(sidecar.c_str())) {
      LOG_ERR("CLIP", "Failed deleting clipping state: %s", sidecar.c_str());
      ok = false;
    }
  }
  return ok;
}

bool ClippingStore::moveBook(const std::string& from, const std::string& to) {
  if (from == to) return true;
  if (Storage.exists(to.c_str())) return false;
  bool directory = false;
  {
    HalFile root = Storage.open(from.c_str());
    if (!root) return false;
    directory = root.isDirectory();
  }
  // Reuse one checked heap workspace for preflight, move, and rollback.
  auto stack = directory ? makeUniqueNoThrow<MoveDirectory[]>(MAX_MOVE_DEPTH) : nullptr;
  auto name = directory ? makeUniqueNoThrow<char[]>(1024) : nullptr;
  if (directory && (!stack || !name)) {
    LOG_ERR("CLIP", "OOM: folder move work stack");
    return false;
  }
  const auto move = [&](const MovePhase phase) {
    return directory ? moveTreeStores(from, to, phase, stack.get(), name.get()) : moveStoreFiles(from, to, phase);
  };
  if (!move(MovePhase::Check)) return false;
  if (move(MovePhase::Move) && Storage.rename(from.c_str(), to.c_str())) return true;
  if (!move(MovePhase::Rollback)) {
    LOG_ERR("CLIP", "Book move rollback failed: %s -> %s", from.c_str(), to.c_str());
  }
  return false;
}
