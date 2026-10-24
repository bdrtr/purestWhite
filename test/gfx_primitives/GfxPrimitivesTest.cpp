// bembeyaz-gfx against the drawing code GfxRenderer had before it (lib/GfxRenderer/GfxRenderer.cpp
// at 87481ce5, fillPolygon at 90f34acc and the pictures at 6603fc10, copied below as Old): random panels, orientations,
// strips, clips and shapes must give the same pixels, and the same count of pixels off the panel.
#include <BitmapHelpers.h>
#include <Memory.h>
#include <bembeyaz/gfx.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <utility>
#include <vector>

// A picture in memory standing in for lib/GfxRenderer's Bitmap: packed 2-bit rows, and a row
// whose read fails.
enum class BmpReaderError { Ok, ShortReadRow };
struct Bitmap {
  int width = 0, height = 0;
  bool topDown = false, oneBit = false;
  std::vector<std::vector<uint8_t>> rows;
  int failAt = -1;
  mutable int next = 0;
  int getWidth() const { return width; }
  int getHeight() const { return height; }
  bool isTopDown() const { return topDown; }
  bool is1Bit() const { return oneBit; }
  int getRowBytes() const { return 4; }
  BmpReaderError readNextRow(uint8_t* data, uint8_t*) const {
    if (next == failAt || next >= static_cast<int>(rows.size())) return BmpReaderError::ShortReadRow;
    std::copy(rows[next].begin(), rows[next].end(), data);
    next++;
    return BmpReaderError::Ok;
  }
};

enum Color : uint8_t { Clear = 0x00, White = 0x01, LightGray = 0x05, DarkGray = 0x0A, Black = 0x10 };

struct Old {
  enum Orientation { Portrait, LandscapeClockwise, PortraitInverted, LandscapeCounterClockwise };
  Orientation orientation = Portrait;
  uint16_t panelWidth = 0, panelHeight = 0, panelWidthBytes = 0;
  uint8_t* frameBuffer = nullptr;
  mutable uint8_t* _stripBuf = nullptr;
  mutable int _stripY0 = 0, _stripRows = 0;
  mutable bool _stripActive = false;
  mutable int clipLeft_ = 0, clipTop_ = 0, clipRight_ = 32767, clipBottom_ = 32767;
  mutable unsigned offPanel = 0;
  uint8_t* getWriteTarget() const { return _stripActive ? _stripBuf : frameBuffer; }
  int getWriteOriginY() const { return _stripActive ? _stripY0 : 0; }
  int getWriteRows() const { return _stripActive ? _stripRows : panelHeight; }
  void drawPixel(int x, int y, bool state = true) const;
  void drawLine(int x1, int y1, int x2, int y2, bool state) const;
  void drawLine(int x1, int y1, int x2, int y2, int lineWidth, bool state) const;
  void drawRect(int x, int y, int width, int height, bool state) const;
  void drawRect(int x, int y, int width, int height, int lineWidth, bool state) const;
  template <Color C>
  void fillRectImpl(int x, int y, int width, int height) const;
  void drawArc(int maxRadius, int cx, int cy, int xDir, int yDir, int lineWidth, bool state) const;
  void drawRoundedRect(int x, int y, int width, int height, int lineWidth, int cornerRadius, bool roundTopLeft,
                       bool roundTopRight, bool roundBottomLeft, bool roundBottomRight, bool state) const;
  void maskRoundedRectOutsideCorners(int x, int y, int width, int height, int radius, Color color) const;
  void fillRect(int x, int y, int width, int height, bool state) const;
  void fillRectDither(int x, int y, int width, int height, Color color) const;
  void fillRoundedRect(int x, int y, int width, int height, int cornerRadius, bool roundTopLeft, bool roundTopRight,
                       bool roundBottomLeft, bool roundBottomRight, Color color) const;
  template <Color color>
  void drawPixelDither(int x, int y) const;
  template <Color color>
  void fillArc(int maxRadius, int cx, int cy, int xDir, int yDir) const;
  void fillPolygon(const int* xPoints, const int* yPoints, int numPoints, bool state = true) const;
  int getScreenWidth() const;
  int getScreenHeight() const;
  void drawIcon(const uint8_t bitmap[], int x, int y, int size) const;
  bool drawBitmap(const Bitmap& bitmap, int x, int y, int maxWidth, int maxHeight, float cropX = 0, float cropY = 0,
                  bool whiteAsTransparent = false) const;
  bool drawBitmap1Bit(const Bitmap& bitmap, int x, int y, int maxWidth, int maxHeight) const;
  void preserveImagePolarity(int x, int y, int width, int height) const;
  void renderCharScaled(const uint8_t* bitmap, const struct OldGlyphMetrics* glyph, const struct OldFontData* fontData,
                        int cursorX, int cursorY, bool pixelState, int scaleNum, int scaleDen) const;
  void drawScaledCodepoint(const uint8_t* bitmap, const struct OldGlyphMetrics* glyph,
                           const struct OldFontData* fontData, int x, int baselineY, int scale256) const;
  enum RenderMode { BW, GRAYSCALE_LSB, GRAYSCALE_MSB };
  RenderMode renderMode = BW;
  bool absoluteGrayPlanes = false;
  struct {
    bool inverted = false;
    bool isInverted() const { return inverted; }
  } display;
  struct {
    bool isScanning() const { return false; }
  }* fontCacheManager_ = nullptr;
};
using Orientation = Old::Orientation;
constexpr auto Portrait = Old::Portrait;
constexpr auto LandscapeClockwise = Old::LandscapeClockwise;
constexpr auto PortraitInverted = Old::PortraitInverted;
constexpr auto LandscapeCounterClockwise = Old::LandscapeCounterClockwise;

// ---- Old: verbatim, except that off-panel pixels are counted rather than logged. ----

#define LOG_ERR(...) ((void)0)
#define LOG_DBG(...) ((void)0)

static inline void rotateCoordinates(const Old::Orientation orientation, const int x, const int y, int* phyX, int* phyY,
                                     const uint16_t panelWidth, const uint16_t panelHeight) {
  switch (orientation) {
    case Old::Portrait: {
      // Logical portrait (480x800) → panel (800x480)
      // Rotation: 90 degrees clockwise
      *phyX = y;
      *phyY = panelHeight - 1 - x;
      break;
    }
    case Old::LandscapeClockwise: {
      // Logical landscape (800x480) rotated 180 degrees (swap top/bottom and left/right)
      *phyX = panelWidth - 1 - x;
      *phyY = panelHeight - 1 - y;
      break;
    }
    case Old::PortraitInverted: {
      // Logical portrait (480x800) → panel (800x480)
      // Rotation: 90 degrees counter-clockwise
      *phyX = panelWidth - 1 - y;
      *phyY = x;
      break;
    }
    case Old::LandscapeCounterClockwise: {
      // Logical landscape (800x480) aligned with panel orientation
      *phyX = x;
      *phyY = y;
      break;
    }
  }
}

void Old::drawPixel(const int x, const int y, const bool state) const {
  if (x < clipLeft_ || y < clipTop_ || x >= clipRight_ || y >= clipBottom_) return;
  int phyX = 0;
  int phyY = 0;

  // Note: this call should be inlined for better performance
  rotateCoordinates(orientation, x, y, &phyX, &phyY, panelWidth, panelHeight);

  // Bounds checking against runtime panel dimensions
  if (phyX < 0 || phyX >= panelWidth || phyY < 0 || phyY >= panelHeight) {
    offPanel++;
    return;
  }

  // Tiled grayscale: redirect writes to the strip scratch and clip to the
  // current band. Single predictable branch on the hot per-pixel path.
  uint8_t* target = frameBuffer;
  uint32_t rowY = static_cast<uint32_t>(phyY);
  if (_stripActive) {
    if (phyY < _stripY0 || phyY >= _stripY0 + _stripRows) {
      return;  // pixel outside the band currently being rendered
    }
    target = _stripBuf;
    rowY = static_cast<uint32_t>(phyY - _stripY0);
  }

  // Calculate byte position and bit position
  const uint32_t byteIndex = rowY * panelWidthBytes + (phyX / 8);
  const uint8_t bitPosition = 7 - (phyX % 8);  // MSB first

  if (state) {
    target[byteIndex] &= ~(1 << bitPosition);  // Clear bit
  } else {
    target[byteIndex] |= 1 << bitPosition;  // Set bit
  }
}

