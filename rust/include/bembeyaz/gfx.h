// C ABI of rust/crates/bembeyaz-ffi/src/gfx.rs. Use lib/GfxRenderer/GfxRenderer.h instead.
#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// GfxRenderer's orientations: how logical coordinates map onto the panel.
enum {
  BB_GFX_PORTRAIT = 0,                     // rotated 90° clockwise
  BB_GFX_LANDSCAPE_CLOCKWISE = 1,          // rotated 180°
  BB_GFX_PORTRAIT_INVERTED = 2,            // rotated 90° counter-clockwise
  BB_GFX_LANDSCAPE_COUNTER_CLOCKWISE = 3,  // the panel's own layout
};

// The panel rows being drawn into: rows rows of stride bytes in buf (len bytes), the first being
// panel row origin_y; one bit per pixel, most significant first, 0 for black. Callers draw in
// logical coordinates, which orientation maps onto the panel, and only inside the clip (left and
// top included, right and bottom not).
typedef struct {
  uint8_t* buf;
  size_t len;
  size_t stride;
  int32_t origin_y;
  int32_t rows;
  int32_t panel_width;
  int32_t panel_height;
  int32_t clip_left;
  int32_t clip_top;
  int32_t clip_right;
  int32_t clip_bottom;
  uint8_t orientation;  // a BB_GFX_* orientation
} bb_gfx_canvas;

// width lines from (x1, y1) to (x2, y2), each one pixel below the last, ends included. Returns how
// many pixels fell off the panel.
uint32_t bb_gfx_line(const bb_gfx_canvas* canvas, int32_t x1, int32_t y1, int32_t x2, int32_t y2, int32_t width,
                     bool black);
// A rectangle's outline, width lines thick, inside it. Returns how many pixels fell off the panel.
uint32_t bb_gfx_rect(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, int32_t width,
                     bool black);
// The BB_GFX_* colors: GfxRenderer's Color values. The calls below draw nothing for any other value
// (such as Color::Clear).
enum {
  BB_GFX_WHITE = 0x01,
  BB_GFX_LIGHT_GRAY = 0x05,  // black where logical x and y are both even
  BB_GFX_DARK_GRAY = 0x0A,   // black where logical x + y is even
  BB_GFX_BLACK = 0x10,
};

// Fills a rectangle with a BB_GFX_* color (anything else draws nothing), clipped to the screen,
// the clip and the rows.
void bb_gfx_fill_rect(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, uint8_t color);

// A quarter of the ring between the circles of radius and radius - width around (cx, cy), on the
// side x_dir and y_dir point to (1 or -1 each); a width of at least the radius fills it.
void bb_gfx_arc(const bb_gfx_canvas* canvas, int32_t radius, int32_t cx, int32_t cy, int32_t x_dir, int32_t y_dir,
                int32_t width, bool black);

// The corners of a rounded rectangle that are round: any of these or'ed together.
enum {
  BB_GFX_TOP_LEFT = 1,
  BB_GFX_TOP_RIGHT = 2,
  BB_GFX_BOTTOM_LEFT = 4,
  BB_GFX_BOTTOM_RIGHT = 8,
};

// A rectangle's outline, width lines thick, inside it, with the round corners rounded to radius
// (at most half the rectangle); sides between two square corners are left out. Returns how many
// pixels fell off the panel, which only a radius of zero (a plain outline) counts.
uint32_t bb_gfx_rounded_rect(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, int32_t width,
                             int32_t radius, uint8_t round, bool black);
// Fills a rectangle with a BB_GFX_* color, the round corners rounded to radius: at most half the
// rectangle when all four are round, at most all of it otherwise.
void bb_gfx_fill_rounded_rect(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, int32_t radius,
                              uint8_t round, uint8_t color);
