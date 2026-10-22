#include "BookMetadataCache.h"

#include <BufferedFile.h>
#include <FileIo.h>
#include <Logging.h>
#include <Memory.h>
#include <Utf8.h>
#include <ZipFile.h>

#include <algorithm>
#include <deque>
#include <mutex>
#include <optional>

#include "FsHelpers.h"

// book.bin's format and version live in rust/crates/bembeyaz-book (cache.rs).
namespace {
constexpr char bookBinFile[] = "/book.bin";
constexpr char tmpSpineBinFile[] = "/spine.bin.tmp";
constexpr char tmpTocBinFile[] = "/toc.bin.tmp";
// Buffer size for the buildBookBin streams. 3 buffers x 4KB, transient (freed on
// return); 4KB = 8 SD sectors per transfer, enough to stop the sector-cache thrash.
constexpr size_t BUILD_IO_BUFFER_SIZE = 4096;
// The render task and the main loop both ask for sizes.
std::mutex sizeWindowLock;

// The strings (by BB_BOOK_ field) a bembeyaz-book read fills; an unset slot takes nothing.
struct Texts {
  std::string* fields[BB_BOOK_FIELD_COUNT] = {};
};

bb_book_target targetOf(Texts& texts) {
  return {&texts, [](void* c, const uint8_t field, const size_t len) -> char* {
            auto* text = field < BB_BOOK_FIELD_COUNT ? static_cast<Texts*>(c)->fields[field] : nullptr;
            if (!text) return nullptr;
            text->resize(len);
            return text->data();
          }};
}

// A small buffer for reads of a few fields.
constexpr size_t ENTRY_BUFFER_SIZE = 128;
}  // namespace

BookMetadataCache::BookMetadataCache(std::string cachePath)
    : cachePath(std::move(cachePath)), lutOffset(0), spineCount(0), tocCount(0), loaded(false), buildMode(false) {}
BookMetadataCache::~BookMetadataCache() = default;

/* ============= WRITING / BUILDING FUNCTIONS ================ */

bool BookMetadataCache::beginWrite() {
  buildMode = true;
  spineCount = 0;
  tocCount = 0;
  LOG_DBG("BMC", "Entering write mode");
  return true;
}

bool BookMetadataCache::beginContentOpfPass() {
  LOG_DBG("BMC", "Beginning content opf pass");

  // Open spine file for writing
  if (!Storage.openFileForWrite("BMC", cachePath + tmpSpineBinFile, spineFile)) {
    return false;
  }
  // Wrapper OOM is fine: createSpineEntry falls back to unbuffered writes.
  passOut = makeUniqueNoThrow<serialization::BufferedFileWriter>(spineFile, BUILD_IO_BUFFER_SIZE);
  return true;
}

bool BookMetadataCache::endContentOpfPass() {
  const bool flushed = !passOut || passOut->flush();
  passOut.reset();
  // Explicit close() required: member variable persists beyond function scope
  spineFile.close();
  if (!flushed) {
    LOG_ERR("BMC", "Failed writing spine tmp file");
  }
  return flushed;
}

bool BookMetadataCache::beginTocPass() {
  LOG_DBG("BMC", "Beginning toc pass");

  if (!Storage.openFileForRead("BMC", cachePath + tmpSpineBinFile, spineFile)) {
    return false;
  }
  if (!Storage.openFileForWrite("BMC", cachePath + tmpTocBinFile, tocFile)) {
    // Explicit close() required: member variable persists beyond function scope
    spineFile.close();
    return false;
  }

  spineHrefIndex.reset();
  if (spineCount >= LARGE_SPINE_THRESHOLD) {
    // 16 B per spine item, so a long book's index lands in PSRAM.
    spineHrefIndex = makeUniqueNoThrow<bb_book_index_entry[]>(spineCount);
    uint8_t buf[128];
    const bb_io_file file = file_io::fileOf(spineFile);
    const bb_book_records spine{&file, buf, sizeof(buf), spineCount};
    if (!spineHrefIndex || !bb_book_index_spine(&spine, spineHrefIndex.get())) {
      // TOC hrefs are then found by reading the spine file, as for a short book.
      LOG_ERR("BMC", "Could not index %d spine items, searching them instead", spineCount);
      spineHrefIndex.reset();
    } else {
      std::sort(spineHrefIndex.get(), spineHrefIndex.get() + spineCount,
                [](const bb_book_index_entry& a, const bb_book_index_entry& b) {
                  return a.hash < b.hash || (a.hash == b.hash && a.len < b.len);
                });
      LOG_DBG("BMC", "Using fast index for %d spine items", spineCount);
    }
  }

  // Wrapper OOM is fine: createTocEntry falls back to unbuffered writes.
  passOut = makeUniqueNoThrow<serialization::BufferedFileWriter>(tocFile, BUILD_IO_BUFFER_SIZE);
  return true;
}