void Old::drawLine(int x1, int y1, int x2, int y2, const bool state) const {
  if (x1 == x2) {
    if (y2 < y1) {
      std::swap(y1, y2);
    }
    for (int y = y1; y <= y2; y++) {
      drawPixel(x1, y, state);
    }
  } else if (y1 == y2) {
    if (x2 < x1) {
      std::swap(x1, x2);
    }
    for (int x = x1; x <= x2; x++) {
      drawPixel(x, y1, state);
    }
  } else {
    // Bresenham's line algorithm — integer arithmetic only
    int dx = x2 - x1;
    int dy = y2 - y1;
    int sx = (dx > 0) ? 1 : -1;
    int sy = (dy > 0) ? 1 : -1;
    dx = sx * dx;  // abs
    dy = sy * dy;  // abs

    int err = dx - dy;
    while (true) {
      drawPixel(x1, y1, state);
      if (x1 == x2 && y1 == y2) break;
      int e2 = 2 * err;
      if (e2 > -dy) {
        err -= dy;
        x1 += sx;
      }
      if (e2 < dx) {
        err += dx;
        y1 += sy;
      }
    }
  }
}

void Old::drawLine(int x1, int y1, int x2, int y2, const int lineWidth, const bool state) const {
  for (int i = 0; i < lineWidth; i++) {
    drawLine(x1, y1 + i, x2, y2 + i, state);
  }
}

void Old::drawRect(const int x, const int y, const int width, const int height, const bool state) const {
  drawLine(x, y, x + width - 1, y, state);
  drawLine(x + width - 1, y, x + width - 1, y + height - 1, state);
  drawLine(x + width - 1, y + height - 1, x, y + height - 1, state);
  drawLine(x, y, x, y + height - 1, state);
}

// Border is inside the rectangle
void Old::drawRect(const int x, const int y, const int width, const int height, const int lineWidth,
                   const bool state) const {
  // Keep the border inside [x, x+width) like the thin overload: the previous
  // right/bottom edges at x+width / y+height sat one pixel outside the rect,
  // so stroked boxes looked shifted against fills computed from the rect.
  for (int i = 0; i < lineWidth; i++) {
    drawLine(x + i, y + i, x + width - 1 - i, y + i, state);
    drawLine(x + width - 1 - i, y + i, x + width - 1 - i, y + height - 1 - i, state);
    drawLine(x + width - 1 - i, y + height - 1 - i, x + i, y + height - 1 - i, state);
    drawLine(x + i, y + height - 1 - i, x + i, y + i, state);
  }
}

template <Color C>
void Old::fillRectImpl(const int x, const int y, const int width, const int height) const {
  if constexpr (C == Color::Clear) return;
  if (width <= 0 || height <= 0) return;

  // Clip in logical space.
  const int screenW = getScreenWidth();
  const int screenH = getScreenHeight();
  const int lx0 = std::max({0, x, clipLeft_});
  const int ly0 = std::max({0, y, clipTop_});
  const int lx1 = std::min({screenW, x + width, clipRight_});
  const int ly1 = std::min({screenH, y + height, clipBottom_});
  if (lx0 >= lx1 || ly0 >= ly1) return;

  // Rotate the two opposing logical corners into physical-framebuffer space.
  // The bounding rect in physical space is the rect we need to fill — rotation
  // is rigid (no shear/stretch) so the bbox of the two corners IS the rect.
  int paX, paY, pbX, pbY;
  rotateCoordinates(orientation, lx0, ly0, &paX, &paY, panelWidth, panelHeight);
  rotateCoordinates(orientation, lx1 - 1, ly1 - 1, &pbX, &pbY, panelWidth, panelHeight);

  const int phyX0 = std::min(paX, pbX);
  const int phyX1 = std::max(paX, pbX);  // inclusive
  int phyY0 = std::min(paY, pbY);
  int phyY1 = std::max(paY, pbY);

  // Strip mode: clip Y range to the active band and redirect writes.
  uint8_t* target = getWriteTarget();
  const int originY = getWriteOriginY();
  const int writeRows = getWriteRows();
  phyY0 = std::max(phyY0, originY);
  phyY1 = std::min(phyY1, originY + writeRows - 1);
  if (phyY0 > phyY1) return;

  // Bit/byte layout: MSB-first within a byte, so phyX → bit (7 - (phyX & 7)).
  // Head and tail masks cover only the in-rect bits of the first/last byte.
  const int byteStart = phyX0 >> 3;
  const int byteEnd = phyX1 >> 3;  // inclusive
  const uint8_t headMask = static_cast<uint8_t>(0xFFu >> (phyX0 & 7));
  const uint8_t tailMask = static_cast<uint8_t>(0xFFu << (7 - (phyX1 & 7)));
  const int32_t panelStride = static_cast<int32_t>(panelWidthBytes);

  if constexpr (C == Color::Black || C == Color::White) {
    // Solid fill. Framebuffer: 0 = black, 1 = white.
    const uint8_t fillByte = (C == Color::Black) ? 0x00u : 0xFFu;
    for (int py = phyY0; py <= phyY1; ++py) {
      uint8_t* row = target + static_cast<int32_t>(py - originY) * panelStride;
      if (byteStart == byteEnd) {
        const uint8_t mask = headMask & tailMask;
        if constexpr (C == Color::Black) {
          row[byteStart] &= static_cast<uint8_t>(~mask);
        } else {
          row[byteStart] |= mask;
        }
      } else {
        if constexpr (C == Color::Black) {
          row[byteStart] &= static_cast<uint8_t>(~headMask);
          if (byteEnd > byteStart + 1) {
            memset(row + byteStart + 1, fillByte, byteEnd - byteStart - 1);
          }
          row[byteEnd] &= static_cast<uint8_t>(~tailMask);
        } else {
          row[byteStart] |= headMask;
          if (byteEnd > byteStart + 1) {
            memset(row + byteStart + 1, fillByte, byteEnd - byteStart - 1);
          }
          row[byteEnd] |= tailMask;
        }
      }
    }
  } else {
    // Dither (LightGray / DarkGray). Both patterns have period 2 in logical
    // (x, y), so per physical row we precompute one byte that represents the
    // pattern across an 8-pixel stretch — every full byte in the row uses
    // that same value.
    //
    // dlxPerPhyX / dlyPerPhyX: how logical (x, y) change as phyX increments
    // along a physical row. Derived from inverting rotateCoordinates.
    int dlxPerPhyX = 0, dlyPerPhyX = 0;
    switch (orientation) {
      case Portrait:
        dlxPerPhyX = 0;
        dlyPerPhyX = 1;
        break;
      case PortraitInverted:
        dlxPerPhyX = 0;
        dlyPerPhyX = -1;
        break;
      case LandscapeClockwise:
        dlxPerPhyX = -1;
        dlyPerPhyX = 0;
        break;
      case LandscapeCounterClockwise:
        dlxPerPhyX = 1;
        dlyPerPhyX = 0;
        break;
    }

    // The dither pattern has period 2 in logical space, and each orientation
    // maps py to logical coords with a fixed parity relationship. The
    // blackMask byte therefore repeats with period 2 in py. Precompute both
    // variants outside the row loop to eliminate the per-row switch + 8-bit
    // construction loop.
    uint8_t blackMasks[2];
    for (int parityIdx = 0; parityIdx < 2; ++parityIdx) {
      const int samplePy = phyY0 + parityIdx;
      int lxBase = 0, lyBase = 0;
      switch (orientation) {
        case Portrait:
          lxBase = panelHeight - 1 - samplePy;
          lyBase = byteStart * 8;
          break;
        case PortraitInverted:
          lxBase = samplePy;
          lyBase = panelWidth - 1 - byteStart * 8;
          break;
        case LandscapeClockwise:
          lxBase = panelWidth - 1 - byteStart * 8;
          lyBase = panelHeight - 1 - samplePy;
          break;
        case LandscapeCounterClockwise:
          lxBase = byteStart * 8;
          lyBase = samplePy;
          break;
      }
      uint8_t mask = 0;
      for (int b = 0; b < 8; ++b) {
        const int lx = lxBase + b * dlxPerPhyX;
        const int ly = lyBase + b * dlyPerPhyX;
        bool isBlack;
        if constexpr (C == Color::LightGray) {
          isBlack = ((lx & 1) == 0) && ((ly & 1) == 0);
        } else {  // DarkGray
          isBlack = (((lx + ly) & 1) == 0);
        }
        if (isBlack) mask |= static_cast<uint8_t>(1u << (7 - b));
      }
      blackMasks[samplePy & 1] = mask;
    }

    for (int py = phyY0; py <= phyY1; ++py) {
      const uint8_t blackMask = blackMasks[py & 1];
      const uint8_t whiteMask = static_cast<uint8_t>(~blackMask);

      // Dither writes BOTH inks (the slow path called drawPixel for every
      // pixel — setting or clearing — so we must do the same). Inside the
      // rect mask: write whiteMask (1s where white, 0s where black). Outside
      // the rect mask: leave the framebuffer untouched.
      uint8_t* row = target + static_cast<int32_t>(py - originY) * panelStride;
      if (byteStart == byteEnd) {
        const uint8_t rectMask = headMask & tailMask;
        row[byteStart] = static_cast<uint8_t>((row[byteStart] & ~rectMask) | (rectMask & whiteMask));
      } else {
        row[byteStart] = static_cast<uint8_t>((row[byteStart] & ~headMask) | (headMask & whiteMask));
        if (byteEnd > byteStart + 1) {
          // Period 2, so every full byte in this row is exactly whiteMask.
          memset(row + byteStart + 1, whiteMask, byteEnd - byteStart - 1);
        }
        row[byteEnd] = static_cast<uint8_t>((row[byteEnd] & ~tailMask) | (tailMask & whiteMask));
      }
    }
  }
}

