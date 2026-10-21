/**
 * XtcParser.h
 *
 * XTC file parsing and page data extraction
 * XTC ebook support for CrossPoint Reader
 */

#pragma once

#include <HalStorage.h>
#include <bembeyaz/xtc.h>

#include <functional>
#include <memory>
#include <string>
#include <vector>

#include "XtcTypes.h"

namespace xtc {

/**
 * XTC File Parser
 *
 * Reads XTC files from SD card and extracts page data.
 * Designed for ESP32-C3's limited RAM (~380KB) using streaming.
 *
 * The source file is kept closed between reads to free heap for rendering.
 * It is reopened on-demand for page table lookups and bitmap data reads.
 */
class XtcParser {
 public:
  XtcParser();
  ~XtcParser();

  // File open/close
  XtcError open(const char* filepath);
  void close();
  bool isOpen() const { return m_isOpen; }

  // Header information access
  uint16_t getPageCount() const { return m_book.page_count; }
  uint16_t getWidth() const { return m_book.width; }
  uint16_t getHeight() const { return m_book.height; }
  uint8_t getBitDepth() const { return m_book.bit_depth; }  // 1 = XTC/XTG, 2 = XTCH/XTH

  // Page information
  bool getPageInfo(uint32_t pageIndex, PageInfo& info);

  /**
   * Load page bitmap (raw 1-bit data, skipping XTG header)
   *
   * @param pageIndex Page index (0-based)
   * @param buffer Output buffer (caller allocated)
   * @param bufferSize Buffer size
   * @return Number of bytes read on success, 0 on failure
   */
  size_t loadPage(uint32_t pageIndex, uint8_t* buffer, size_t bufferSize);

  /**
   * Streaming page load
   * Memory-efficient method that reads page data in chunks.
   *
   * @param pageIndex Page index
   * @param callback Callback function to receive data chunks
   * @param chunkSize Chunk size (default: 1024 bytes)
   * @return Error code
   */
  XtcError loadPageStreaming(uint32_t pageIndex,
                             std::function<void(const uint8_t* data, size_t size, size_t offset)> callback,
                             size_t chunkSize = 1024);

  // Get title/author from metadata
  std::string getTitle() const { return std::string(m_book.title, m_book.title_len); }
  std::string getAuthor() const { return std::string(m_book.author, m_book.author_len); }

  bool hasChapters() const { return m_hasChapters; }
  const std::vector<ChapterInfo>& getChapters();

  // Validation
  static bool isValidXtcFile(const char* filepath);

  // Error information
  XtcError getLastError() const { return m_lastError; }

 private:
  HalFile m_file;
  std::string m_filepath;
  bool m_isOpen;
  bb_xtc m_book{};
  std::vector<ChapterInfo> m_chapters;
  bool m_hasChapters;
  bool m_chaptersLoaded;
  XtcError m_lastError;

  XtcError readChapters();
  // The bb_xtc_file over m_file, reopening it as needed.
  bb_xtc_file file();
  static int64_t readAt(void* context, uint64_t offset, uint8_t* buf, size_t len);
  static uint64_t fileSize(void* context);

  // File handle management — reopen on demand, close after use
  bool ensureFileOpen();
  void closeFile();
};

}  // namespace xtc