bool BookMetadataCache::endTocPass() {
  const bool flushed = !passOut || passOut->flush();
  passOut.reset();
  if (!flushed) {
    LOG_ERR("BMC", "Failed writing toc tmp file");
  }
  // Explicit close() required: member variables persist beyond function scope
  tocFile.close();
  spineFile.close();

  spineHrefIndex.reset();

  return flushed;
}

bool BookMetadataCache::endWrite() {
  if (!buildMode) {
    LOG_DBG("BMC", "endWrite called but not in build mode");
    return false;
  }

  buildMode = false;
  LOG_DBG("BMC", "Wrote %d spine, %d TOC entries", spineCount, tocCount);
  return true;
}

namespace {
// What the build asks of the book: each spine item's href, then its size.
struct Items {
  ZipFile& zip;
  bool isZip;
  size_t rawSize;
  const std::optional<std::deque<uint32_t>>& spineSizes;
  std::string href;
};

bb_book_items itemsOf(Items& items) {
  return {&items,
          [](void* c, const size_t len) {
            auto& href = static_cast<Items*>(c)->href;
            href.resize(len);
            return href.data();
          },
          [](void* c, const uint16_t index) -> uint32_t {
            auto& items = *static_cast<Items*>(c);
            if (!items.isZip) return items.rawSize;
            size_t size = items.spineSizes ? (*items.spineSizes)[index] : 0;
            if (size == 0) {
              const std::string path = FsHelpers::normalisePath(items.href);
              if (!items.zip.getInflatedFileSize(path.c_str(), &size)) {
                LOG_ERR("BMC", "Warning: Could not get size for spine item: %s", path.c_str());
              }
            }
            return static_cast<uint32_t>(size);
          }};
}

// The batch size lookup's targets: each spine href's normalised path, 2,048 at a time.
struct SizeTargets {
  ZipFile& zip;
  uint16_t spineCount;
  std::deque<uint32_t>& sizes;
  std::deque<ZipFile::SizeTarget> targets;
  std::string href;
  int matched = 0;
};
}  // namespace

