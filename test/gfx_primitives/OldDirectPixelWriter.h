// lib/Epub/Epub/converters/DirectPixelWriter.h's DirectPixelWriter at c7580634, verbatim but for the
// namespace, with just enough of GfxRenderer for it: the oracle for bembeyaz-gfx's level rows.
#pragma once

#include <BitmapHelpers.h>

#include <cassert>
#include <cstdint>

namespace oldLevels {

struct GfxRenderer {
  enum Orientation { Portrait, LandscapeClockwise, PortraitInverted, LandscapeCounterClockwise };
  enum RenderMode { BW, GRAYSCALE_LSB, GRAYSCALE_MSB };
  uint8_t* target;
  int originY, rows, widthBytes, width, height;
  Orientation orientation;
  RenderMode mode;
  bool absolute;
  uint8_t* getWriteTarget() const { return target; }
  int getWriteOriginY() const { return originY; }
  int getWriteRows() const { return rows; }
  RenderMode getRenderMode() const { return mode; }
  bool grayPlanesAreAbsolute() const { return absolute; }
  int getDisplayWidthBytes() const { return widthBytes; }
  int getDisplayWidth() const { return width; }
  int getDisplayHeight() const { return height; }
  Orientation getOrientation() const { return orientation; }
};

// Direct framebuffer writer that eliminates per-pixel overhead from the image
// rendering hot path.  Pre-computes orientation transform as linear coefficients
// and caches render-mode state so the inner loop is: one multiply, one add,
// one shift, and one AND per pixel — no branches, no method calls.
//
// Caller is responsible for ensuring (outX, outY) are within screen bounds.
// ImageBlock::render() already validates this before entering the pixel loop,
// and the JPEG/PNG callbacks pre-clamp destination ranges to screen bounds.
struct DirectPixelWriter {
  uint8_t* fb;
  GfxRenderer::RenderMode mode;
  bool absolute = false;
  uint16_t displayWidthBytes;  // Runtime framebuffer stride (X4: 100, X3: 99)
  // Active write target: for tiled grayscale, fb is the band scratch, originY is
  // the band's top physical row, and clipRows is the band height. Off-band
  // pixels are dropped. With no strip active these collapse to the full frame
  // (originY 0, clipRows panelHeight) so the clip doubles as a bounds guard.
  int originY;
  int clipRows;

  // Orientation is collapsed into a linear transform:
  //   phyX = phyXBase + x * phyXStepX + y * phyXStepY
  //   phyY = phyYBase + x * phyYStepX + y * phyYStepY
  int phyXBase, phyYBase;
  int phyXStepX, phyYStepX;  // per logical-X step
  int phyXStepY, phyYStepY;  // per logical-Y step

  // Row-precomputed: the Y-dependent portion of the physical coords
  int rowPhyXBase, rowPhyYBase;

  void init(GfxRenderer& renderer) {
    fb = renderer.getWriteTarget();
    originY = renderer.getWriteOriginY();
    clipRows = renderer.getWriteRows();
    mode = renderer.getRenderMode();
    absolute = renderer.grayPlanesAreAbsolute();
    displayWidthBytes = renderer.getDisplayWidthBytes();

    const int phyW = renderer.getDisplayWidth();
    const int phyH = renderer.getDisplayHeight();

    switch (renderer.getOrientation()) {
      case GfxRenderer::Portrait:
        // phyX = y, phyY = (phyH-1) - x
        phyXBase = 0;
        phyYBase = phyH - 1;
        phyXStepX = 0;
        phyYStepX = -1;
        phyXStepY = 1;
        phyYStepY = 0;
        break;
      case GfxRenderer::LandscapeClockwise:
        // phyX = (phyW-1) - x, phyY = (phyH-1) - y
        phyXBase = phyW - 1;
        phyYBase = phyH - 1;
        phyXStepX = -1;
        phyYStepX = 0;
        phyXStepY = 0;
        phyYStepY = -1;
        break;
      case GfxRenderer::PortraitInverted:
        // phyX = (phyW-1) - y, phyY = x
        phyXBase = phyW - 1;
        phyYBase = 0;
        phyXStepX = 0;
        phyYStepX = 1;
        phyXStepY = -1;
        phyYStepY = 0;
        break;
      case GfxRenderer::LandscapeCounterClockwise:
        // phyX = x, phyY = y
        phyXBase = 0;
        phyYBase = 0;
        phyXStepX = 1;
        phyYStepX = 0;
        phyXStepY = 0;
        phyYStepY = 1;
        break;
      default:
        // Fallback to LandscapeCounterClockwise (identity transform)
        phyXBase = 0;
        phyYBase = 0;
        phyXStepX = 1;
        phyYStepX = 0;
        phyXStepY = 0;
        phyYStepY = 1;
        break;
    }
  }

