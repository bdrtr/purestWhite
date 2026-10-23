// bembeyaz-gfx against the drawing code GfxRenderer had before it (lib/GfxRenderer/GfxRenderer.cpp
// up to 87481ce5, copied below as Old): random panels, orientations, strips, clips and shapes must
// give the same pixels, and the same count of pixels off the panel.
#include <bembeyaz/gfx.h>
#include <gtest/gtest.h>

#include <algorithm>
#include <cstdint>
#include <cstring>
#include <utility>
#include <vector>

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
  void drawPixel(int x, int y, bool state) const;
  void drawLine(int x1, int y1, int x2, int y2, bool state) const;
  void drawLine(int x1, int y1, int x2, int y2, int lineWidth, bool state) const;
  void drawRect(int x, int y, int width, int height, bool state) const;
  void drawRect(int x, int y, int width, int height, int lineWidth, bool state) const;
  template <Color C>
  void fillRectImpl(int x, int y, int width, int height) const;
  int getScreenWidth() const;
  int getScreenHeight() const;
};
using Orientation = Old::Orientation;
constexpr auto Portrait = Old::Portrait;
constexpr auto LandscapeClockwise = Old::LandscapeClockwise;
constexpr auto PortraitInverted = Old::PortraitInverted;
constexpr auto LandscapeCounterClockwise = Old::LandscapeCounterClockwise;

// ---- Old: verbatim, except that off-panel pixels are counted rather than logged. ----

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
      const int kind = range(0, 5);
      const int x1 = range(-40, 140), y1 = range(-40, 140), x2 = range(-40, 140), y2 = range(-40, 140);
      const int w = range(-5, 120), h = range(-5, 120), lw = range(-1, 6);
      const bool black = rnd() % 2;
      const Color colors[] = {Clear, White, LightGray, DarkGray, Black};
      const Color color = colors[rnd() % 5];
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

// GfxRenderer still paints dithered rounded-rect corners pixel by pixel (drawPixelDither) next to
// bodies filled here: both must use the same pattern.
TEST(GfxPrimitivesTest, OnePixelDitherFillsMatchThePixelRule) {
  for (int orientation = 0; orientation < 4; ++orientation) {
    for (const Color color : {LightGray, DarkGray}) {
      Old o;
      o.orientation = static_cast<Old::Orientation>(orientation);
      o.panelWidthBytes = 3;
      o.panelWidth = 24;
      o.panelHeight = 13;
      std::vector<uint8_t> a(o.panelWidthBytes * o.panelHeight, 0x5A), b = a;
      o.frameBuffer = a.data();
      bb_gfx_canvas cv{};
      cv.buf = b.data();
      cv.len = b.size();
      cv.stride = o.panelWidthBytes;
      cv.rows = o.panelHeight;
      cv.panel_width = o.panelWidth;
      cv.panel_height = o.panelHeight;
      cv.clip_right = 32767;
      cv.clip_bottom = 32767;
      cv.orientation = static_cast<uint8_t>(orientation);
      for (int y = 0; y < o.getScreenHeight(); ++y) {
        for (int x = 0; x < o.getScreenWidth(); ++x) {
          // GfxRenderer::drawPixelDither's rule.
          o.drawPixel(x, y, color == LightGray ? x % 2 == 0 && y % 2 == 0 : (x + y) % 2 == 0);
          bb_gfx_fill_rect(&cv, x, y, 1, 1, color);
        }
      }
      EXPECT_EQ(a, b) << "orientation " << orientation << " color " << static_cast<int>(color);
    }
  }
}
