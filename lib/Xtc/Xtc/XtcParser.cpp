/**
 * XtcParser.cpp
 *
 * XTC file parsing implementation over bembeyaz-xtc
 * XTC ebook support for CrossPoint Reader
 */

#include "XtcParser.h"

#include <HalStorage.h>
#include <Logging.h>

#include <cstddef>

namespace xtc {

// Checked against Book in rust/crates/bembeyaz-ffi/src/xtc.rs too.
static_assert(sizeof(bb_xtc) == 216 && offsetof(bb_xtc, page_count) == 16 && offsetof(bb_xtc, title) == 26 &&
                  offsetof(bb_xtc, author) == 153,
              "bb_xtc layout");
static_assert(static_cast<int>(XtcError::INVALID_MAGIC) == 2 && static_cast<int>(XtcError::INVALID_VERSION) == 3 &&
                  static_cast<int>(XtcError::CORRUPTED_HEADER) == 4 &&
                  static_cast<int>(XtcError::PAGE_OUT_OF_RANGE) == 5 && static_cast<int>(XtcError::READ_ERROR) == 6 &&
                  static_cast<int>(XtcError::MEMORY_ERROR) == 8,
              "bb_xtc results");

XtcParser::XtcParser() : m_isOpen(false), m_hasChapters(false), m_chaptersLoaded(false), m_lastError(XtcError::OK) {
  m_book.width = DISPLAY_WIDTH;
  m_book.height = DISPLAY_HEIGHT;
  m_book.bit_depth = 1;
}

XtcParser::~XtcParser() { close(); }

bb_xtc_file XtcParser::file() { return {readAt, fileSize, this}; }

int64_t XtcParser::readAt(void* context, const uint64_t offset, uint8_t* buf, const size_t len) {
  auto* self = static_cast<XtcParser*>(context);
  if (!self->ensureFileOpen() || !self->m_file.seek64(offset)) return -1;
  const int count = self->m_file.read(buf, len);
  return count > 0 ? count : 0;
}

uint64_t XtcParser::fileSize(void* context) {
  auto* self = static_cast<XtcParser*>(context);
  return self->ensureFileOpen() ? self->m_file.fileSize64() : 0;
}

XtcError XtcParser::open(const char* filepath) {
  // Close if already open
  if (m_isOpen) {
    close();
  }

  m_filepath = filepath;

  // Open file
  if (!Storage.openFileForRead("XTC", filepath, m_file)) {
    m_lastError = XtcError::FILE_NOT_FOUND;
    return m_lastError;
  }

  const bb_xtc_file source = file();
  m_lastError = static_cast<XtcError>(bb_xtc_open(&m_book, &source));
  if (m_lastError != XtcError::OK) {
    LOG_DBG("XTC", "Failed to open: %s", errorToString(m_lastError));
    // Explicit close() required: member variable persists beyond function scope
    m_file.close();
    return m_lastError;
  }

  // Chapters load on first use: their strings would take heap from the page bitmap buffer.
  m_hasChapters = m_book.has_chapters;
  m_chaptersLoaded = false;

  // Close the source file to free its internal SdFat buffers.
  // It will be reopened on-demand for page table lookups and bitmap reads.
  m_file.close();

  m_isOpen = true;
  LOG_DBG("XTC", "Opened file: %s (%u pages, %dx%d, %u-bit)", filepath, m_book.page_count, m_book.width, m_book.height,
          m_book.bit_depth);
  return XtcError::OK;
}

void XtcParser::close() {
  closeFile();
  m_isOpen = false;
  m_chaptersLoaded = false;
  m_chapters.clear();
  m_hasChapters = false;
  m_book.page_table_offset = 0;
  m_book.data_offset = 0;
  m_book.page_count = 0;
  m_book.has_chapters = false;
  m_book.title_len = 0;
  m_book.author_len = 0;
}

bool XtcParser::ensureFileOpen() {
  if (m_file.isOpen()) {
    return true;
  }
  return Storage.openFileForRead("XTC", m_filepath.c_str(), m_file);
}

void XtcParser::closeFile() {
  if (m_file.isOpen()) {
    m_file.close();
  }
}

XtcError XtcParser::readChapters() {
  m_chapters.clear();
  const auto add = [](void* context, const char* name, const size_t len, const uint16_t first, const uint16_t last) {
    static_cast<std::vector<ChapterInfo>*>(context)->push_back(ChapterInfo{std::string(name, len), first, last});
  };
  const auto reserve = [](void* context, const size_t records) {
    static_cast<std::vector<ChapterInfo>*>(context)->reserve(records);
  };
  const bb_xtc_file source = file();
  bool listed = false;
  const auto error = static_cast<XtcError>(bb_xtc_chapters(&m_book, &source, reserve, add, &m_chapters, &listed));
  if (error != XtcError::OK) return error;
  if (listed) {
    m_hasChapters = !m_chapters.empty();
    LOG_DBG("XTC", "Chapters: %u", static_cast<unsigned int>(m_chapters.size()));
  }
  return XtcError::OK;
}

const std::vector<ChapterInfo>& XtcParser::getChapters() {
  // Lazy load chapters on first access
  if (!m_chaptersLoaded && m_hasChapters) {
    const XtcError err = readChapters();
    if (err != XtcError::OK) {
      LOG_ERR("XTC", "Failed to lazy-load chapters: %s", errorToString(err));
      m_hasChapters = false;
      m_chapters.clear();
    }
    m_chaptersLoaded = true;
    // Close file after chapter read to free buffers for rendering
    closeFile();
  }
  return m_chapters;
}

bool XtcParser::getPageInfo(uint32_t pageIndex, PageInfo& info) {
  const bb_xtc_file source = file();
  bb_xtc_page page{};
  if (!bb_xtc_page_info(&m_book, &source, pageIndex, &page)) return false;
  info.offset = page.offset;
  info.size = page.size;
  info.width = page.width;
  info.height = page.height;
  info.bitDepth = m_book.bit_depth;
  return true;
}

size_t XtcParser::loadPage(uint32_t pageIndex, uint8_t* buffer, size_t bufferSize) {
  if (!m_isOpen) {
    m_lastError = XtcError::FILE_NOT_FOUND;
    return 0;
  }
  const bb_xtc_file source = file();
  size_t loaded = 0;
  m_lastError = static_cast<XtcError>(bb_xtc_load_page(&m_book, &source, pageIndex, buffer, bufferSize, &loaded));
  if (m_lastError != XtcError::OK) {
    LOG_DBG("XTC", "Failed to load page %u: %s", pageIndex, errorToString(m_lastError));
  }
  return loaded;
}

XtcError XtcParser::loadPageStreaming(uint32_t pageIndex,
                                      std::function<void(const uint8_t* data, size_t size, size_t offset)> callback,
                                      size_t chunkSize) {
  if (!m_isOpen) {
    return XtcError::FILE_NOT_FOUND;
  }
  using Callback = std::function<void(const uint8_t*, size_t, size_t)>;
  const auto forward = [](void* context, const uint8_t* data, const size_t size, const size_t offset) {
    (*static_cast<Callback*>(context))(data, size, offset);
  };
  std::vector<uint8_t> chunk(chunkSize);
  const bb_xtc_file source = file();
  return static_cast<XtcError>(
      bb_xtc_stream_page(&m_book, &source, pageIndex, chunk.data(), chunk.size(), forward, &callback));
}

bool XtcParser::isValidXtcFile(const char* filepath) {
  HalFile file;
  if (!Storage.openFileForRead("XTC", filepath, file)) {
    return false;
  }
  uint8_t magic[4];
  const int bytesRead = file.read(magic, sizeof(magic));
  file.close();
  return bytesRead == static_cast<int>(sizeof(magic)) && bb_xtc_is_book_magic(magic, sizeof(magic));
}

}  // namespace xtc
