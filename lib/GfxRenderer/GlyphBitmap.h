#pragma once

// Glyph placement for GfxRenderer::drawGlyphBitmap; bembeyaz-gfx (glyph.rs) draws the glyph.
namespace glyphBitmap {

// Position of glyph pixel (0, 0) plus the step for one move along the glyph's
// x and y axes, all in logical coordinates. The axes must describe an
// orthogonal unit rotation: each is +/-1 on exactly one component.
struct Frame {
  int x;
  int y;
  int dxX;
  int dxY;
  int dyX;
  int dyY;
};

}  // namespace glyphBitmap