int Old::getScreenWidth() const {
  switch (orientation) {
    case Portrait:
    case PortraitInverted:
      // 480px wide in portrait logical coordinates
      return panelHeight;
    case LandscapeClockwise:
    case LandscapeCounterClockwise:
      // 800px wide in landscape logical coordinates
      return panelWidth;
  }
  return panelHeight;
}

int Old::getScreenHeight() const {
  switch (orientation) {
    case Portrait:
    case PortraitInverted:
      // 800px tall in portrait logical coordinates
      return panelWidth;
    case LandscapeClockwise:
    case LandscapeCounterClockwise:
      // 480px tall in landscape logical coordinates
      return panelHeight;
  }
  return panelWidth;
}

void Old::drawArc(const int maxRadius, const int cx, const int cy, const int xDir, const int yDir, const int lineWidth,
                  const bool state) const {
  const int stroke = std::min(lineWidth, maxRadius);
  const int innerRadius = std::max(maxRadius - stroke, 0);
  const int outerRadius = maxRadius;

  if (outerRadius <= 0) {
    return;
  }

  const int outerRadiusSq = outerRadius * outerRadius;
  const int innerRadiusSq = innerRadius * innerRadius;

  int xOuter = outerRadius;
  int xInner = innerRadius;

  for (int dy = 0; dy <= outerRadius; ++dy) {
    while (xOuter > 0 && (xOuter * xOuter + dy * dy) > outerRadiusSq) {
      --xOuter;
    }
    // Keep the smallest x that still lies outside/at the inner radius,
    // i.e. (x^2 + y^2) >= innerRadiusSq.
    while (xInner > 0 && ((xInner - 1) * (xInner - 1) + dy * dy) >= innerRadiusSq) {
      --xInner;
    }

    if (xOuter < xInner) {
      continue;
    }

    const int x0 = cx + xDir * xInner;
    const int x1 = cx + xDir * xOuter;
    const int left = std::min(x0, x1);
    const int width = std::abs(x1 - x0) + 1;
    const int py = cy + yDir * dy;

    fillRect(left, py, width, 1, state);
  }
};

// Border is inside the rectangle, rounded corners
void Old::drawRoundedRect(const int x, const int y, const int width, const int height, const int lineWidth,
                          const int cornerRadius, bool roundTopLeft, bool roundTopRight, bool roundBottomLeft,
                          bool roundBottomRight, bool state) const {
  if (lineWidth <= 0 || width <= 0 || height <= 0) {
    return;
  }

  const int maxRadius = std::min({cornerRadius, width / 2, height / 2});
  if (maxRadius <= 0) {
    drawRect(x, y, width, height, lineWidth, state);
    return;
  }

  const int stroke = std::min(lineWidth, maxRadius);
  const int right = x + width - 1;
  const int bottom = y + height - 1;

  const int horizontalWidth = width - 2 * maxRadius;
  if (horizontalWidth > 0) {
    if (roundTopLeft || roundTopRight) {
      fillRect(x + maxRadius, y, horizontalWidth, stroke, state);
    }
    if (roundBottomLeft || roundBottomRight) {
      fillRect(x + maxRadius, bottom - stroke + 1, horizontalWidth, stroke, state);
    }
  }

  const int verticalHeight = height - 2 * maxRadius;
  if (verticalHeight > 0) {
    if (roundTopLeft || roundBottomLeft) {
      fillRect(x, y + maxRadius, stroke, verticalHeight, state);
    }
    if (roundTopRight || roundBottomRight) {
      fillRect(right - stroke + 1, y + maxRadius, stroke, verticalHeight, state);
    }
  }

  if (roundTopLeft) {
    drawArc(maxRadius, x + maxRadius, y + maxRadius, -1, -1, lineWidth, state);
  }
  if (roundTopRight) {
    drawArc(maxRadius, right - maxRadius, y + maxRadius, 1, -1, lineWidth, state);
  }
  if (roundBottomRight) {
    drawArc(maxRadius, right - maxRadius, bottom - maxRadius, 1, 1, lineWidth, state);
  }
  if (roundBottomLeft) {
    drawArc(maxRadius, x + maxRadius, bottom - maxRadius, -1, 1, lineWidth, state);
  }
}

void Old::fillRect(const int x, const int y, const int width, const int height, const bool state) const {
  if (state) {
    fillRectImpl<Color::Black>(x, y, width, height);
  } else {
    fillRectImpl<Color::White>(x, y, width, height);
  }
}

// NOTE: Those are in critical path, and need to be templated to avoid runtime checks for every pixel.
// Any branching must be done outside the loops to avoid performance degradation.
template <>
void Old::drawPixelDither<Color::Clear>(const int x, const int y) const {
  // Do nothing
}

template <>
void Old::drawPixelDither<Color::Black>(const int x, const int y) const {
  drawPixel(x, y, true);
}

template <>
void Old::drawPixelDither<Color::White>(const int x, const int y) const {
  drawPixel(x, y, false);
}

template <>
void Old::drawPixelDither<Color::LightGray>(const int x, const int y) const {
  drawPixel(x, y, x % 2 == 0 && y % 2 == 0);
}

template <>
void Old::drawPixelDither<Color::DarkGray>(const int x, const int y) const {
  drawPixel(x, y, (x + y) % 2 == 0);  // TODO: maybe find a better pattern?
}

void Old::fillRectDither(const int x, const int y, const int width, const int height, Color color) const {
  switch (color) {
    case Color::Clear:
      break;
    case Color::Black:
      fillRectImpl<Color::Black>(x, y, width, height);
      break;
    case Color::White:
      fillRectImpl<Color::White>(x, y, width, height);
      break;
    case Color::LightGray:
      fillRectImpl<Color::LightGray>(x, y, width, height);
      break;
    case Color::DarkGray:
      fillRectImpl<Color::DarkGray>(x, y, width, height);
      break;
  }
}

void Old::maskRoundedRectOutsideCorners(const int x, const int y, const int width, const int height, const int radius,
                                        const Color color) const {
  if (radius <= 0 || color == Color::Clear) {
    return;
  }

  const int rr = radius - 1;
  const int rr2 = rr * rr;
  for (int dy = 0; dy < radius; dy++) {
    for (int dx = 0; dx < radius; dx++) {
      const int tx = rr - dx;
      const int ty = rr - dy;
      if (tx * tx + ty * ty > rr2) {
        if (color == Color::White || color == Color::Black) {
          bool state = color == Color::Black;
          drawPixel(x + dx, y + dy, state);                           // top-left
          drawPixel(x + width - 1 - dx, y + dy, state);               // top-right
          drawPixel(x + dx, y + height - 1 - dy, state);              // bottom-left
          drawPixel(x + width - 1 - dx, y + height - 1 - dy, state);  // bottom-right
        } else if (color == Color::LightGray) {
          drawPixelDither<Color::LightGray>(x + dx, y + dy);                           // top-left
          drawPixelDither<Color::LightGray>(x + width - 1 - dx, y + dy);               // top-right
          drawPixelDither<Color::LightGray>(x + dx, y + height - 1 - dy);              // bottom-left
          drawPixelDither<Color::LightGray>(x + width - 1 - dx, y + height - 1 - dy);  // bottom-right
        } else if (color == Color::DarkGray) {
          drawPixelDither<Color::DarkGray>(x + dx, y + dy);                           // top-left
          drawPixelDither<Color::DarkGray>(x + width - 1 - dx, y + dy);               // top-right
          drawPixelDither<Color::DarkGray>(x + dx, y + height - 1 - dy);              // bottom-left
          drawPixelDither<Color::DarkGray>(x + width - 1 - dx, y + height - 1 - dy);  // bottom-right
        }
      }
    }
  }
}

