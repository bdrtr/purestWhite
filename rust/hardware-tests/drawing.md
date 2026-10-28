# Hardware tests: drawing

These tests cover the drawing code that moved from GfxRenderer to the `bembeyaz-gfx` crate: lines,
rectangles, rounded shapes, polygons, pictures, glyphs and copies of screen regions.

## Drawing primitives in Rust ("rust: our own drawing primitives")

The lines, rectangle frames and filled rectangles of the UI and of book pages (black, white, and
light and dark gray dither patterns; selection bars, buttons, progress bars, table borders, boxes
with shadows, the straight edges of rounded rectangles) are now drawn in the `bembeyaz-gfx` crate:
logical coordinates are mapped to the panel's bit layout for all four screen orientations, and the
clip rectangle and the gray-tone strips are respected. For each drawing, GfxRenderer passes the
canvas (buffer, rows, panel size, orientation, clip) as `bb_gfx_canvas`. The crate has 5 unit tests
(including a mini-fuzz). Because GfxRenderer is not built in the host tests, the old drawing code
was copied as it was into the new `test/gfx_primitives` host test; on each run the test compares
the two paths on 20000 random canvases, and checks that the rounded-corner dithering that stayed in
C++ matches Rust's fill pattern (1048 host tests pass in total). During development the same
harness, with random panel sizes, orientations, strips and clips, found the old and new buffers the
same byte for byte on 144000 drawings over 12000 canvases, and the counts of pixels that fall off
the panel the same too; it caught five of the six faults put into the code on purpose (the sixth
was writing past the last row of the strip, which cannot happen anyway, because the buffer is
exactly that size). For lines and frames, pixels that fall off the panel are now logged with a
single error line per drawing, not each one on its own with its coordinates; an attempt to draw
when there is no buffer (while it is lent out for a build) now warns once instead of crashing as
before. On the emulator the settings, sleep, library, BLE settings, clipping, reading and book
opening (CSS and XML), JPEG and PNG scenarios gave the same screens and the same drawing times as
the previous version. On x4pro, flash shrank by 316 bytes; RAM did not change.

- [ ] Open the home screen, the menus, the settings and the library; open a book in the four
  Reading Orientation values (Portrait, Landscape CW, Portrait 180°, Landscape CCW): selection bars,
  frames, progress bars and gray fills must look as before. With Text Anti-Aliasing on, a page with
  a table, horizontal rules and images must also be drawn as before.

## Rounded rectangles in Rust ("rust: our own rounded rectangles")

The UI's rounded rectangles are now drawn in the `round` module of `bembeyaz-gfx`: quarter arcs,
rounded frames (Lyra's button hints, RoundedRaff's bottom hint groups, the buttons in text
selection), rounded fills (popups, menu rows, the home screen's selected cover box, the text
selection handles; each corner can be chosen on its own, gray dither patterns included) and the
mask that rounds the corners of RoundedRaff's cover images. GfxRenderer used to draw these row by
row or pixel by pixel; now there is one Rust call per shape, and `fillArc` and `drawPixelDither`
were deleted from C++. The crate has 7 unit tests; the old C++ code of these four shapes was also
added to the copy of the old code in `test/gfx_primitives`, and on 20000 random canvases the two
paths came out the same byte for byte, with the same counts of pixels off the panel. It caught nine
of the ten faults put into the code on purpose (the tenth was equivalent: the extra column of the
middle fill already falls under the right column or the corner arc). One thing changed on purpose:
like the other drawings, the corner mask draws nothing during the font cache scan (only
RoundedRaff's home screen calls it, and there is no scan there), and pixels that fall off the panel
are logged with one line instead of a separate line each. On the emulator the settings, sleep,
library, BLE settings, clipping, reading, book opening, JPEG and PNG scenarios, and the screens of
the Lyra, Lyra Extended, RoundedRaff, Cover Grid and Classic themes (including RoundedRaff's home
screen with covers), came out the same as the previous version pixel for pixel. Flash grew by 156
bytes on x4pro and 328 on x4c, RAM did not change.

- [ ] In Settings → Display → UI Theme, choose Classic, Lyra, Lyra Extended, RoundedRaff and Cover
  Grid in turn; in each, open the home screen (with a book with a cover opened), the menus, a popup
  and the button hints at the bottom: the rounded corners, the selected boxes and the rounded
  corners of RoundedRaff's covers must look as before; there must be no gaps, extra dots or cut
  edges at the corners. (Cover Grid is listed only on a device with PSRAM.)
- [ ] On a touch screen, select text in a book (in the Classic and Lyra themes): the selection
  handles and the Look Up / Clip / Bookmark buttons must look as before.

## Filled polygons in Rust ("rust: our own filled polygons")

GfxRenderer's `fillPolygon` (the bookmark ribbon of a book without a cover on the Classic theme's
home screen, and FreeInkUI's triangles: today only the sort arrow in the Library's tab bar) is now
in the `polygon` module of `bembeyaz-gfx`. The old code allocated a node buffer on the heap on each
call, sorted the intersections on each row and drew each pixel on its own; the new code allocates
no buffer (so the allocation-failure path is gone too), finds each row's intersections in order by
walking the edges again, and fills the space between them with a single `fill_rect`. The cost is
that the work per row grows with the square of the number of points; the callers draw shapes with
3 and 5 points. Intersections are computed with 64 bits: the old code's undefined overflow at large
coordinates is gone, and values wrap only at coordinates near the i32 limits. The only visible
difference: like the other drawings, the polygon draws nothing during the font cache scan. The old
`fillPolygon` was added to the copy of the old code in `test/gfx_primitives`; with random polygons
of 0–8 points on 20000 canvases the two paths are the same byte for byte, and all nine faults put
into the code on purpose were caught. The crate has 8 unit tests. On the emulator the bookmark
ribbon on the Classic theme's home screen (with the card selected and not selected), the Library's
sort arrow in both directions, and the settings, library, reading, book opening, sleep, clipping
and BLE settings scenarios came out the same as the previous version pixel for pixel. Flash shrank
by 64 bytes on x4pro and 240 on x4c, RAM did not change.

