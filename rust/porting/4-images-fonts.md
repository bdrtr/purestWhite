# Stage 4: images and fonts

Stage 4 of the [porting order](../README.md) moves image decoding and drawing, fonts (EpdFont) and
the renderer (GfxRenderer) to Rust. Its boundary is the frame buffer. ✅ marks a module that is
done.

## PNG decoder ✅

Used for chapter images, the image viewer, the transparent sleep overlay and the Details cover.
`bembeyaz-png` replaces PNGdec. Turning a row gray (luma, alpha or blending with white, the tRNS
color key, low bit depths) is in `bembeyaz-png`'s `gray` module.

Choosing the output rows and the cache file stayed in the C++ converter. Column sampling and
dithering are in `bembeyaz-dither`.

## JPEG decoder ✅

Used for chapter images and covers. `bembeyaz-jpeg` replaces JPEGDEC, with grayscale block output.

The row loops of the cover converter (JpegToBmpConverter) are in `bembeyaz-dither`'s `scale` module
(area averaging; for progressive images, linear upscaling and row blending), dithering is in
`bembeyaz-dither`, and the row state in C++. For chapter images, see "Drawing JPEG images on
screen" below.

## PNG covers ✅

`bembeyaz-png`'s `cover` reader replaces PngToBmpConverter's own decoder. Shrinking rows by area
averaging and dithering are in `bembeyaz-dither`'s `scale` module and in its root. The row state
and writing the BMP are in C++.

## Dithering ✅

In `bembeyaz-dither`: turning BMP rows into the panel's 2-bit levels or into 1 bit. That is
Atkinson and Floyd-Steinberg error diffusion, 1-bit Atkinson, thresholds tuned to the panel, the
native palette, the brightness setting and packing.

`Bitmap` and the PNG and JPEG cover converters make one call per row or per chunk. The memory of
the error rows is in the C++ `Ditherer`. The unused noisy 1-bit threshold was removed.

## Drawing JPEG images on screen ✅

In `bembeyaz-dither`'s `sample` module: JpegToFramebufferConverter's scaling of the decoded blocks
to the output size (bilinear when it upscales on both axes, a copy at 1:1, the nearest pixel in the
other cases; 16.16 fixed point), Bayer ordered dithering or the one-third thresholds, and writing
the rows into the cache band. The rows are written to the screen with `bembeyaz-gfx`'s level rows.

Fitting the block ranges to the screen and the cache file are in C++.

## Image size probe ✅

The chapter parser's `ImageDimsProbe`, in the new `bembeyaz-image` crate. It finds the size of a
JPEG streamed from the EPUB by skipping its marker segments up to the frame header, and of a PNG by
reading its IHDR at fixed offsets. Its state is in opaque `bb_image_probe` memory inside the C++
class.

The bounds check on the size is in C++.

## Drawing PNG images on screen ✅

In `bembeyaz-dither`'s `sample` module: PngToFramebufferConverter's sampling of a row's columns
with a Bresenham step, Bayer ordered dithering or the top two bits, turning alpha into a column
mask with a 4×4 ordered threshold for transparent images, and writing the rows into the cache
band.

Choosing the source rows and the cache file are in C++. `DitherUtils.h` was deleted.

## BMP reader ✅

GfxRenderer's `Bitmap`, used by the sleep screen, the image viewer and cover thumbnails. The
header, the palette and turning rows into brightness are in `bembeyaz-bmp`. SleepActivity's
transparent sleep overlay also reads its header with `Bitmap`.

## EpdFont: SD card font header ✅

In `bembeyaz-font`'s `cpfont` module: `SdCardFont::load`'s reading of the `.cpfont` header and of
the style table of contents, the count limit checks, the section offsets, the content hash and the
interval table check.

## EpdFont: SD card font kerning ✅

In `bembeyaz-font`'s `kern` module: building the page's mini kerning matrix (reading the class
tables block by block, the block index, renumbering the classes, reading the matrix rows).

## EpdFont: SD card font glyphs ✅

In the `glyph` module: decoding glyph records (a glyph whose bitmap is shorter than its size is
drawn empty), the glyph record reads of the page cache, of the advance width table and of
on-demand loading (seeking is skipped for consecutive glyphs), and reading page bitmaps into the
buffer.

In C++: file reading, memory allocation and buffer policy, sorting the read order, reading the
bitmap of a glyph loaded on demand, and the pair lookup when drawing (`EpdFont::getKerning`). The
ligature table stays in C++, because it is copied without being parsed.