template <Color color>
void Old::fillArc(const int maxRadius, const int cx, const int cy, const int xDir, const int yDir) const {
  if (maxRadius <= 0) return;

  if constexpr (color == Color::Clear) {
    return;
  }

  const int radiusSq = maxRadius * maxRadius;

  // Avoid sqrt by scanning from outer radius inward while y grows.
  int x = maxRadius;
  for (int dy = 0; dy <= maxRadius; ++dy) {
    while (x > 0 && (x * x + dy * dy) > radiusSq) {
      --x;
    }
    if (x < 0) break;

    const int py = cy + yDir * dy;
    if (py < 0 || py >= getScreenHeight()) continue;

    int x0 = cx;
    int x1 = cx + xDir * x;
    if (x0 > x1) std::swap(x0, x1);
    const int width = x1 - x0 + 1;

    if (width <= 0) continue;

    if constexpr (color == Color::Black) {
      fillRect(x0, py, width, 1, true);
    } else if constexpr (color == Color::White) {
      fillRect(x0, py, width, 1, false);
    } else {
      // LightGray / DarkGray: use existing dithered fill path.
      fillRectDither(x0, py, width, 1, color);
    }
  }
}

void Old::fillRoundedRect(const int x, const int y, const int width, const int height, const int cornerRadius,
                          bool roundTopLeft, bool roundTopRight, bool roundBottomLeft, bool roundBottomRight,
                          const Color color) const {
  if (width <= 0 || height <= 0) {
    return;
  }

  // Assume if we're not rounding all corners then we are only rounding one side
  const int roundedSides = (!roundTopLeft || !roundTopRight || !roundBottomLeft || !roundBottomRight) ? 1 : 2;
  const int maxRadius = std::min({cornerRadius, width / roundedSides, height / roundedSides});
  if (maxRadius <= 0) {
    fillRectDither(x, y, width, height, color);
    return;
  }

  const int horizontalWidth = width - 2 * maxRadius;
  if (horizontalWidth > 0) {
    fillRectDither(x + maxRadius + 1, y, horizontalWidth - 2, height, color);
  }

  const int leftFillTop = y + (roundTopLeft ? (maxRadius + 1) : 0);
  const int leftFillBottom = y + height - 1 - (roundBottomLeft ? (maxRadius + 1) : 0);
  if (leftFillBottom >= leftFillTop) {
    fillRectDither(x, leftFillTop, maxRadius + 1, leftFillBottom - leftFillTop + 1, color);
  }

  const int rightFillTop = y + (roundTopRight ? (maxRadius + 1) : 0);
  const int rightFillBottom = y + height - 1 - (roundBottomRight ? (maxRadius + 1) : 0);
  if (rightFillBottom >= rightFillTop) {
    fillRectDither(x + width - maxRadius - 1, rightFillTop, maxRadius + 1, rightFillBottom - rightFillTop + 1, color);
  }

  auto fillArcTemplated = [this](int maxRadius, int cx, int cy, int xDir, int yDir, Color color) {
    switch (color) {
      case Color::Clear:
        break;
      case Color::Black:
        fillArc<Color::Black>(maxRadius, cx, cy, xDir, yDir);
        break;
      case Color::White:
        fillArc<Color::White>(maxRadius, cx, cy, xDir, yDir);
        break;
      case Color::LightGray:
        fillArc<Color::LightGray>(maxRadius, cx, cy, xDir, yDir);
        break;
      case Color::DarkGray:
        fillArc<Color::DarkGray>(maxRadius, cx, cy, xDir, yDir);
        break;
    }
  };

  if (roundTopLeft) {
    fillArcTemplated(maxRadius, x + maxRadius, y + maxRadius, -1, -1, color);
  }

  if (roundTopRight) {
    fillArcTemplated(maxRadius, x + width - maxRadius - 1, y + maxRadius, 1, -1, color);
  }

  if (roundBottomRight) {
    fillArcTemplated(maxRadius, x + width - maxRadius - 1, y + height - maxRadius - 1, 1, 1, color);
  }

  if (roundBottomLeft) {
    fillArcTemplated(maxRadius, x + maxRadius, y + height - maxRadius - 1, -1, 1, color);
  }
}

void Old::fillPolygon(const int* xPoints, const int* yPoints, int numPoints, bool state) const {
  if (numPoints < 3) return;

  // Find bounding box
  int minY = yPoints[0], maxY = yPoints[0];
  for (int i = 1; i < numPoints; i++) {
    if (yPoints[i] < minY) minY = yPoints[i];
    if (yPoints[i] > maxY) maxY = yPoints[i];
  }

  // Clip to screen
  if (minY < 0) minY = 0;
  if (maxY >= getScreenHeight()) maxY = getScreenHeight() - 1;

  // Allocate node buffer for scanline algorithm
  auto nodeX = makeUniqueNoThrow<int[]>(numPoints);
  if (!nodeX) {
    LOG_ERR("GFX", "!! Failed to allocate polygon node buffer");
    return;
  }

  // Scanline fill algorithm
  for (int scanY = minY; scanY <= maxY; scanY++) {
    int nodes = 0;

    // Find all intersection points with edges
    int j = numPoints - 1;
    for (int i = 0; i < numPoints; i++) {
      if ((yPoints[i] < scanY && yPoints[j] >= scanY) || (yPoints[j] < scanY && yPoints[i] >= scanY)) {
        // Calculate X intersection using fixed-point to avoid float
        int dy = yPoints[j] - yPoints[i];
        if (dy != 0) {
          nodeX[nodes++] = xPoints[i] + (scanY - yPoints[i]) * (xPoints[j] - xPoints[i]) / dy;
        }
      }
      j = i;
    }

    // Sort nodes by X
    std::sort(nodeX.get(), nodeX.get() + nodes);

    // Fill between pairs of nodes
    for (int i = 0; i < nodes - 1; i += 2) {
      int startX = nodeX[i];
      int endX = nodeX[i + 1];

      // Clip to screen
      if (startX < 0) startX = 0;
      if (endX >= getScreenWidth()) endX = getScreenWidth() - 1;

      // Draw horizontal line
      for (int x = startX; x <= endX; x++) {
        drawPixel(x, scanY, state);
      }
    }
  }
}

void Old::drawIcon(const uint8_t bitmap[], const int x, const int y, const int size) const {
  // Plot the icon pixel-by-pixel through drawPixel (which applies the orientation
  // transform) instead of the byte-aligned framebuffer blit. The blit snaps the
  // icon's position to 8px (one byte) along the rotated axis, which prevents it
  // from aligning with adjacent text; per-pixel plotting is pixel-precise.
  // Icons are square and 1bpp (MSB-first, bit==0 = ink). The (size-1-row, col)
  // mapping reproduces the Portrait orientation the blit produced; drawIcon is
  // only called by the UI themes, which all render in forced Portrait.
  const int rowBytes = (size + 7) / 8;
  for (int row = 0; row < size; row++) {
    for (int col = 0; col < size; col++) {
      const uint8_t byte = bitmap[row * rowBytes + (col >> 3)];
      const bool ink = ((byte >> (7 - (col & 7))) & 1) == 0;
      if (ink) {
        drawPixel(x + (size - 1 - row), y + col, true);
      }
    }
  }
}

