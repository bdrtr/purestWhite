// bembeyaz-gfx's glyph drawing (bb_gfx_glyph) against a per-pixel reference, and against the C++
// rasterizer it replaced (OldGlyphBitmap.h, with GfxRenderer::drawGlyphBitmap's setup below).
#include <bembeyaz/gfx.h>
#include <gtest/gtest.h>

#include <array>
#include <cstdint>
#include <utility>
#include <vector>

#include "OldGlyphBitmap.h"

namespace {
constexpr int PANEL_WIDTH = 40;
constexpr int PANEL_HEIGHT = 32;
constexpr int STRIDE = PANEL_WIDTH / 8;

enum class Plane : uint8_t { BW = BB_GFX_PLANE_BW, GrayLSB = BB_GFX_PLANE_GRAY_LSB, GrayMSB = BB_GFX_PLANE_GRAY_MSB };

// Logical placements used by the renderer: unrotated text, and side-button
// labels rotated 90 degrees clockwise (glyph x runs up, glyph y runs right).
constexpr std::array<std::array<int, 4>, 2> AXES = {{{1, 0, 0, 1}, {0, -1, 1, 0}}};

std::pair<int, int> physical(int orientation, int x, int y) {
  switch (orientation) {
    case 0:
      return {y, PANEL_HEIGHT - 1 - x};
    case 1:
      return {PANEL_WIDTH - 1 - x, PANEL_HEIGHT - 1 - y};
    case 2:
      return {PANEL_WIDTH - 1 - y, x};
    default:
      return {x, y};
  }
}

// Logical rectangle the renderer clips to; the glyph-local clip is derived from it.
struct Rect {
  int left;
  int top;
  int right;
  int bottom;
};

void compare(int orientation, int rotation, bool twoBit, Plane plane, bool state, int width, int height, int x, int y,
             int originY, int rows, Rect clip) {
  SCOPED_TRACE(::testing::Message() << orientation << ',' << rotation << ',' << twoBit << ',' << static_cast<int>(plane)
                                    << ',' << state << " size=" << width << 'x' << height << " at=" << x << ',' << y
                                    << " band=" << originY << ',' << rows);
  const auto [dxX, dxY, dyX, dyY] = AXES[rotation];
  const bb_gfx_frame logical{x, y, dxX, dxY, dyX, dyY};
  std::vector<uint8_t> bitmap((width * height * (twoBit ? 2 : 1) + 7) / 8);
  for (size_t i = 0; i < bitmap.size(); ++i) bitmap[i] = static_cast<uint8_t>(i * 73 + 0x1b);
  // Nonuniform background and guards verify transparent pixels and all off-band bytes.
  std::vector<uint8_t> expected(rows * STRIDE + 32);
  for (size_t i = 0; i < expected.size(); ++i) expected[i] = static_cast<uint8_t>(i * 53 + 0xa5);
  auto actual = expected;
  for (int gy = 0; gy < height; ++gy) {
    for (int gx = 0; gx < width; ++gx) {
      const int lx = x + gx * dxX + gy * dyX;
      const int ly = y + gx * dxY + gy * dyY;
      if (lx < clip.left || ly < clip.top || lx >= clip.right || ly >= clip.bottom) continue;
      const auto [px, py] = physical(orientation, lx, ly);
      if (px < 0 || px >= PANEL_WIDTH || py < originY || py >= originY + rows) continue;
      const int source = gy * width + gx;
      bool draw = false;
      bool black = state;
      if (twoBit) {
        const int value = 3 - ((bitmap[source / 4] >> ((3 - source % 4) * 2)) & 3);
        if (plane == Plane::BW) draw = value < 3;
        if (plane == Plane::GrayLSB) draw = value == 1;
        if (plane == Plane::GrayMSB) draw = value == 1 || value == 2;
        if (plane != Plane::BW) black = false;
      } else {
        draw = (bitmap[source / 8] >> (7 - source % 8)) & 1;
      }
      if (!draw) continue;
      auto& byte = expected[16 + (py - originY) * STRIDE + px / 8];
      if (black)
        byte &= static_cast<uint8_t>(~(0x80 >> (px % 8)));
      else
        byte |= 0x80 >> (px % 8);
    }
  }

  bb_gfx_canvas canvas{};
  canvas.buf = actual.data() + 16;
  canvas.len = static_cast<size_t>(rows) * STRIDE;
  canvas.stride = STRIDE;
  canvas.origin_y = originY;
  canvas.rows = rows;
  canvas.panel_width = PANEL_WIDTH;
  canvas.panel_height = PANEL_HEIGHT;
  canvas.clip_left = clip.left;
  canvas.clip_top = clip.top;
  canvas.clip_right = clip.right;
  canvas.clip_bottom = clip.bottom;
  // The test's orientations are bb_gfx_canvas's in order.
  static_assert(BB_GFX_PORTRAIT == 0 && BB_GFX_LANDSCAPE_CLOCKWISE == 1 && BB_GFX_PORTRAIT_INVERTED == 2 &&
                    BB_GFX_LANDSCAPE_COUNTER_CLOCKWISE == 3,
                "orientation order");
  canvas.orientation = static_cast<uint8_t>(orientation);
  bb_gfx_glyph(&canvas, bitmap.data(), bitmap.size(), width, height, &logical, twoBit, static_cast<uint8_t>(plane),
               state);
  EXPECT_EQ(expected, actual);
}
}  // namespace