// Paints a BB_GFX_* color over the pixels of the rectangle's radius by radius corners that lie
// outside the circle of radius - 1 around each corner's inner pixel, so that what is under them
// looks rounded. Returns how many pixels fell off the panel.
uint32_t bb_gfx_mask_corners(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, int32_t radius,
                             uint8_t color);

// Fills the polygon with the count corners (xs[i], ys[i]) black or white (nothing for fewer than
// three, or NULL arrays), clipped to the screen, the clip and the rows: on each row, between the
// first and second edge crossing, the third and fourth, and so on. Each row goes over the edges
// once per crossing, so keep count small. The points are int, not int32_t (long on Xtensa), so
// that GfxRenderer's int arrays pass as they are.
void bb_gfx_fill_polygon(const bb_gfx_canvas* canvas, const int* xs, const int* ys, size_t count, bool black);

// The planes a picture draw writes: GfxRenderer's RenderMode values.
enum {
  BB_GFX_PLANE_BW = 0,
  BB_GFX_PLANE_GRAY_LSB = 1,
  BB_GFX_PLANE_GRAY_MSB = 2,
};

// A picture read row by row (2 bits per pixel, most significant first, 0 black to 3 white) and
// where it goes: its top left corner at (x, y), shrunk (never grown) to fit max_width by max_height
// (0 or less for no bound), crop_x and crop_y of its width and height cut off, half on each side.
typedef struct {
  int32_t width;
  int32_t height;
  int32_t x;
  int32_t y;
  int32_t max_width;
  int32_t max_height;
  float crop_x;
  float crop_y;
  bool top_down;           // rows come top first, else bottom first
  bool one_bit;            // drawn as 1-bit files are: black where not white in every plane, never cropped
  bool white_transparent;  // leave white pixels as they are
  bool absolute_gray;      // gray planes hold absolute levels, both inks written
  uint8_t plane;           // a BB_GFX_PLANE_*
} bb_gfx_picture;

// What bb_gfx_draw_picture drew: pixels cropped off each side, the scale (1 when not shrunk), the size
// drawn, and the row that could not be read (-1 when all were).
typedef struct {
  int32_t crop_x;
  int32_t crop_y;
  float scale;
  int32_t width;
  int32_t height;
  int32_t failed_row;
} bb_gfx_drawn;

// Draws a picture whose rows read() fills, one call per row, into row (row_len bytes, at least
// (width + 3) / 4, or nothing is drawn and row 0 counts as failed), from the first row of the file
// to the last one needed: columns off the screen are skipped, and the picture ends at the first row
// below the screen (one_bit pictures read on to the end). read() must not draw. Returns false, with
// drawn->failed_row set, when read() fails; the rows before it stay drawn.
bool bb_gfx_draw_picture(const bb_gfx_canvas* canvas, const bb_gfx_picture* picture, uint8_t* row, size_t row_len,
                         bool (*read)(void* ctx, uint8_t* row), void* ctx, bb_gfx_drawn* drawn);
// The ink of a square 1-bit icon of size pixels (len bytes of rows of (size + 7) / 8 bytes, most
// significant bit first, 0 for ink), its rows running right to left and its columns down from
// (x, y). Returns how many pixels fell off the panel.
uint32_t bb_gfx_icon(const bb_gfx_canvas* canvas, const uint8_t* bits, size_t len, int32_t x, int32_t y, int32_t size);
// Inverts every pixel of a rectangle, clipped to the screen, the clip and the rows.
void bb_gfx_invert_rect(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h);

// Where glyph pixel (0, 0) lands, in logical coordinates, and the step for one move along the
// glyph's x and y axes: an orthogonal unit rotation, each axis +/-1 on exactly one component and
// the two on different ones (bb_gfx_glyph draws nothing for any other frame).
typedef struct {
  int32_t x;
  int32_t y;
  int32_t dx_x;
  int32_t dx_y;
  int32_t dy_x;
  int32_t dy_y;
} bb_gfx_frame;