bool Old::drawBitmap(const Bitmap& bitmap, const int x, const int y, const int maxWidth, const int maxHeight,
                     const float cropX, const float cropY, const bool whiteAsTransparent) const {
  if (fontCacheManager_ && fontCacheManager_->isScanning()) return false;
  // For 1-bit bitmaps, use optimized 1-bit rendering path (no crop support for 1-bit)
  if (bitmap.is1Bit() && cropX == 0.0f && cropY == 0.0f) {
    return drawBitmap1Bit(bitmap, x, y, maxWidth, maxHeight);
  }

  float scale = 1.0f;
  bool isScaled = false;
  int cropPixX = std::floor(bitmap.getWidth() * cropX / 2.0f);
  int cropPixY = std::floor(bitmap.getHeight() * cropY / 2.0f);
  LOG_DBG("GFX", "Cropping %dx%d by %dx%d pix, is %s", bitmap.getWidth(), bitmap.getHeight(), cropPixX, cropPixY,
          bitmap.isTopDown() ? "top-down" : "bottom-up");

  const float croppedWidth = (1.0f - cropX) * static_cast<float>(bitmap.getWidth());
  const float croppedHeight = (1.0f - cropY) * static_cast<float>(bitmap.getHeight());
  bool hasTargetBounds = false;
  float fitScale = 1.0f;

  if (maxWidth > 0 && croppedWidth > 0.0f) {
    fitScale = static_cast<float>(maxWidth) / croppedWidth;
    hasTargetBounds = true;
  }

  if (maxHeight > 0 && croppedHeight > 0.0f) {
    const float heightScale = static_cast<float>(maxHeight) / croppedHeight;
    fitScale = hasTargetBounds ? std::min(fitScale, heightScale) : heightScale;
    hasTargetBounds = true;
  }

  if (hasTargetBounds && fitScale < 1.0f) {
    scale = fitScale;
    isScaled = true;
  }
  LOG_DBG("GFX", "Scaling by %f - %s", scale, isScaled ? "scaled" : "not scaled");

  // Calculate output row size (2 bits per pixel, packed into bytes)
  // IMPORTANT: Use int, not uint8_t, to avoid overflow for images > 1020 pixels wide
  const int outputRowSize = (bitmap.getWidth() + 3) / 4;
  const auto rowBytesSize = static_cast<size_t>(bitmap.getRowBytes());
  auto rowScratch = makeUniqueNoThrow<uint8_t[]>(static_cast<size_t>(outputRowSize) + rowBytesSize);
  if (!rowScratch) {
    LOG_ERR("GFX", "!! Failed to allocate BMP row buffers");

    return false;
  }

  auto* outputRow = rowScratch.get();
  auto* rowBytes = rowScratch.get() + outputRowSize;

  for (int bmpY = 0; bmpY < (bitmap.getHeight() - cropPixY); bmpY++) {
    // The BMP's (0, 0) is the bottom-left corner (if the height is positive, top-left if negative).
    // Screen's (0, 0) is the top-left corner.
    int screenY = -cropPixY + (bitmap.isTopDown() ? bmpY : bitmap.getHeight() - 1 - bmpY);
    if (isScaled) {
      screenY = std::floor(screenY * scale);
    }
    screenY += y;  // the offset should not be scaled
    if (screenY >= getScreenHeight()) {
      break;
    }

    if (bitmap.readNextRow(outputRow, rowBytes) != BmpReaderError::Ok) {
      LOG_ERR("GFX", "Failed to read row %d from bitmap", bmpY);

      return false;
    }

    if (screenY < 0) {
      continue;
    }

    if (bmpY < cropPixY) {
      // Skip the row if it's outside the crop area
      continue;
    }

    for (int bmpX = cropPixX; bmpX < bitmap.getWidth() - cropPixX; bmpX++) {
      int screenX = bmpX - cropPixX;
      if (isScaled) {
        screenX = std::floor(screenX * scale);
      }
      screenX += x;  // the offset should not be scaled
      if (screenX >= getScreenWidth()) {
        break;
      }
      if (screenX < 0) {
        continue;
      }

      const uint8_t val = outputRow[bmpX / 4] >> (6 - ((bmpX * 2) % 8)) & 0x3;
      if (whiteAsTransparent && val == 3) continue;

      if (renderMode == BW && val < 3) {
        drawPixel(screenX, screenY);
      } else if (renderMode == GRAYSCALE_LSB || renderMode == GRAYSCALE_MSB) {
        const auto pixel = grayPlanePixel(val, renderMode == GRAYSCALE_MSB, absoluteGrayPlanes);
        if (pixel.write) drawPixel(screenX, screenY, pixel.black);
      }
    }
  }

  const int sourceWidth = bitmap.getWidth() - cropPixX * 2;
  const int sourceHeight = bitmap.getHeight() - cropPixY * 2;
  const int renderedWidth = isScaled ? static_cast<int>(std::floor((sourceWidth - 1) * scale)) + 1 : sourceWidth;
  const int renderedHeight = isScaled ? static_cast<int>(std::floor((sourceHeight - 1) * scale)) + 1 : sourceHeight;
  preserveImagePolarity(x, y, renderedWidth, renderedHeight);
  return true;
}

bool Old::drawBitmap1Bit(const Bitmap& bitmap, const int x, const int y, const int maxWidth,
                         const int maxHeight) const {
  float scale = 1.0f;
  bool isScaled = false;
  if (maxWidth > 0 && bitmap.getWidth() > maxWidth) {
    scale = static_cast<float>(maxWidth) / static_cast<float>(bitmap.getWidth());
    isScaled = true;
  }
  if (maxHeight > 0 && bitmap.getHeight() > maxHeight) {
    scale = std::min(scale, static_cast<float>(maxHeight) / static_cast<float>(bitmap.getHeight()));
    isScaled = true;
  }

  // For 1-bit BMP, output is still 2-bit packed (for consistency with readNextRow)
  const int outputRowSize = (bitmap.getWidth() + 3) / 4;
  const auto rowBytesSize = static_cast<size_t>(bitmap.getRowBytes());
  auto rowScratch = makeUniqueNoThrow<uint8_t[]>(static_cast<size_t>(outputRowSize) + rowBytesSize);
  if (!rowScratch) {
    LOG_ERR("GFX", "!! Failed to allocate 1-bit BMP row buffers");

    return false;
  }

  auto* outputRow = rowScratch.get();
  auto* rowBytes = rowScratch.get() + outputRowSize;

  for (int bmpY = 0; bmpY < bitmap.getHeight(); bmpY++) {
    // Read rows sequentially using readNextRow
    if (bitmap.readNextRow(outputRow, rowBytes) != BmpReaderError::Ok) {
      LOG_ERR("GFX", "Failed to read row %d from 1-bit bitmap", bmpY);

      return false;
    }

    // Calculate screen Y based on whether BMP is top-down or bottom-up
    const int bmpYOffset = bitmap.isTopDown() ? bmpY : bitmap.getHeight() - 1 - bmpY;
    int screenY = y + (isScaled ? static_cast<int>(std::floor(bmpYOffset * scale)) : bmpYOffset);
    if (screenY >= getScreenHeight()) {
      continue;  // Continue reading to keep row counter in sync
    }
    if (screenY < 0) {
      continue;
    }

    for (int bmpX = 0; bmpX < bitmap.getWidth(); bmpX++) {
      int screenX = x + (isScaled ? static_cast<int>(std::floor(bmpX * scale)) : bmpX);
      if (screenX >= getScreenWidth()) {
        break;
      }
      if (screenX < 0) {
        continue;
      }

      // Get 2-bit value (result of readNextRow quantization)
      const uint8_t val = outputRow[bmpX / 4] >> (6 - ((bmpX * 2) % 8)) & 0x3;

      // For 1-bit source: 0 or 1 -> map to black (0,1,2) or white (3)
      // val < 3 means black pixel (draw it)
      if (val < 3) {
        drawPixel(screenX, screenY, true);
      }
      // White pixels (val == 3) are not drawn (leave background)
    }
  }

  const int renderedWidth =
      isScaled ? static_cast<int>(std::floor((bitmap.getWidth() - 1) * scale)) + 1 : bitmap.getWidth();
  const int renderedHeight =
      isScaled ? static_cast<int>(std::floor((bitmap.getHeight() - 1) * scale)) + 1 : bitmap.getHeight();
  preserveImagePolarity(x, y, renderedWidth, renderedHeight);
  return true;
}