bool BookMetadataCache::buildBookBin(const std::string& epubPath, const BookMetadata& metadata) {
  // Open all three files, writing to meta, reading from spine and toc
  if (!Storage.openFileForWrite("BMC", cachePath + bookBinFile, bookFile)) {
    return false;
  }

  if (!Storage.openFileForRead("BMC", cachePath + tmpSpineBinFile, spineFile)) {
    // Explicit close() required: member variable persists beyond function scope
    bookFile.close();
    return false;
  }

  if (!Storage.openFileForRead("BMC", cachePath + tmpTocBinFile, tocFile)) {
    // Explicit close() required: member variables persist beyond function scope
    bookFile.close();
    spineFile.close();
    return false;
  }
  const auto closeAll = [this]() {
    // Explicit close() required: member variables persist beyond function scope
    bookFile.close();
    spineFile.close();
    tocFile.close();
  };

  // Buffered streams for the whole build: every access is sequential per file, but interleaved
  // ACROSS files, which thrashes SdFat's single shared sector cache when unbuffered (one 512B SD
  // transaction per 4-byte pod -- measured 31s for a 1,732-spine omnibus). Three 4KB buffers (the
  // two readers here, the book.bin writer below), freed on return; a reader without one uses a
  // small stack buffer.
  auto spineBuf = makeUniqueNoThrow<uint8_t[]>(BUILD_IO_BUFFER_SIZE);
  auto tocBuf = makeUniqueNoThrow<uint8_t[]>(BUILD_IO_BUFFER_SIZE);
  if (!spineBuf || !tocBuf) {
    LOG_ERR("BMC", "OOM: %u B build buffers, reading through the stack", static_cast<uint32_t>(BUILD_IO_BUFFER_SIZE));
  }
  uint8_t spineFallback[128];
  uint8_t tocFallback[128];
  const bb_io_file spineView = file_io::fileOf(spineFile);
  const bb_io_file tocView = file_io::fileOf(tocFile);
  const bb_book_records spine{&spineView, spineBuf ? spineBuf.get() : spineFallback,
                              spineBuf ? BUILD_IO_BUFFER_SIZE : sizeof(spineFallback), spineCount};
  const bb_book_records toc{&tocView, tocBuf ? tocBuf.get() : tocFallback,
                            tocBuf ? BUILD_IO_BUFFER_SIZE : sizeof(tocFallback), tocCount};

  const bool isZip = !FsHelpers::hasTxtExtension(epubPath) && !FsHelpers::hasMarkdownExtension(epubPath);
  ZipFile zip(epubPath);
  std::optional<std::deque<uint32_t>> spineSizes;
  size_t rawSize = 0;
  if (isZip) {
    // Pre-open zip file to speed up size calculations
    if (!zip.open()) {
      LOG_ERR("BMC", "Could not open EPUB zip for size calculations");
      closeAll();
      return false;
    }
    // NOTE: We intentionally skip calling loadAllFileStatSlims() here.
    // For large EPUBs (2000+ chapters), pre-loading all ZIP central directory entries
    // into memory causes OOM crashes on ESP32-C3's limited ~380KB RAM.
    // Instead, for large books we use a batch lookup that scans the ZIP central
    // directory once per chunk and matches against spine targets using hash comparison.
    // This is O(n*log(m)) instead of O(n*m) while avoiding memory exhaustion.
    // See: https://github.com/crosspoint-reader/crosspoint-reader/issues/134

    if (spineCount >= LARGE_SPINE_THRESHOLD) {
      LOG_DBG("BMC", "Using batch size lookup for %d spine items", spineCount);
      spineSizes.emplace(spineCount, 0);
      SizeTargets batch{zip, spineCount, *spineSizes, {}, {}};
      const bb_book_items targets{&batch,
                                  [](void* c, const size_t len) {
                                    auto& href = static_cast<SizeTargets*>(c)->href;
                                    href.resize(len);
                                    return href.data();
                                  },
                                  [](void* c, const uint16_t index) -> uint32_t {
                                    auto& b = *static_cast<SizeTargets*>(c);
                                    const std::string path = FsHelpers::normalisePath(b.href);
                                    ZipFile::SizeTarget t;
                                    t.hash = ZipFile::fnvHash64(path.c_str(), path.size());
                                    t.len = static_cast<uint16_t>(path.size());
                                    t.index = index;
                                    b.targets.push_back(t);
                                    // Targets take 16 B per spine item, so look sizes up 2,048 items at a time.
                                    if (b.targets.size() < 2048 && index + 1 < b.spineCount) return 0;
                                    std::sort(b.targets.begin(), b.targets.end(),
                                              [](const ZipFile::SizeTarget& x, const ZipFile::SizeTarget& y) {
                                                return x.hash < y.hash || (x.hash == y.hash && x.len < y.len);
                                              });
                                    b.matched += b.zip.fillUncompressedSizes(b.targets, b.sizes);
                                    b.targets.clear();
                                    return 0;
                                  }};
      if (!bb_book_spine_hrefs(&spine, &targets)) {
        LOG_ERR("BMC", "Could not read the spine for size lookups");
      }
      LOG_DBG("BMC", "Batch lookup matched %d/%d spine items", batch.matched, spineCount);
    }
  } else {
    HalFile rawFile;
    if (Storage.openFileForRead("BMC", epubPath, rawFile)) {
      rawSize = rawFile.size();
    }
  }

  // Each spine item's TOC entry, as working room for the build.
  auto tocMap = makeUniqueNoThrow<int16_t[]>(std::max<size_t>(spineCount, 1));
  int result = BB_BOOK_NO_ROOM;
  bool written = false;
  if (!tocMap) {
    LOG_ERR("BMC", "OOM: TOC map, %u B", static_cast<uint32_t>(spineCount * sizeof(int16_t)));
  } else {
    serialization::BufferedFileWriter bookOut(bookFile, BUILD_IO_BUFFER_SIZE);
    const bb_io_sink out = file_io::sinkOf(bookOut);
    const char* texts[5] = {metadata.title.c_str(), metadata.author.c_str(), metadata.language.c_str(),
                            metadata.coverItemHref.c_str(), metadata.textReferenceHref.c_str()};
    const size_t lens[5] = {metadata.title.size(), metadata.author.size(), metadata.language.size(),
                            metadata.coverItemHref.size(), metadata.textReferenceHref.size()};
    Items items{zip, isZip, rawSize, spineSizes, {}};
    const bb_book_items itemView = itemsOf(items);
    result = bb_book_build(&spine, &toc, &out, texts, lens, tocMap.get(), &itemView);
    written = bookOut.flush();
  }
  spineSizes.reset();
  if (isZip) zip.close();
  closeAll();

  if (result != BB_BOOK_BUILT || !written) {
    // A failed build or a short write (card full/removed) leaves a partial book.bin that could
    // still pass the version check on load; remove it so the next open rebuilds.
    LOG_ERR("BMC", "Failed building book.bin (%d), removing it", result);
    Storage.remove((cachePath + bookBinFile).c_str());
    return false;
  }

  LOG_DBG("BMC", "Successfully built book.bin");
  return true;
}

