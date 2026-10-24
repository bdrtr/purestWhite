// lib/Epub/Epub/converters/ImageDimsProbe.cpp's parser at de4bdf33 (its state from the header),
// verbatim but for the namespace, the class name and the class being a plain struct: the oracle for
// bembeyaz-image's probe.
#pragma once

#include <cstddef>
#include <cstdint>

namespace oldProbe {

struct ImageDimsProbe {
  bool feed(uint8_t b);
  size_t write(const uint8_t* data, const size_t len) {
    for (size_t i = 0; i < len; i++) {
      if (!feed(data[i])) return i;  // short write: polite early stop
    }
    return len;
  }

  enum class State : uint8_t {
    Sniff,       // first byte decides the format
    PngHeader,   // PNG signature + IHDR at fixed offsets
    JpegSoi,     // second SOI byte (0xD8)
    JpegFf,      // expect a 0xFF marker prefix
    JpegMarker,  // marker type byte (0xFF padding allowed)
    JpegLenHi,   // segment length, high byte
    JpegLenLo,   // segment length, low byte
    JpegSkip,    // skipping a non-SOF segment body
    JpegSof,     // collecting the 5 SOF bytes: precision, height(2), width(2)
    Done,
    Failed,
  };
  State state = State::Sniff;
  uint32_t pos = 0;       // absolute stream offset (PNG fixed-offset parsing)
  uint32_t skipLeft = 0;  // remaining segment bytes to skip
  uint16_t segLen = 0;
  bool sofPending = false;  // current segment is a SOF frame header
  uint8_t sofBuf[5] = {0};
  uint8_t sofFill = 0;
  // 32-bit: PNG IHDR width/height are 4-byte fields. Accumulating them in a
  // uint16_t silently truncates an oversized image to a plausible small value
  // that passes the INT16_MAX sanity check in getDimensions().
  uint32_t width = 0;
  uint32_t height = 0;
};

namespace {
// SOFn markers carry the frame dimensions. C4 (DHT), C8 (JPG extension) and
// CC (DAC) share the 0xCn range but are not frame headers.
bool isJpegSof(const uint8_t marker) {
  return marker >= 0xC0 && marker <= 0xCF && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
}
constexpr uint8_t PNG_SIG[8] = {0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A};
}  // namespace

inline bool ImageDimsProbe::feed(const uint8_t b) {
  switch (state) {
    case State::Sniff:
      if (b == 0xFF) {
        state = State::JpegSoi;
      } else if (b == PNG_SIG[0]) {
        state = State::PngHeader;
      } else {
        state = State::Failed;
        return false;
      }
      pos = 1;
      return true;

    case State::PngHeader:
      // Bytes 1..7: signature; 8..11: IHDR length; 12..15: "IHDR"; 16..23: dims.
      if (pos < 8) {
        if (b != PNG_SIG[pos]) {
          state = State::Failed;
          return false;
        }
      } else if (pos >= 12 && pos < 16) {
        if (b != "IHDR"[pos - 12]) {
          state = State::Failed;
          return false;
        }
      } else if (pos >= 16 && pos < 20) {
        width = (width << 8) | b;
      } else if (pos >= 20 && pos < 24) {
        height = (height << 8) | b;
        if (pos == 23) {
          state = State::Done;
          pos++;
          return false;
        }
      }
      pos++;
      return true;

    case State::JpegSoi:
      if (b != 0xD8) {
        state = State::Failed;
        return false;
      }
      state = State::JpegFf;
      return true;

    case State::JpegFf:
      if (b != 0xFF) {
        state = State::Failed;
        return false;
      }
      state = State::JpegMarker;
      return true;

    case State::JpegMarker:
      if (b == 0xFF) return true;  // fill bytes before a marker are legal
      if (b == 0x01 || (b >= 0xD0 && b <= 0xD8)) {
        // TEM / RSTn / SOI: standalone, no length field.
        state = State::JpegFf;
        return true;
      }
      if (b == 0xD9 || b == 0xDA) {
        // EOI or SOS before any SOF: no dimensions to be found.
        state = State::Failed;
        return false;
      }
      sofPending = isJpegSof(b);
      state = State::JpegLenHi;
      return true;

    case State::JpegLenHi:
      segLen = static_cast<uint16_t>(b << 8);
      state = State::JpegLenLo;
      return true;

    case State::JpegLenLo:
      segLen = static_cast<uint16_t>(segLen | b);
      if (segLen < 2 || (sofPending && segLen < 7)) {
        state = State::Failed;
        return false;
      }
      if (sofPending) {
        sofFill = 0;
        state = State::JpegSof;
      } else if (segLen == 2) {
        state = State::JpegFf;
      } else {
        skipLeft = static_cast<uint32_t>(segLen) - 2;
        state = State::JpegSkip;
      }
      return true;

    case State::JpegSkip:
      if (--skipLeft == 0) state = State::JpegFf;
      return true;

    case State::JpegSof:
      sofBuf[sofFill++] = b;
      if (sofFill == 5) {
        height = static_cast<uint32_t>((sofBuf[1] << 8) | sofBuf[2]);
        width = static_cast<uint32_t>((sofBuf[3] << 8) | sofBuf[4]);
        state = State::Done;
        return false;
      }
      return true;

    case State::Done:
    case State::Failed:
      return false;
  }
  return false;
}

}  // namespace oldProbe