- [ ] In the Classic theme, open a book without a cover and go back to the home screen: the
  bookmark ribbon in the book box (with a notched bottom end) must look as before. The box is
  selected when the screen opens, so the card is drawn black and the ribbon white; when you move the
  selection down, the ribbon must show black.
- [ ] Open the Library: the triangle sort arrow on the active tab must look as before. While the
  tabs are selected, long-press Confirm (on a touch screen, tap the active tab): the sort direction
  and the arrow must flip.

## Pictures in Rust ("rust: our own picture drawing")

GfxRenderer's `drawBitmap` and `drawBitmap1Bit` (cover thumbnails on the home screens, the sleep
screen's cover or custom image and its overlay, the image viewer), `drawIcon` (icons of books
without a cover, Lyra's main menu icons, Cover Grid's tab and empty home screen icons) and
`preserveImagePolarity`, which keeps pictures in their own colors in night mode (these pictures and
the book pictures in the reader), are now in the `image` module of `bembeyaz-gfx`: clipping,
shrinking to fit a box, rows that start at the top or at the bottom, black and white and the two
gray planes (difference or absolute levels), treating white as transparent, and the separate rules
for 1-bit files. `Bitmap`, which reads rows from the file, and the row buffer are in C++; Rust asks
for each row through a callback, and draws nothing if the buffer is shorter than a row. The old two
separate paths were merged into one loop; their differences (the 1-bit path does not clip, draws
black on every plane, and reads and skips the rows below the screen) are kept with a flag. One
quirk of the old code was kept on purpose: if the first row of a bottom-up picture falls below the
screen, the picture is not drawn at all. `drawIcon` and `drawBitmap1Bit` now draw nothing during the
font cache scan, like the other drawings. The old picture code (with a fake `Bitmap` in memory) was
added to the copy of the old code in `test/gfx_primitives`; on 6000 random pictures, icons and
inversions the two paths are the same byte for byte, and so is the row that could not be read. All
the faults put into the code on purpose that were not equivalent were caught. The crate has 11 unit
tests. On the emulator, seven BMPs in the image viewer (1, 2, 4, 8, 24, 32 bits and a cut-short
file), the sleep screen, books with JPEG and PNG images, the Lyra and RoundedRaff home screens with
covers and book opening with night mode on, the Lyra Extended and RoundedRaff home screens, and the
library and reading scenarios came out the same as the previous version pixel for pixel; drawing
per picture in the image viewer is the same or 30–70 ms shorter (24-bit and 32-bit BMPs). Flash grew
by 688 bytes on x4pro and 936 on x4c (pixel writes are now inline; the old code called `drawPixel`
for each pixel), RAM did not change.