bool BookMetadataCache::cleanupTmpFiles() const {
  const auto spineBinFile = cachePath + tmpSpineBinFile;
  if (Storage.exists(spineBinFile.c_str())) {
    Storage.remove(spineBinFile.c_str());
  }
  const auto tocBinFile = cachePath + tmpTocBinFile;
  if (Storage.exists(tocBinFile.c_str())) {
    Storage.remove(tocBinFile.c_str());
  }
  return true;
}

bb_io_sink BookMetadataCache::passSink(HalFile& file, uint32_t& at) {
  if (passOut) {
    at = static_cast<uint32_t>(passOut->position());
    return file_io::sinkOf(*passOut);
  }
  at = static_cast<uint32_t>(file.position());
  return file_io::sinkOf(file);
}

// Note: for the LUT to be accurate, this **MUST** be called for all spine items before `addTocEntry` is ever called
// this is because in this function we're marking positions of the items
void BookMetadataCache::createSpineEntry(const std::string& href) {
  if (!buildMode || !spineFile) {
    LOG_DBG("BMC", "createSpineEntry called but not in build mode");
    return;
  }

  uint32_t at = 0;
  const bb_io_sink sink = passSink(spineFile, at);
  if (!bb_book_write_spine_record(&sink, &at, href.data(), href.size(), 0, -1)) {
    LOG_ERR("BMC", "Failed writing spine entry");
  }
  spineCount++;
}

void BookMetadataCache::createTocEntry(const std::string& title, const std::string& href, const std::string& anchor,
                                       const uint8_t level) {
  if (!buildMode || !tocFile || !spineFile) {
    LOG_DBG("BMC", "createTocEntry called but not in build mode");
    return;
  }

  int16_t spineIndex = -1;
  if (spineHrefIndex) {
    spineIndex = bb_book_find_in_index(spineHrefIndex.get(), spineCount, href.data(), href.size());
  } else {
    uint8_t buf[128];
    const bb_io_file file = file_io::fileOf(spineFile);
    const bb_book_records spine{&file, buf, sizeof(buf), spineCount};
    spineIndex = bb_book_find_spine(&spine, href.data(), href.size());
  }
  if (spineIndex == -1) {
    LOG_DBG("BMC", "createTocEntry: Could not find spine item for TOC href %s", href.c_str());
  }

  // Compose the title to NFC at index time so the cache stores precomposed glyphs;
  // device fonts have no combining-mark positioning, so NFD titles render broken.
  const std::string composed = utf8ComposeNfc(title);
  uint32_t at = 0;
  const bb_io_sink sink = passSink(tocFile, at);
  if (!bb_book_write_toc_record(&sink, &at, composed.data(), composed.size(), href.data(), href.size(), anchor.data(),
                                anchor.size(), level, spineIndex)) {
    LOG_ERR("BMC", "Failed writing toc entry");
  }
  tocCount++;
}

/* ============= READING / LOADING FUNCTIONS ================ */

bool BookMetadataCache::load() {
  if (!Storage.openFileForRead("BMC", cachePath + bookBinFile, bookFile)) {
    return false;
  }

  Texts texts;
  texts.fields[BB_BOOK_TITLE] = &coreMetadata.title;
  texts.fields[BB_BOOK_AUTHOR] = &coreMetadata.author;
  texts.fields[BB_BOOK_LANGUAGE] = &coreMetadata.language;
  texts.fields[BB_BOOK_COVER] = &coreMetadata.coverItemHref;
  texts.fields[BB_BOOK_TEXT_REFERENCE] = &coreMetadata.textReferenceHref;
  const bb_book_target target = targetOf(texts);
  const bb_io_file view = file_io::fileOf(bookFile);
  // The load reads the header, then every spine entry's size in order.
  uint8_t buf[256];
  const bb_book_records book{&view, buf, sizeof(buf), 0};
  bb_book_header header{};
  uint8_t version = 0;
  const int result = bb_book_load(&book, &target, &header, &version);
  if (result == BB_BOOK_OTHER_VERSION) {
    LOG_DBG("BMC", "Cache version mismatch: got %d", version);
    // Explicit close() required: member variable persists beyond function scope
    bookFile.close();
    return false;
  }
  if (result != BB_BOOK_LOADED) {
    // A damaged file is not loaded: the caller rebuilds the cache from the book.
    LOG_ERR("BMC", "Cache file damaged: %s", (cachePath + bookBinFile).c_str());
    bookFile.close();
    return false;
  }
  lutOffset = header.lut;
  spineCount = header.spine_count;
  tocCount = header.toc_count;
  windowStarts.assign((spineCount + BB_BOOK_WINDOW - 1) / BB_BOOK_WINDOW, 0);
  bookSize = bb_book_window_starts(&book, &header, windowStarts.data(), windowStarts.size());
  windowFirst = -1;

  loaded = true;
  LOG_DBG("BMC", "Loaded cache data: %d spine, %d TOC entries", spineCount, tocCount);
  return true;
}