  // Call once per row before the column loop.
  // Pre-computes the Y-dependent portion so writePixel() only needs the X part.
  inline void beginRow(int logicalY) {
    rowPhyXBase = phyXBase + logicalY * phyXStepY;
    rowPhyYBase = phyYBase + logicalY * phyYStepY;
  }

  // For the current row (set via beginRow), narrow [colStart, colEnd) to the
  // columns whose pixels fall inside the active strip band. writePixel() would
  // clip the rest anyway, but on a strip pass that is most of a full-page image
  // (only ~one strip-height worth of columns survive in portrait); skipping them
  // here avoids the per-pixel unpack+transform entirely. For full-frame passes
  // (clipRows == panel height) the range is unchanged. xBase is the logical X of
  // column 0; the band test mirrors writePixel(): 0 <= phyY - originY < clipRows.
  inline void bandColRange(int xBase, int width, int& colStart, int& colEnd) const {
    // init() only ever sets phyYStepX to 0, +1, or -1; the +1/-1 solve below
    // relies on that.
    assert(phyYStepX == 0 || phyYStepX == 1 || phyYStepX == -1);
    colStart = 0;
    colEnd = width;
    if (phyYStepX == 0) {
      // phyY is constant across the row: the whole row is in-band or out.
      const int sy = rowPhyYBase - originY;
      if (static_cast<unsigned>(sy) >= static_cast<unsigned>(clipRows)) colEnd = 0;
      return;
    }
    // phyY = rowPhyYBase + logicalX * phyYStepX (phyYStepX is +1 or -1).
    // Solve originY <= phyY <= originY + clipRows - 1 for logicalX.
    const int loY = originY;
    const int hiY = originY + clipRows - 1;
    int xLo, xHi;
    if (phyYStepX > 0) {
      xLo = loY - rowPhyYBase;
      xHi = hiY - rowPhyYBase;
    } else {
      xLo = rowPhyYBase - hiY;
      xHi = rowPhyYBase - loY;
    }
    const int cs = xLo - xBase;
    const int ce = xHi - xBase + 1;  // exclusive
    if (cs > colStart) colStart = cs;
    if (ce < colEnd) colEnd = ce;
    if (colStart < 0) colStart = 0;
    if (colEnd > width) colEnd = width;
    if (colStart > colEnd) colStart = colEnd;
  }

  // Write a single 2-bit dithered pixel value to the framebuffer.
  // Must be called after beginRow() for the current row.
  // No bounds checking — caller guarantees coordinates are valid.
  inline void writePixel(int logicalX, uint8_t pixelValue, bool writeWhiteInBw = false) const {
    // Determine whether to draw based on render mode
    bool draw;
    bool state;
    switch (mode) {
      case GfxRenderer::BW:
        draw = writeWhiteInBw || pixelValue < 3;
        state = pixelValue < 3;
        break;
      case GfxRenderer::GRAYSCALE_MSB:
      case GfxRenderer::GRAYSCALE_LSB: {
        const auto pixel = grayPlanePixel(pixelValue, mode == GfxRenderer::GRAYSCALE_MSB, absolute);
        draw = pixel.write;
        state = pixel.black;
        break;
      }
      default:
        return;
    }

    if (!draw) return;

    const int phyX = rowPhyXBase + logicalX * phyXStepX;
    const int phyY = rowPhyYBase + logicalX * phyYStepX;

    // Band-local row. The unsigned compare drops both off-band pixels (strip
    // mode) and any out-of-frame row (full-frame mode) in one branch.
    const int sy = phyY - originY;
    if (static_cast<unsigned>(sy) >= static_cast<unsigned>(clipRows)) return;

    const uint16_t byteIndex = static_cast<uint16_t>(sy * displayWidthBytes + (phyX >> 3));
    const uint8_t bitMask = 1 << (7 - (phyX & 7));

    if (state) {
      fb[byteIndex] &= ~bitMask;  // Clear bit (draw black)
    } else {
      fb[byteIndex] |= bitMask;  // Set bit (draw white)
    }
  }
};

// lib/Epub/Epub/blocks/ImageBlock.cpp's row loop (renderRowsFromPxcSlot) at c7580634, with
// writePixel's whiteInBw passed through (the PNG converter sets it).
inline void drawRow(GfxRenderer& renderer, const uint8_t* rowBuffer, int width, int x, int y, bool whiteInBw) {
  DirectPixelWriter pw;
  pw.init(renderer);
  pw.beginRow(y);
  int colStart, colEnd;
  pw.bandColRange(x, width, colStart, colEnd);
  for (int col = colStart; col < colEnd; col++) {
    const int byteIdx = col >> 2;            // col / 4
    const int bitShift = 6 - (col & 3) * 2;  // MSB first within byte
    const uint8_t pixelValue = (rowBuffer[byteIdx] >> bitShift) & 0x03;
    pw.writePixel(x + col, pixelValue, whiteInBw);
  }
}

}  // namespace oldLevels