TEST(GlyphBitmap, MatchesPerPixelReferenceAcrossOrientationsRotationsPlanesAndClipping) {
  for (int orientation = 0; orientation < 4; ++orientation) {
    for (int rotation = 0; rotation < 2; ++rotation) {
      for (bool twoBit : {false, true}) {
        for (Plane plane : {Plane::BW, Plane::GrayLSB, Plane::GrayMSB}) {
          for (bool state : {false, true}) {
            for (int width : {1, 3, 7, 16, 31}) {
              for (const auto [x, y] : {std::pair{-5, -3}, std::pair{0, 0}, std::pair{9, 11}, std::pair{29, 31}}) {
                for (const auto [origin, rows] :
                     {std::pair{0, 32}, std::pair{0, 7}, std::pair{7, 11}, std::pair{28, 4}}) {
                  compare(orientation, rotation, twoBit, plane, state, width, 13, x, y, origin, rows,
                          {-20, -20, 60, 60});
                  compare(orientation, rotation, twoBit, plane, state, width, 13, x, y, origin, rows, {2, 3, 18, 20});
                }
              }
            }
          }
        }
      }
    }
  }
}

TEST(GlyphBitmap, EmptyAndFullyClippedGlyphsDoNotWrite) {
  for (int orientation = 0; orientation < 4; ++orientation) {
    for (int rotation = 0; rotation < 2; ++rotation) {
      compare(orientation, rotation, true, Plane::BW, true, 0, 0, 0, 0, 0, 32, {0, 0, 40, 32});
      compare(orientation, rotation, true, Plane::BW, true, 11, 9, 100, 100, 0, 32, {0, 0, 40, 32});
      compare(orientation, rotation, true, Plane::BW, true, 11, 9, 0, 0, 0, 32, {12, 10, 15, 20});
    }
  }
}

namespace {
// GfxRenderer's rotateCoordinates and drawGlyphBitmap at 64b27c5b, on oldGlyphBitmap.
struct OldRenderer {
  int orientation;
  int panelWidth, panelHeight, panelWidthBytes;
  uint8_t* target;
  int originY, rows;
  int clipLeft_, clipTop_, clipRight_, clipBottom_;

  void rotateCoordinates(const int x, const int y, int* phyX, int* phyY) const {
    switch (orientation) {
      case 0:
        *phyX = y;
        *phyY = panelHeight - 1 - x;
        break;
      case 1:
        *phyX = panelWidth - 1 - x;
        *phyY = panelHeight - 1 - y;
        break;
      case 2:
        *phyX = panelWidth - 1 - y;
        *phyY = x;
        break;
      default:
        *phyX = x;
        *phyY = y;
    }
  }

  void drawGlyphBitmap(const uint8_t* bitmap, const int width, const int height, const oldGlyphBitmap::Frame& frame,
                       const bool twoBit, const oldGlyphBitmap::Plane plane, const bool state) const {
    oldGlyphBitmap::Clip clip{0, 0, width, height};
    oldGlyphBitmap::clipToRect(frame, clipLeft_, clipTop_, clipRight_, clipBottom_, clip);
    oldGlyphBitmap::Target t{target, panelWidth, panelWidthBytes, originY, rows, {}};
    oldGlyphBitmap::Frame& physical = t.frame;
    rotateCoordinates(frame.x, frame.y, &physical.x, &physical.y);
    int nextX, nextY;
    rotateCoordinates(frame.x + frame.dxX, frame.y + frame.dxY, &nextX, &nextY);
    physical.dxX = nextX - physical.x;
    physical.dxY = nextY - physical.y;
    rotateCoordinates(frame.x + frame.dyX, frame.y + frame.dyY, &nextX, &nextY);
    physical.dyX = nextX - physical.x;
    physical.dyY = nextY - physical.y;
    oldGlyphBitmap::draw(bitmap, width, height, twoBit, plane, state, t, clip);
  }
};

uint32_t seed = 1;
uint32_t rnd() {
  seed ^= seed << 13;
  seed ^= seed >> 17;
  seed ^= seed << 5;
  return seed;
}
int range(int lo, int hi) { return lo + static_cast<int>(rnd() % static_cast<uint32_t>(hi - lo + 1)); }
}  // namespace