- [ ] With night mode off and on, open the Classic, Lyra, Lyra Extended, RoundedRaff and Cover Grid
  home screens with books with covers (JPEG and PNG) (Cover Grid only on a device with PSRAM): the
  cover thumbnails must look as before, and in night mode too in their own colors; the book icon of
  a book without a cover, the main menu icons in Lyra and the tab icons in Cover Grid must be as
  before.
- [ ] With night mode off and on, open a book with JPEG and PNG images, go to a page with an image
  and come back (first opening and opening from the cache): the images must show in their own
  colors.
- [ ] Set Sleep Screen to Custom and put `/sleep.bmp` in the card's root (or BMPs in the `/.sleep`
  folder); try Sleep Screen Cover Mode with Fit and Crop, and Sleep Screen Cover Filter with None
  and Inverted: the cropping, scale and gray tones of the image must be as before. Also try a 1-bit
  image with Crop. Also set Sleep Screen to Cover and go to sleep from a book.
- [ ] Set Sleep Screen to Transparent, put a `/sleep-overlay.bmp` with no transparency channel
  (24-bit) in the card's root and go to sleep while reading: the image must show on top of the page,
  with its whites transparent.
- [ ] From Browse Files, open 1-, 2-, 4-, 8-, 24- and 32-bit BMPs in the image viewer, with night
  mode off and on: they must look as before.

## Glyphs in Rust ("rust: our own glyph drawing")

Every unscaled glyph (the body of the book text, menus, button hints, the rotated text on the side
buttons of the on-screen keyboard) is now drawn in the `glyph` module of `bembeyaz-gfx`: the
logical clip, the rotation of the text and the panel orientation are combined into a single
right-angle transform, and the strip (gray-tone band) clipping and the addressing are computed once
per glyph; the rules for 1-bit and 2-bit pixels, black and white and the two gray planes are the
same as before. What does not go through this path stays as before: superscripts and subscripts and
small capitals (scaled glyphs) and the drop cap are drawn pixel by pixel, and the boxes for
characters that are missing from the font are drawn as filled rectangles. Only the placement struct
remains in `GlyphBitmap.h`; a placement that is not at a right angle now draws nothing (callers
always give a right-angle placement). The first version made page drawing 20% slower; the path
where a glyph row falls on a single bit column of the panel in portrait orientation was split out
(the mask once per row, a fixed byte step), full 2-bit bytes were unpacked with fixed shifts, the
plane rules were moved to compile time and the black/white choice to a branchless mask, and the
panel axis is found with a single orientation match. On the emulator page drawing times are almost
the same as before: in portrait, on three pages, black and white 44–45 ms (old 44), gray planes 216
and 249 ms (old 215 and 246, at most 1%); in landscape, on one page, 11 (old 10), 69–70 and 80 ms
(old 69 and 79). The cost is flash: 3324 bytes on x4pro and 3440 on x4c (separate loops for the
four planes and the depths), RAM did not change; the stack frame is 256 bytes (the old nested ones
272). The pixel-by-pixel reference of the existing `test/glyph_bitmap` test now runs against Rust;
the old `GlyphBitmap.h` was copied as it was into the test folder (only the namespace changed), and
on 20000 random glyphs (eight right-angle glyph directions, four panel orientations, strips, clips,
some panel widths that are not a multiple of 8) the two paths are the same byte for byte. All 24
faults put into the final code on purpose were caught. The crate has 12 unit tests. On the emulator
the settings, library, book opening (including the CSS, bidirectional and XML books), reading
(portrait and landscape), sleep, clipping, BLE settings, BMP viewer, night mode, and the Classic,
Lyra Extended, RoundedRaff and Cover Grid home screens came out the same as the previous version
pixel for pixel.

- [x] Read a book in the four Reading Orientation values (Portrait, Landscape CW, Portrait 180°,
  Landscape CCW) with Text Anti-Aliasing off and on: letters must look sharp with AA off, and with
  gray edges as before with AA on; there must be no cut or shifted letters at the page edges or in
  clipped places.
- [ ] In the serial log, compare the `bw_render`, `gray_lsb` and `gray_msb` times on the
  `Page render:` lines, in portrait and landscape reading, with the previous version (64b27c5b):
  they must be the same or very close (on the emulator, about 15/72/83 ms per page in portrait);
  report it if there is a clear slowdown.
