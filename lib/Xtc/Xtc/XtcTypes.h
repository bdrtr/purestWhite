/**
 * XtcTypes.h
 *
 * XTC file format type definitions
 * XTC ebook support for CrossPoint Reader
 *
 * XTC is the native binary ebook format for XTeink X4 e-reader.
 * It stores pre-rendered bitmap images per page.
 *
 * Format based on EPUB2XTC converter by Rafal-P-Mazur
 */

#pragma once

#include <cstdint>
#include <string>

namespace xtc {

// The layout and its checks live in rust/crates/bembeyaz-xtc.

// XTeink X4 display resolution: a book's page size until its first page is read.
constexpr uint16_t DISPLAY_WIDTH = 480;
constexpr uint16_t DISPLAY_HEIGHT = 800;

// Page information (internal use, optimized for memory)
struct PageInfo {
  uint64_t offset;   // File offset to page data
  uint32_t size;     // Data size (bytes)
  uint16_t width;    // Page width
  uint16_t height;   // Page height
  uint8_t bitDepth;  // 1 = XTG (1-bit), 2 = XTH (2-bit grayscale)
  uint8_t padding;   // Alignment padding
};

struct ChapterInfo {
  std::string name;
  uint16_t startPage;
  uint16_t endPage;
};

// Error codes
enum class XtcError {
  OK = 0,
  FILE_NOT_FOUND,
  INVALID_MAGIC,
  INVALID_VERSION,
  CORRUPTED_HEADER,
  PAGE_OUT_OF_RANGE,
  READ_ERROR,
  WRITE_ERROR,
  MEMORY_ERROR,
  DECOMPRESSION_ERROR,
};

// Convert error code to string
inline const char* errorToString(XtcError err) {
  switch (err) {
    case XtcError::OK:
      return "OK";
    case XtcError::FILE_NOT_FOUND:
      return "File not found";
    case XtcError::INVALID_MAGIC:
      return "Invalid magic number";
    case XtcError::INVALID_VERSION:
      return "Unsupported version";
    case XtcError::CORRUPTED_HEADER:
      return "Corrupted header";
    case XtcError::PAGE_OUT_OF_RANGE:
      return "Page out of range";
    case XtcError::READ_ERROR:
      return "Read error";
    case XtcError::WRITE_ERROR:
      return "Write error";
    case XtcError::MEMORY_ERROR:
      return "Memory allocation error";
    case XtcError::DECOMPRESSION_ERROR:
      return "Decompression error";
    default:
      return "Unknown error";
  }
}

}  // namespace xtc