// Random panels (some widths not a multiple of 8), all eight orthogonal glyph frames, clips, strips,
// sizes and positions: the same bytes as the old rasterizer.
TEST(GlyphBitmap, MatchesTheOldRasterizer) {
  constexpr std::array<std::array<int, 4>, 8> FRAMES = {{{1, 0, 0, 1},
                                                         {0, -1, 1, 0},
                                                         {-1, 0, 0, -1},
                                                         {0, 1, -1, 0},
                                                         {-1, 0, 0, 1},
                                                         {1, 0, 0, -1},
                                                         {0, 1, 1, 0},
                                                         {0, -1, -1, 0}}};
  unsigned diffs = 0;
  for (int c = 0; c < 20000; ++c) {
    OldRenderer o{};
    o.orientation = range(0, 3);
    o.panelWidthBytes = range(1, 10);
    o.panelWidth = o.panelWidthBytes * 8 - (rnd() % 3 == 0 ? range(0, 7) : 0);
    o.panelHeight = range(1, 60);
    o.originY = 0;
    o.rows = o.panelHeight;
    if (rnd() % 3 == 0) {
      o.originY = range(0, o.panelHeight - 1);
      o.rows = range(1, o.panelHeight - o.originY);
    }
    o.clipLeft_ = 0;
    o.clipTop_ = 0;
    o.clipRight_ = 32767;
    o.clipBottom_ = 32767;
    if (rnd() % 2) {
      o.clipLeft_ = range(-10, 50);
      o.clipTop_ = range(-10, 50);
      o.clipRight_ = range(-10, 100);
      o.clipBottom_ = range(-10, 100);
    }
    std::vector<uint8_t> a(static_cast<size_t>(o.rows) * o.panelWidthBytes);
    for (auto& v : a) v = static_cast<uint8_t>(rnd());
    auto b = a;
    o.target = a.data();

    const int width = range(0, 40), height = range(0, 40);
    const bool twoBit = rnd() % 2;
    std::vector<uint8_t> bitmap((static_cast<size_t>(width) * height * (twoBit ? 2 : 1) + 7) / 8);
    for (auto& v : bitmap) v = static_cast<uint8_t>(rnd());
    const auto& f = FRAMES[rnd() % 8];
    const int x = range(-50, 120), y = range(-50, 120);
    const int plane = range(0, 2);
    const bool state = rnd() % 2;
    if (width > 0 && height > 0) {
      o.drawGlyphBitmap(bitmap.data(), width, height, {x, y, f[0], f[1], f[2], f[3]}, twoBit,
                        static_cast<oldGlyphBitmap::Plane>(plane), state);
    }

    bb_gfx_canvas canvas{};
    canvas.buf = b.data();
    canvas.len = b.size();
    canvas.stride = static_cast<size_t>(o.panelWidthBytes);
    canvas.origin_y = o.originY;
    canvas.rows = o.rows;
    canvas.panel_width = o.panelWidth;
    canvas.panel_height = o.panelHeight;
    canvas.clip_left = o.clipLeft_;
    canvas.clip_top = o.clipTop_;
    canvas.clip_right = o.clipRight_;
    canvas.clip_bottom = o.clipBottom_;
    canvas.orientation = static_cast<uint8_t>(o.orientation);
    const bb_gfx_frame frame{x, y, f[0], f[1], f[2], f[3]};
    bb_gfx_glyph(&canvas, bitmap.data(), bitmap.size(), width, height, &frame, twoBit, static_cast<uint8_t>(plane),
                 state);
    if (a != b) {
      ADD_FAILURE() << "case " << c << " orientation " << o.orientation << " frame " << f[0] << f[1] << f[2] << f[3]
                    << " size " << width << "x" << height << " twoBit " << twoBit << " plane " << plane;
      if (++diffs == 5) return;
    }
  }
}