void Old::preserveImagePolarity(const int x, const int y, const int width, const int height) const {
  if (renderMode != BW || !display.isInverted() || _stripActive || !frameBuffer || width <= 0 || height <= 0) {
    return;
  }

  const int lx0 = std::max(x, clipLeft_);
  const int ly0 = std::max(y, clipTop_);
  const int lx1 = std::min(x + width, clipRight_);
  const int ly1 = std::min(y + height, clipBottom_);
  if (lx0 >= lx1 || ly0 >= ly1) return;

  int ax, ay, bx, by;
  rotateCoordinates(orientation, lx0, ly0, &ax, &ay, panelWidth, panelHeight);
  rotateCoordinates(orientation, lx1 - 1, ly1 - 1, &bx, &by, panelWidth, panelHeight);

  int left = std::max(0, std::min(ax, bx));
  int right = std::min(static_cast<int>(panelWidth) - 1, std::max(ax, bx));
  int top = std::max(0, std::min(ay, by));
  int bottom = std::min(static_cast<int>(panelHeight) - 1, std::max(ay, by));
  if (left > right || top > bottom) return;

  for (int row = top; row <= bottom; row++) {
    uint8_t* rowData = frameBuffer + static_cast<uint32_t>(row) * panelWidthBytes;
    int col = left;
    while (col <= right && (col & 7) != 0) {
      rowData[col >> 3] ^= static_cast<uint8_t>(0x80U >> (col & 7));
      col++;
    }
    while (col + 7 <= right) {
      rowData[col >> 3] ^= 0xFF;
      col += 8;
    }
    while (col <= right) {
      rowData[col >> 3] ^= static_cast<uint8_t>(0x80U >> (col & 7));
      col++;
    }
  }
}

// ---- The scaled glyph loops of renderCharScaled and drawScaledCodepoint at 87894d21, verbatim from
// the glyph metrics on (glyph lookup and advance stay in C++). ----

struct OldGlyphMetrics {
  int width, height, left, top;
};
struct OldFontData {
  bool is2Bit;
};

void Old::renderCharScaled(const uint8_t* bitmap, const OldGlyphMetrics* glyph, const OldFontData* fontData,
                           const int cursorX, const int cursorY, const bool pixelState, const int scaleNum,
                           const int scaleDen) const {
  const Old& renderer = *this;
  const int srcW = glyph->width;
  const int srcH = glyph->height;
  const int dstW = (srcW * scaleNum + scaleDen - 1) / scaleDen;  // ceil so edge columns aren't clipped
  const int dstH = (srcH * scaleNum + scaleDen - 1) / scaleDen;
  // Scale the glyph bearing by the same factor so the scaled glyph sits at the correct
  // pixel offset from the (already-shifted) cursor position.
  const int baseX = cursorX + glyph->left * scaleNum / scaleDen;
  const int baseY = cursorY - glyph->top * scaleNum / scaleDen;
  const bool halving = scaleNum * 2 == scaleDen;
  const bool is2Bit = fontData->is2Bit;

  // Raw sample value: 1-bit fonts read 0 or 3, 2-bit fonts 0 (white) .. 3 (black).
  const auto sample = [&](const int pos) -> uint8_t {
    if (is2Bit) return (bitmap[pos >> 2] >> ((3 - (pos & 3)) * 2)) & 0x3;
    return ((bitmap[pos >> 3] >> (7 - (pos & 7))) & 1) ? 3 : 0;
  };

  for (int dstY = 0; dstY < dstH; dstY++) {
    const int srcY0 = dstY * scaleDen / scaleNum;
    const int srcY1 = std::min(srcH, std::max(srcY0 + 1, ((dstY + 1) * scaleDen + scaleNum - 1) / scaleNum));
    for (int dstX = 0; dstX < dstW; dstX++) {
      const int srcX0 = dstX * scaleDen / scaleNum;
      const int srcX1 = std::min(srcW, std::max(srcX0 + 1, ((dstX + 1) * scaleDen + scaleNum - 1) / scaleNum));
      int coverage = 0;
      uint8_t maxRaw = 0;
      int samples = 0;
      for (int y = srcY0; y < srcY1; y++) {
        for (int x = srcX0; x < srcX1; x++) {
          const uint8_t raw = sample(y * srcW + x);
          coverage += raw;
          maxRaw = std::max(maxRaw, raw);
          samples++;
        }
      }
      const bool ink = halving ? (is2Bit ? (maxRaw >= 2 || coverage >= 2) : maxRaw != 0)
                               : samples > 0 && coverage * 2 >= samples * 3;
      if (ink) renderer.drawPixel(baseX + dstX, baseY + dstY, pixelState);
    }
  }
}

void Old::drawScaledCodepoint(const uint8_t* bitmap, const OldGlyphMetrics* glyph, const OldFontData* fontData,
                              const int x, const int baselineY, const int scale256) const {
  const int srcW = glyph->width;
  const int srcH = glyph->height;
  const bool is2Bit = fontData->is2Bit;
  // Raw coverage: 1-bit fonts read 0 or 3, 2-bit fonts 0 (white) .. 3 (black); 0 outside the bitmap.
  const auto sample = [&](const int sx, const int sy) -> int {
    if (sx < 0 || sy < 0 || sx >= srcW || sy >= srcH) return 0;
    const int pos = sy * srcW + sx;
    if (is2Bit) return (bitmap[pos >> 2] >> ((3 - (pos & 3)) * 2)) & 0x3;
    return ((bitmap[pos >> 3] >> (7 - (pos & 7))) & 1) ? 3 : 0;
  };

  const int dstW = (srcW * scale256 + 255) / 256;
  const int dstH = (srcH * scale256 + 255) / 256;
  const int left = x + glyph->left * scale256 / 256;
  const int top = baselineY - glyph->top * scale256 / 256;
  // Source position of each destination pixel centre, 16.16 fixed point.
  const int32_t step = (256 << 16) / scale256;
  const int32_t origin = step / 2 - (1 << 15);
  for (int dy = 0; dy < dstH; ++dy) {
    const int32_t sy = origin + dy * step;
    const int y0 = sy >> 16;
    const int fy = (sy >> 8) & 0xFF;
    for (int dx = 0; dx < dstW; ++dx) {
      const int32_t sx = origin + dx * step;
      const int x0 = sx >> 16;
      const int fx = (sx >> 8) & 0xFF;
      const int upper = sample(x0, y0) * (256 - fx) + sample(x0 + 1, y0) * fx;
      const int lower = sample(x0, y0 + 1) * (256 - fx) + sample(x0 + 1, y0 + 1) * fx;
      // Ink where interpolated coverage reaches half of full black (3).
      if ((upper * (256 - fy) + lower * fy) * 2 >= 3 * 256 * 256) drawPixel(left + dx, top + dy, true);
    }
  }
}

// ---- The comparison. ----

static uint32_t seed;
static uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}
static int range(int lo, int hi) { return lo + static_cast<int>(rnd() % static_cast<uint32_t>(hi - lo + 1)); }