bb_book_sizes BookMetadataCache::sizesView() const {
  return {spineCount, bookSize, windowStarts.data(), windowStarts.size(), &windowFirst, window};
}

uint32_t BookMetadataCache::getCumulativeSize(const int index) const {
  if (!loaded) return 0;
  // The book's size, asked for on every progress update, needs no window.
  if (index == spineCount - 1) return bookSize;
  std::lock_guard<std::mutex> lock(sizeWindowLock);
  const bb_book_sizes sizes = sizesView();
  return bb_book_cumulative_size(&sizes, index, const_cast<BookMetadataCache*>(this),
                                 [](void* c, const int32_t first, uint32_t* into) {
                                   return static_cast<const BookMetadataCache*>(c)->fillWindow(first, into);
                                 });
}

bool BookMetadataCache::fillWindow(const int32_t first, uint32_t* into) const {
  // Own handle: getSpineEntry() moves bookFile.
  HalFile file;
  if (!Storage.openFileForRead("BMC", cachePath + bookBinFile, file)) return false;
  const bb_io_file view = file_io::fileOf(file);
  uint8_t buf[256];
  const bb_book_records book{&view, buf, sizeof(buf), spineCount};
  return bb_book_fill_window(&book, lutOffset, static_cast<uint16_t>(first), into);
}

int BookMetadataCache::getSpineIndexForSize(const uint32_t size) const {
  if (!loaded) return -1;
  std::lock_guard<std::mutex> lock(sizeWindowLock);
  const bb_book_sizes sizes = sizesView();
  return bb_book_index_for_size(&sizes, size, const_cast<BookMetadataCache*>(this),
                                [](void* c, const int32_t first, uint32_t* into) {
                                  return static_cast<const BookMetadataCache*>(c)->fillWindow(first, into);
                                });
}

BookMetadataCache::SpineEntry BookMetadataCache::getSpineEntry(const int index) {
  if (!loaded) {
    LOG_ERR("BMC", "getSpineEntry called but cache not loaded");
    return {};
  }

  if (index < 0 || index >= static_cast<int>(spineCount)) {
    LOG_ERR("BMC", "getSpineEntry index %d out of range", index);
    return {};
  }

  SpineEntry entry;
  Texts texts;
  texts.fields[BB_BOOK_HREF] = &entry.href;
  const bb_book_target target = targetOf(texts);
  const bb_io_file view = file_io::fileOf(bookFile);
  uint8_t buf[ENTRY_BUFFER_SIZE];
  const bb_book_records book{&view, buf, sizeof(buf), spineCount};
  bb_book_spine_entry(&book, lutOffset, static_cast<uint16_t>(index), &target, &entry.cumulativeSize, &entry.tocIndex);
  return entry;
}

BookMetadataCache::TocEntry BookMetadataCache::getTocEntry(const int index) {
  if (!loaded) {
    LOG_ERR("BMC", "getTocEntry called but cache not loaded");
    return {};
  }

  if (index < 0 || index >= static_cast<int>(tocCount)) {
    LOG_ERR("BMC", "getTocEntry index %d out of range", index);
    return {};
  }

  TocEntry entry;
  Texts texts;
  texts.fields[BB_BOOK_TOC_TITLE] = &entry.title;
  texts.fields[BB_BOOK_HREF] = &entry.href;
  texts.fields[BB_BOOK_ANCHOR] = &entry.anchor;
  const bb_book_target target = targetOf(texts);
  const bb_io_file view = file_io::fileOf(bookFile);
  uint8_t buf[ENTRY_BUFFER_SIZE];
  const bb_book_records book{&view, buf, sizeof(buf), tocCount};
  bb_book_toc_entry(&book, lutOffset, spineCount, static_cast<uint16_t>(index), &target, &entry.level,
                    &entry.spineIndex);
  return entry;
}