- [ ] On a device without a touch screen, open the on-screen keyboard (e.g. while entering a Wi-Fi
  password): the rotated ">" and "<" text on the side buttons must look as before. Also look over
  the menus and the button hints at the bottom.

## Scaled glyphs in Rust ("rust: our own scaled glyphs")

Superscripts and subscripts (half size), small capitals (three-quarter size) and drop caps are now
drawn in the `scaled` module of `bembeyaz-gfx`. When shrinking, each pixel looks at the source block
it covers: at half size, any ink in the block (at 2 bits, dark gray or two light grays) is drawn,
so thin lines are not lost; at three quarters, a majority is needed, so stems do not get thicker.
For a drop cap, the center of each pixel is mapped back to the glyph, and the pixel is drawn when
the bilinear coverage of the four neighbors reaches half of black. Finding the glyph, the advance
width and the position are in C++; before, there was a separate `drawPixel` call for each pixel,
now there is one Rust call per glyph and a single log line for pixels that fall off the panel. Rust
does not draw glyphs larger than 4096 pixels or scales larger than 16 times (chapters store at most
16 times; before, a broken chapter cache could start a loop for a scale of up to 65 thousand
times). The two old loops were added to the copy of the old code in `test/gfx_primitives`; on 6000
random glyphs (1 and 2 bits, half and three-quarter size, scales from 1/256 to 16 times — chapters
store 1 to 16 times —, some 255-pixel glyphs, four orientations, strips, clips) the two paths are
the same byte for byte and in the off-panel count; all 12 faults put into the code on purpose were
caught. On the emulator the CSS book's small-caps heading, its three-line drop cap and its page
with superscripts are the same as the previous version pixel for pixel, and the drawing times are
the same too (black and white 10, gray planes 69 and 79 ms). Flash grew by 540 bytes on x4pro and
424 on x4c, RAM did not change.

- [ ] Open a book with superscript footnote numbers, subscripts, small-caps headings and paragraphs
  that start with a drop cap, in the four Reading Orientation values, with Text Anti-Aliasing off
  and on: these must look as before.

## Screen region copies in Rust ("rust: our own screen region copies")

The five GfxRenderer functions that save a part of the screen and put it back without drawing it
again (`readFramebufferRegion`/`writeFramebufferRegion`: what is under the highlight while choosing
a footnote in the reader; `getRegionByteSize`/`copyRegionToBuffer`/`copyBufferToRegion`: the cover
buffer of the Lyra, Classic and RoundedRaff home screens, Cover Grid's cover cache, and the
background of the popup that appears when going to sleep while Sleep Screen is "Transparent") are
now in the `region` module of `bembeyaz-gfx`: the bytes that the logical rectangle covers on the
panel are found (aligned to 8 pixels, or the bytes that are touched), and the rows are copied to
the buffer and written back. The only thing that changed on purpose: when there is no framebuffer
(while it is lent out for a build), `readFramebufferRegion`/`writeFramebufferRegion` do nothing
instead of crashing as before. The old five functions were added to the copy of the old code in
`test/gfx_primitives`; on 20000 random rectangles (panel widths that are not a multiple of 8, four
orientations, short buffers) the sizes, the copies and the screen written back are the same byte
for byte. On the emulator the Lyra, RoundedRaff and Cover Grid home screens with covers, sleep,
library, reading and settings are the same as the previous version pixel for pixel. Flash grew by
340 bytes on x4pro and 276 on x4c, RAM did not change.

- [ ] With a book with a cover, on the Lyra, Classic and RoundedRaff home screens, move the
  selection between the cover and the menu a few times: the cover must come back complete every
  time. In the Cover Grid theme (on a device with PSRAM), move between covers and between tabs: the
  covers that come from the cache must be complete.
- [ ] On a page with at least two footnotes, open "Links and footnotes" from the reader menu and
  move between the footnotes with the arrow or side buttons: when the highlight moves, no trace must
  stay in its old place.
- [ ] Set Sleep Screen to Transparent and put an overlay image (`/sleep-overlay.bmp` or `.png`) in
  the card's root; go to sleep while reading (also in a rotated Reading Orientation) and while on
  the home screen: no trace of the "Going to sleep" popup must stay under the overlay.