// Draws a packed glyph of width by height pixels (len bytes: rows back to back, most significant
// bit first, 1 or 2 bits per pixel) placed by frame, clipped to the clip, the panel and the rows.
// 2-bit values are 0 white, 1 light gray, 2 dark gray, 3 black. In BB_GFX_PLANE_BW every non-white
// value is drawn black or white as black says; the gray planes only set bits (light and dark gray
// for MSB, dark gray for LSB). 1-bit glyphs draw ink (value 1) as black says in every plane.
void bb_gfx_glyph(const bb_gfx_canvas* canvas, const uint8_t* bits, size_t len, int32_t width, int32_t height,
                  const bb_gfx_frame* frame, bool two_bit, uint8_t plane, bool black);

// A glyph bitmap as bb_gfx_glyph takes it, shrunk by num / den (meant for at most 1) and drawn black or white
// pixel by pixel with its top left at (x, y): each pixel covers a block of source pixels; at one
// half any ink there draws (2-bit: dark gray or more, or two light pixels), milder reductions need
// majority coverage. Returns how many pixels fell off the panel; negative sizes, sizes over 4096 and
// factors outside 1 to 4096 draw nothing.
uint32_t bb_gfx_reduced_glyph(const bb_gfx_canvas* canvas, const uint8_t* bits, size_t len, int32_t width,
                              int32_t height, bool two_bit, int32_t num, int32_t den, int32_t x, int32_t y, bool black);
// A glyph bitmap scaled by scale256 / 256 and drawn black with its top left at (x, y): each pixel's
// centre maps back onto the glyph and is inked where the bilinear coverage of its four neighbours
// reaches half of black. Returns how many pixels fell off the panel; sizes over 4096, or a scale256
// under 1 or over 4096 (16 times, the most a chapter stores), draw nothing.
uint32_t bb_gfx_resampled_glyph(const bb_gfx_canvas* canvas, const uint8_t* bits, size_t len, int32_t width,
                                int32_t height, bool two_bit, int32_t scale256, int32_t x, int32_t y);

// Draws the first width 2-bit gray levels of a row (len bytes, four levels to a byte, most significant
// first, 0 black to 3 white) along logical row y from column x, kept to the panel and the rows (the
// clip does not apply). With a mask (mask_len bytes, a bit a column, most significant first) only the
// columns whose bit is set are drawn. In BB_GFX_PLANE_BW levels 0 to 2 draw black and 3 draws white
// with a mask and is left as it is without one; the gray planes follow grayPlanePixel (absolute or as
// differences).
void bb_gfx_level_row(const bb_gfx_canvas* canvas, const uint8_t* bits, size_t len, const uint8_t* mask,
                      size_t mask_len, int32_t width, int32_t x, int32_t y, uint8_t plane, bool absolute);

// Regions: the whole bytes of the rows that a logical rectangle (x, y, w, h) covers on the panel,
// kept on the panel (the clip does not apply). aligned snaps the panel x range out to multiples of
// 8 pixels first and leaves out a last partial byte the panel's width cuts off (the panel's window
// alignment); otherwise every byte the kept range touches counts. Copies are the rows one after
// another.
//
// The bytes a copy of the region takes; 0 when it is empty or off the panel.
size_t bb_gfx_region_size(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, bool aligned);
// Copies the region into out; returns the bytes copied, 0 (copying nothing) when the region is
// empty, off the panel, not all in the canvas's rows, or larger than capacity.
size_t bb_gfx_read_region(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, bool aligned,
                          uint8_t* out, size_t capacity);
// Copies src (len bytes) into the region; false, copying nothing, when the region is empty, off the
// panel, not all in the canvas's rows, or larger than len.
bool bb_gfx_write_region(const bb_gfx_canvas* canvas, int32_t x, int32_t y, int32_t w, int32_t h, bool aligned,
                         const uint8_t* src, size_t len);

#ifdef __cplusplus
}
#endif