TEST(GfxPrimitivesTest, MatchTheOldRenderer) {
  seed = 7;
  const int cases = 20000;
  unsigned diffs = 0;
  for (int c = 0; c < cases; ++c) {
    Old o;
    o.panelWidthBytes = static_cast<uint16_t>(range(1, 12));
    o.panelWidth = static_cast<uint16_t>(o.panelWidthBytes * 8 - (rnd() % 3 == 0 ? range(0, 7) : 0));
    o.panelHeight = static_cast<uint16_t>(range(1, 70));
    o.orientation = static_cast<Old::Orientation>(rnd() % 4);
    std::vector<uint8_t> a(o.panelWidthBytes * o.panelHeight), b;
    for (auto& v : a) v = static_cast<uint8_t>(rnd());
    b = a;
    std::vector<uint8_t> sa, sb;
    if (rnd() % 3 == 0) {
      o._stripActive = true;
      o._stripY0 = range(0, o.panelHeight - 1);
      o._stripRows = range(1, o.panelHeight - o._stripY0);
      sa.resize(o.panelWidthBytes * o._stripRows);
      for (auto& v : sa) v = static_cast<uint8_t>(rnd());
      sb = sa;
    }
    if (rnd() % 2) {
      o.clipLeft_ = range(-10, 60);
      o.clipTop_ = range(-10, 60);
      o.clipRight_ = range(-10, 120);
      o.clipBottom_ = range(-10, 120);
    }
    for (int op = 0; op < 12; ++op) {
      const int kind = range(0, 10);
      const int x1 = range(-40, 140), y1 = range(-40, 140), x2 = range(-40, 140), y2 = range(-40, 140);
      const int w = range(-5, 120), h = range(-5, 120), lw = range(-1, 6);
      const bool black = rnd() % 2;
      const Color colors[] = {Clear, White, LightGray, DarkGray, Black};
      const Color color = colors[rnd() % 5];
      const int radius = range(-3, 40), xDir = rnd() % 2 ? 1 : -1, yDir = rnd() % 2 ? 1 : -1;
      const uint8_t round = rnd() % 3 == 0 ? 15 : static_cast<uint8_t>(rnd() % 16);
      int xs[8], ys[8];
      const int points = range(0, 8);
      for (int p = 0; p < points; ++p) {
        xs[p] = range(-40, 140);
        ys[p] = range(-40, 140);
      }
      const bool tl = round & BB_GFX_TOP_LEFT, tr = round & BB_GFX_TOP_RIGHT, bl = round & BB_GFX_BOTTOM_LEFT,
                 br = round & BB_GFX_BOTTOM_RIGHT;
      // Old
      o.frameBuffer = a.data();
      o._stripBuf = sa.data();
      o.offPanel = 0;
      switch (kind) {
        case 0:
          o.drawLine(x1, y1, x2, y2, black);
          break;
        case 1:
          o.drawLine(x1, y1, x2, y2, lw, black);
          break;
        case 2:
          o.drawRect(x1, y1, w, h, black);
          break;
        case 3:
          o.drawRect(x1, y1, w, h, lw, black);
          break;
        case 6:
          o.drawArc(radius, x1, y1, xDir, yDir, lw, black);
          break;
        case 7:
          o.drawRoundedRect(x1, y1, w, h, lw, radius, tl, tr, bl, br, black);
          break;
        case 8:
          o.fillRoundedRect(x1, y1, w, h, radius, tl, tr, bl, br, color);
          break;
        case 9:
          o.maskRoundedRectOutsideCorners(x1, y1, w, h, radius, color);
          break;
        case 10:
          o.fillPolygon(xs, ys, points, black);
          break;
        default:
          if (w > 0 && h > 0) {
            switch (color) {
              case Black:
                o.fillRectImpl<Black>(x1, y1, w, h);
                break;
              case White:
                o.fillRectImpl<White>(x1, y1, w, h);
                break;
              case LightGray:
                o.fillRectImpl<LightGray>(x1, y1, w, h);
                break;
              case DarkGray:
                o.fillRectImpl<DarkGray>(x1, y1, w, h);
                break;
              default:
                break;
            }
          }
      }
      const unsigned oldOff = o.offPanel;
      // New
      bb_gfx_canvas cv{};
      cv.buf = o._stripActive ? sb.data() : b.data();
      cv.rows = o._stripActive ? o._stripRows : o.panelHeight;
      cv.origin_y = o._stripActive ? o._stripY0 : 0;
      cv.stride = o.panelWidthBytes;
      cv.len = static_cast<size_t>(cv.rows) * o.panelWidthBytes;
      cv.panel_width = o.panelWidth;
      cv.panel_height = o.panelHeight;
      cv.clip_left = o.clipLeft_;
      cv.clip_top = o.clipTop_;
      cv.clip_right = o.clipRight_;
      cv.clip_bottom = o.clipBottom_;
      cv.orientation = static_cast<uint8_t>(o.orientation);
      unsigned newOff = 0;
      switch (kind) {
        case 0:
          newOff = bb_gfx_line(&cv, x1, y1, x2, y2, 1, black);
          break;
        case 1:
          newOff = bb_gfx_line(&cv, x1, y1, x2, y2, lw, black);
          break;
        case 2:
          newOff = bb_gfx_rect(&cv, x1, y1, w, h, 1, black);
          break;
        case 3:
          newOff = bb_gfx_rect(&cv, x1, y1, w, h, lw, black);
          break;
        case 6:
          bb_gfx_arc(&cv, radius, x1, y1, xDir, yDir, lw, black);
          break;
        case 7:
          newOff = bb_gfx_rounded_rect(&cv, x1, y1, w, h, lw, radius, round, black);
          break;
        case 8:
          bb_gfx_fill_rounded_rect(&cv, x1, y1, w, h, radius, round, color);
          break;
        case 9:
          newOff = bb_gfx_mask_corners(&cv, x1, y1, w, h, radius, color);
          break;
        case 10:
          bb_gfx_fill_polygon(&cv, xs, ys, static_cast<size_t>(points), black);
          break;
        default:
          if (w > 0 && h > 0) bb_gfx_fill_rect(&cv, x1, y1, w, h, color);
      }
      if (a != b || sa != sb || oldOff != newOff) {
        ADD_FAILURE() << "case " << c << " op " << op << " kind " << kind << " orientation " << o.orientation
                      << " strip " << o._stripActive << " off " << oldOff << "/" << newOff;
        if (++diffs == 5) return;
        a = b;
        sa = sb;
      }
    }
  }
}

namespace {
bool readRow(void* ctx, uint8_t* row) {
  return static_cast<const Bitmap*>(ctx)->readNextRow(row, nullptr) == BmpReaderError::Ok;
}
}  // namespace