## EpdFont: SD card font interval table ✅

In the `intervals` module: `load`'s reading of the interval table, checking it, converting it to a
compact 16-bit form and sharing it among the styles, and the glyph lookup from a code point.

The table's memory is in C++.

## EpdFont: SD card font page set ✅

In the `page` module: collecting code points from the page text (within the budget C++ gives,
U+FFFD, ligature outputs), checking whether the cached page meets the request, merging the cache
with the request, mapping code points to glyphs and building the mini intervals.

Sorting (the `std::sort` instance in C++ is in the firmware already), memory allocation and buffer
policy are in C++.

## EpdFont: SD card font advance width table ✅

In the `advance` module: collecting code points from words for layout (the capital forms of small
caps, space, hyphen; with the 4096 limit C++ gives), the lookup in the table, the plan of glyphs to
fetch (those not in the cache; U+FFFD for missing ones), and merging with the 768 limit C++ gives.

The table's memory and growing it, and the sorts, are in C++.

Of the data logic in `SdCardFont`, only reading the bitmap of a glyph loaded on demand and the
ligature copy are in C++. The rest is memory and cache policy, file access and Rust calls.

## EpdFont: compressed built-in fonts ✅

In `bembeyaz-font`'s `compressed` module: `FontDecompressor`'s page glyph set, finding the groups,
the glyphs' places in the decompressed group, dropping the row padding, and building and searching
the page table.

Memory, inflate (tinfl in ROM), the page slots and the hot group cache are in C++.

## GfxRenderer: drawing primitives ✅

In `bembeyaz-gfx`: lines (Bresenham) and thick lines, rectangle outlines, filled rectangles (black,
white, and the light and dark gray dither patterns), and for them the mapping of logical
coordinates to the panel in four orientations, clipping and grayscale strips. The canvas comes from
C++ as `bb_gfx_canvas`.

## GfxRenderer: rounded rectangles ✅

In `bembeyaz-gfx`'s `round` module: quarter arcs, rounded outlines and fills (each corner can be
chosen on its own), and the mask that rounds the corners of cover images.

## GfxRenderer: filled polygons ✅

In the `polygon` module: edge crossings row by row (without allocating a buffer) and filling
between them.

## GfxRenderer: pictures ✅

In the `image` module: `drawBitmap`'s placing of BMP rows (clipping, shrinking to fit the box, rows
that start from the bottom or from the top, black-and-white and the two gray planes, white taken as
transparent, the 1-bit path), 1-bit icons, and the night mode inversion that keeps pictures in
their own colors.

The `Bitmap` that reads the rows (it too sits on `bembeyaz-bmp`) and the row buffer are in C++.
Rows are asked for through a callback.

## GfxRenderer: glyphs ✅

In the `glyph` module: drawing unscaled glyphs (logical clipping, text rotation and the panel
orientation merged into one right-angle transform, strip clipping, 1- and 2-bit pixels,
black-and-white and the two gray planes; computed once per glyph, then one step per pixel: the next
bit along the panel row, or in vertical text the same bit of the next row). Only the layout struct
(`Frame`) remained in `GlyphBitmap.h`.

Finding the glyph, skipping glyphs outside the strip, the filled box for a character missing from
the font, and building the layout are in C++.

## GfxRenderer: scaled glyphs ✅

In the `scaled` module: shrinking superscripts, subscripts and small caps by block sampling, and
enlarging the drop cap bilinearly. Finding the glyph, the advance and the position are in C++.

## GfxRenderer: screen region copies ✅

In the `region` module: finding the whole bytes a logical rectangle covers on the panel (aligned to
8 pixels, or the bytes touched), and copying the rows into a buffer and writing them back. Used for
the footnote highlight, the cover cache and the background of the sleep popup.

## GfxRenderer: cached image rows ✅

In the `levels` module: drawing the cached 2-bit level rows of chapter images (the orientation once
per row, only the columns that stay on the panel and in the strip, one bit step per column;
black-and-white and the two gray planes). Reading the cache is in C++.

The JPEG and PNG converters draw their rows with it too (PNG's transparent overlay with the column
mask). `DirectPixelWriter` was deleted.

`drawPixel`, which draws single pixels (the activities', the theme's and the SDK interface's own
drawing), `drawImage`'s byte-aligned transfer to the HAL, and the screen refresh are in C++.