// Pictures (both paths, crops, shrinking, the three planes, transparency, failed reads), icons and
// the polarity inversion.
TEST(GfxPrimitivesTest, PicturesMatchTheOldRenderer) {
  seed = 11;
  unsigned diffs = 0;
  for (int c = 0; c < 6000; ++c) {
    Old o;
    o.panelWidthBytes = static_cast<uint16_t>(range(1, 12));
    o.panelWidth = static_cast<uint16_t>(o.panelWidthBytes * 8 - (rnd() % 3 == 0 ? range(0, 7) : 0));
    o.panelHeight = static_cast<uint16_t>(range(1, 70));
    o.orientation = static_cast<Old::Orientation>(rnd() % 4);
    o.renderMode = static_cast<Old::RenderMode>(rnd() % 3);
    o.absoluteGrayPlanes = rnd() % 2;
    o.display.inverted = rnd() % 2;
    std::vector<uint8_t> a(o.panelWidthBytes * o.panelHeight), b, sa, sb;
    for (auto& v : a) v = static_cast<uint8_t>(rnd());
    b = a;
    if (rnd() % 3 == 0) {
      o._stripActive = true;
      o._stripY0 = range(0, o.panelHeight - 1);
      o._stripRows = range(1, o.panelHeight - o._stripY0);
      sa.resize(o.panelWidthBytes * o._stripRows);
      for (auto& v : sa) v = static_cast<uint8_t>(rnd());
      sb = sa;
    }
    if (rnd() % 2) {
      o.clipLeft_ = range(-10, 60);
      o.clipTop_ = range(-10, 60);
      o.clipRight_ = range(-10, 120);
      o.clipBottom_ = range(-10, 120);
    }
    o.frameBuffer = a.data();
    o._stripBuf = sa.data();
    bb_gfx_canvas cv{};
    cv.buf = o._stripActive ? sb.data() : b.data();
    cv.rows = o._stripActive ? o._stripRows : o.panelHeight;
    cv.origin_y = o._stripActive ? o._stripY0 : 0;
    cv.stride = o.panelWidthBytes;
    cv.len = static_cast<size_t>(cv.rows) * o.panelWidthBytes;
    cv.panel_width = o.panelWidth;
    cv.panel_height = o.panelHeight;
    cv.clip_left = o.clipLeft_;
    cv.clip_top = o.clipTop_;
    cv.clip_right = o.clipRight_;
    cv.clip_bottom = o.clipBottom_;
    cv.orientation = static_cast<uint8_t>(o.orientation);
    // GfxRenderer only inverts the framebuffer itself, in black and white, on an inverted display.
    const bool inverts = o.renderMode == Old::BW && o.display.inverted && !o._stripActive;

    const int kind = range(0, 3);
    o.offPanel = 0;
    unsigned newOff = 0;
    bool oldOk = true, newOk = true;
    if (kind == 0) {
      // An icon.
      const int size = range(-2, 40);
      std::vector<uint8_t> bits(static_cast<size_t>(std::max(size, 0)) * ((std::max(size, 0) + 7) / 8));
      for (auto& v : bits) v = static_cast<uint8_t>(rnd() | rnd());
      const int x = range(-40, 120), y = range(-40, 120);
      o.drawIcon(bits.data(), x, y, size);
      newOff = bb_gfx_icon(&cv, bits.data(), bits.size(), x, y, size);
    } else if (kind == 1) {
      // The polarity inversion alone.
      const int x = range(-40, 120), y = range(-40, 120), w = range(-5, 120), h = range(-5, 120);
      o.preserveImagePolarity(x, y, w, h);
      if (inverts) bb_gfx_invert_rect(&cv, x, y, w, h);
    } else {
      Bitmap bmp;
      bmp.width = range(1, 70);
      bmp.height = range(1, 70);
      bmp.topDown = rnd() % 2;
      bmp.oneBit = rnd() % 3 == 0;
      bmp.failAt = rnd() % 8 == 0 ? range(0, bmp.height) : -1;
      const size_t rowSize = static_cast<size_t>(bmp.width + 3) / 4;
      bmp.rows.resize(bmp.height);
      for (auto& r : bmp.rows) {
        r.resize(rowSize);
        for (auto& v : r) v = static_cast<uint8_t>(bmp.oneBit ? (rnd() % 2 ? 0xFF : rnd()) : rnd());
      }
      const int x = range(-40, 100), y = range(-40, 100);
      const int maxW = rnd() % 3 == 0 ? 0 : range(-5, 90), maxH = rnd() % 3 == 0 ? 0 : range(-5, 90);
      const float cropX = rnd() % 2 ? 0.0f : static_cast<float>(range(0, 60)) / 100.0f;
      const float cropY = rnd() % 2 ? 0.0f : static_cast<float>(range(0, 60)) / 100.0f;
      const bool transparent = rnd() % 2;
      const bool forceOneBit = rnd() % 8 == 0;
      if (forceOneBit) {
        oldOk = o.drawBitmap1Bit(bmp, x, y, maxW, maxH);
      } else {
        oldOk = o.drawBitmap(bmp, x, y, maxW, maxH, cropX, cropY, transparent);
      }
      // GfxRenderer::drawPicture.
      bmp.next = 0;
      const bool oneBit = forceOneBit || (bmp.oneBit && cropX == 0.0f && cropY == 0.0f);
      bb_gfx_picture p{};
      p.width = bmp.width;
      p.height = bmp.height;
      p.x = x;
      p.y = y;
      p.max_width = maxW;
      p.max_height = maxH;
      // Rust, not the shim, ignores a 1-bit picture's crop and transparency: pass them anyway.
      p.crop_x = cropX;
      p.crop_y = cropY;
      p.top_down = bmp.topDown;
      p.one_bit = oneBit;
      p.white_transparent = transparent;
      p.absolute_gray = o.absoluteGrayPlanes;
      p.plane = static_cast<uint8_t>(o.renderMode);
      std::vector<uint8_t> row(rowSize);
      bb_gfx_drawn drawn{};
      // A short row buffer is refused before anything is read.
      EXPECT_FALSE(bb_gfx_draw_picture(&cv, &p, row.data(), row.size() - 1, readRow, &bmp, &drawn));
      EXPECT_EQ(drawn.failed_row, 0);
      EXPECT_EQ(bmp.next, 0);
      newOk = bb_gfx_draw_picture(&cv, &p, row.data(), row.size(), readRow, &bmp, &drawn);
      if (newOk && inverts) bb_gfx_invert_rect(&cv, x, y, drawn.width, drawn.height);
      // The old code only logged the row it failed on: the one being read.
      if (!newOk) {
        EXPECT_EQ(drawn.failed_row, bmp.next) << "case " << c;
      }
    }
    if (a != b || sa != sb || o.offPanel != newOff || oldOk != newOk) {
      ADD_FAILURE() << "case " << c << " kind " << kind << " orientation " << o.orientation << " strip "
                    << o._stripActive << " mode " << o.renderMode << " ok " << oldOk << "/" << newOk << " off "
                    << o.offPanel << "/" << newOff;
      if (++diffs == 5) return;
    }
  }
}

// Shrunk glyphs (superscripts, subscripts, small caps) and grown ones (drop caps).
TEST(GfxPrimitivesTest, ScaledGlyphsMatchTheOldRenderer) {
  seed = 13;
  unsigned diffs = 0;
  for (int c = 0; c < 6000; ++c) {
    Old o;
    o.panelWidthBytes = static_cast<uint16_t>(range(1, 12));
    o.panelWidth = static_cast<uint16_t>(o.panelWidthBytes * 8 - (rnd() % 3 == 0 ? range(0, 7) : 0));
    o.panelHeight = static_cast<uint16_t>(range(1, 70));
    o.orientation = static_cast<Old::Orientation>(rnd() % 4);
    std::vector<uint8_t> a(o.panelWidthBytes * o.panelHeight), b, sa, sb;
    for (auto& v : a) v = static_cast<uint8_t>(rnd());
    b = a;
    if (rnd() % 3 == 0) {
      o._stripActive = true;
      o._stripY0 = range(0, o.panelHeight - 1);
      o._stripRows = range(1, o.panelHeight - o._stripY0);
      sa.resize(o.panelWidthBytes * o._stripRows);
      for (auto& v : sa) v = static_cast<uint8_t>(rnd());
      sb = sa;
    }
    if (rnd() % 2) {
      o.clipLeft_ = range(-10, 60);
      o.clipTop_ = range(-10, 60);
      o.clipRight_ = range(-10, 120);
      o.clipBottom_ = range(-10, 120);
    }
    o.frameBuffer = a.data();
    o._stripBuf = sa.data();
    bb_gfx_canvas cv{};
    cv.buf = o._stripActive ? sb.data() : b.data();
    cv.rows = o._stripActive ? o._stripRows : o.panelHeight;
    cv.origin_y = o._stripActive ? o._stripY0 : 0;
    cv.stride = o.panelWidthBytes;
    cv.len = static_cast<size_t>(cv.rows) * o.panelWidthBytes;
    cv.panel_width = o.panelWidth;
    cv.panel_height = o.panelHeight;
    cv.clip_left = o.clipLeft_;
    cv.clip_top = o.clipTop_;
    cv.clip_right = o.clipRight_;
    cv.clip_bottom = o.clipBottom_;
    cv.orientation = static_cast<uint8_t>(o.orientation);

    // Mostly small glyphs; one in 50 up to the largest an EpdGlyph holds.
    const int side = c % 50 == 0 ? 255 : 30;
    const OldGlyphMetrics glyph{range(0, side), range(0, side), range(-5, 10), range(-5, 30)};
    const OldFontData font{rnd() % 4 != 0};
    std::vector<uint8_t> bitmap((static_cast<size_t>(glyph.width) * glyph.height * (font.is2Bit ? 2 : 1) + 7) / 8);
    for (auto& v : bitmap) v = static_cast<uint8_t>(rnd() & rnd());
    const int x = range(-30, 100), y = range(-30, 100);
    o.offPanel = 0;
    unsigned newOff = 0;
    if (rnd() % 2) {
      // GfxRenderer's 1/2 (superscript, subscript) and 3/4 (small caps).
      const bool half = rnd() % 2;
      const int num = half ? 1 : 3, den = half ? 2 : 4;
      const bool black = rnd() % 2;
      o.renderCharScaled(bitmap.data(), &glyph, &font, x, y, black, num, den);
      // GfxRenderer's renderCharScaled.
      newOff = bb_gfx_reduced_glyph(&cv, bitmap.data(), bitmap.size(), glyph.width, glyph.height, font.is2Bit, num, den,
                                    x + glyph.left * num / den, y - glyph.top * num / den, black);
    } else {
      // Chapters store 256 to 4096; smaller ones still work.
      const int scale256 = rnd() % 4 == 0 ? range(1, 255) : range(256, glyph.width > 30 ? 600 : 4096);
      o.drawScaledCodepoint(bitmap.data(), &glyph, &font, x, y, scale256);
      // GfxRenderer::drawScaledCodepoint.
      newOff = bb_gfx_resampled_glyph(&cv, bitmap.data(), bitmap.size(), glyph.width, glyph.height, font.is2Bit,
                                      scale256, x + glyph.left * scale256 / 256, y - glyph.top * scale256 / 256);
    }
    if (a != b || sa != sb || o.offPanel != newOff) {
      ADD_FAILURE() << "case " << c << " orientation " << o.orientation << " strip " << o._stripActive << " off "
                    << o.offPanel << "/" << newOff;
      if (++diffs == 5) return;
    }
  }
}
