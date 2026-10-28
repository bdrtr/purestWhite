# Hardware tests: images

These tests cover how the firmware reads, scales, dithers and draws pictures: PNG, JPEG and BMP
files, chapter images, book covers and their thumbnails, and sleep screen images.

## PNG images ("rust: our own PNG decoder for chapter images")

Chapter images are cached after the first decode. Before you try the items below, delete the book's
cache (while the book is open, Delete Book Cache in the reader menu) or open the book for the first
time. Signs of a problem: shifted or striped rows, wrong gray tones, transparent areas that come out
black, an empty picture, a hang or a restart.

- [ ] A few books with PNG images (diagrams, screenshots, a transparent logo): the images must look
  as before. Gray tones, transparent areas and large images shown smaller must also be the same.
- [ ] Copy the `png.epub` made with `python3 emu/make_png_epub.py emu/books/png` to the card and
  open its 13 chapters one by one; each chapter has one image. They must look the same as the
  screenshots in `emu/runs/png-png-*/report.html` from the output of
  `emu/compare.sh emu/png.txt x4pro emu/books/png/png.epub`.
- [x] `png-broken.epub` in the same folder: the chapter must open and the device must not hang. The
  log must show `Decode failed: 1`, and an empty frame must stand in the image's place (the same on
  the emulator). The old version waited forever on this image.
- [ ] Speed: open a chapter with a large PNG image with its cache deleted, and compare the
  `PNG decoding complete - render time:` time in the log with the previous version. On the emulator
  a 1600×1000 RGB image takes ~1195 ms, and ~1135 ms with the old one (5% slower). The emulator
  models PSRAM as faster than it really is; if the difference on the device is more than 10%, note
  it.
- [ ] Open a `.png` file from the file browser (try a large image too): the image must look as
  before, and moving to the previous/next image must work.
- [ ] The Details screen of a book with a PNG cover in the OPDS catalog: the cover must show (this
  screen draws the cover with the chapter image decoder).
- [x] Set the Sleep Screen setting to **Transparent**, put a `sleep-overlay.png` with an alpha
  channel in the card's root (there must be no `sleep-overlay.bmp` in the root; it comes first), and
  put the device to sleep: the screen below must show through the transparent areas, and the image
  must show in the opaque areas.
- [x] With a BLE remote connected, open a chapter with PNG images for the first time: the remote
  must stay connected and page turns must work (the decoder never asks internal RAM for a block
  larger than 4 KB).

## JPEG images ("rust: our own JPEG decoder")

Images and covers are cached after the first decode: before you try the items below, delete the
book's cache (while the book is open, Delete Book Cache in the reader menu) or open the book for the
first time. Signs of a problem: a shifted or blocky (8×8 squares) picture, wrong gray tones, missing
strips, an empty picture, a hang or a restart.

- [ ] A few books with JPEG images (photos, scanned pages, comics): the images must look as before;
  large photos too (shrunk to the page).
- [ ] Copy the `jpeg.epub` made with `python3 emu/make_jpeg_epub.py emu/books/jpeg` to the card and
  open its 11 chapters one by one; they must look the same as the screenshots in
  `emu/runs/jpeg-jpeg-*/report.html` from the output of
  `emu/compare.sh emu/jpeg.txt x4pro emu/books/jpeg/jpeg.epub`. Progressive JPEGs look blurry (they
  did before too: only the first scan is decoded, at 1/8 size). The CMYK chapter looks broken and
  striped (it did before too).
- [ ] Books with JPEG covers: the thumbnails on Home and the cover on the sleep screen must show
  (the cover of `jpeg.epub` too); the cover must also show on the Details screen of a book with a
  JPEG cover in the OPDS catalog.
- [ ] Open a `.jpg` file from the file browser (try a large photo too): the image must look as
  before, and moving to the previous/next image must work.
- [ ] While a `.jpg` is open in the image viewer, press **Set Cover**, then put the device to sleep:
  the image must look as before on the sleep screen.
- [ ] Speed: open the three chapters of `jpeg.epub` with large images with the cache deleted, and
  compare the `JPEG decoding complete - render time:` times in the log with the previous version. On
  the emulator the new decoder is slower: "Large, at 1/2" ~425 ms (old ~383, 11%), "Larger, at 1/4"
  ~742 ms (old ~656, 13%), "Wide, at 1/8" ~470 ms (old ~410, 15%); a few ms for small images. The
  time of the cover thumbnail is the time between the timestamps of the
  `Converting JPEG to 1-bit BMP` and `Successfully converted JPEG to BMP` lines in the log: ~232 ms
  on the emulator (old ~219, 6%). If the difference on the device is more than 30%, or opening a
  cover or a page gets visibly slower, note it.

## Broken JPEG ("fix: JPEG open no longer loops on a segment past the end of the file")

- [x] Copy the `jpeg-broken.epub` made with `python3 emu/make_jpeg_epub.py emu/books/jpeg` to the
  card and open its chapter: the page must open and the device must not hang. The page shows only
  the title, and the image is skipped (the log shows `Failed to open JPEG for dimensions (err=2)`);
  the same on the emulator. The old version waited forever on this image. Good JPEGs are not
  affected by this change.

## PNG cover bit depth ("fix: PNG covers refuse a bit depth the color type does not allow")

- [ ] This case only comes up in files damaged by hand, and the host tests cover it: a PNG cover
  with an invalid bit depth no longer crashes the device; the cover is not shown. The device test
  for PNG covers is in the "PNG covers" section below.

## PNG covers ("rust: PNG covers through bembeyaz-png")

Cover thumbnails and the sleep screen cover stay in the cache after they are first made: before you
try the items below, delete the book's cache (while the book is open, Delete Book Cache in the
reader menu) or open the book for the first time. Signs of a problem: an empty, shifted or striped
cover, wrong gray tones, a hang or a restart.

- [ ] A few books with PNG covers (palette, gray, color and transparent covers): the thumbnails on
  Home and the cover on the sleep screen must look as before (the Sleep Screen setting **Cover**:
  the sleep screen shows the cover of the last opened book).
- [ ] Copy the `png.epub` made with `python3 emu/make_png_epub.py emu/books/png` (palette cover) and
  the `inflate.epub` made with `python3 emu/make_inflate_epub.py emu/books/inflate` (RGB cover) to
  the card, open them and go back to Home: in the same theme, the thumbnails must look the same as
  the `home-cover` screenshots in the output (`emu/runs/png-png-*/report.html`,
  `emu/runs/inflate-inflate-*/report.html`) of
  `emu/compare.sh emu/png.txt x4pro emu/books/png/png.epub` and
  `emu/compare.sh emu/inflate.txt x4pro emu/books/inflate/inflate.epub`.
- [ ] Speed: while the thumbnail of `png.epub` is made, measure the time between the timestamps of
  the `Converting PNG to 1-bit BMP` and `Successfully converted PNG to BMP` lines in the log. On the
  emulator ~400 ms (~495 in unmodified CrossPoint), and ~450 ms for the RGB cover of `inflate.epub`
  (CrossPoint ~490). If on the device it is more than 10% longer than the `../crosspoint-base`
  build, note it.
- [ ] Put a `.png` with the same name next to a `.txt` book (a `.bmp`, `.jpg` or `.jpeg` with the
  same name comes first; if there is no image with the same name, `cover.png` in the folder is also
  used): open the book and go back to Home; the thumbnail and the sleep screen cover must show.
- [ ] With a BLE remote that is paired but not yet connected, make the thumbnail of a book with a
  PNG cover again, with its cache deleted (open and close the book, restart the device and go back
  to Home): note the `HID begin skipped` / `BLE start deferred` lines in the log and the free and
  largest block figures; the remote must connect (if needed, at the next book opening), and page
  turns must work. While the cover is read, the ~2.3 KB reader state (before, it was on the task
  stack) stays in the internal heap; the reader never asks internal RAM for a block larger than 4 KB
  (the large row buffers go to PSRAM).

## BMP reader in Rust ("rust: our own BMP reader")

For sleep screen images, the image viewer and cover thumbnails, the header and the palette of BMP
files are now read with `bembeyaz-bmp`, and their rows are turned into brightness with it;
dithering and the conversion to the screen's gray tones stayed in C++. 898 host tests pass,
including the new `test/bmp_reader` tests; these new tests also pass with the old code. During
development a difference test compared the old C++ code with the Rust code row by row, with and
without dithering, on 30,000 generated BMPs (some broken or cut short) and on 12 real BMPs from a
card backup: the results were the same. On the emulator, `emu/compare.sh emu/bmp.txt` gave the same
screens as unmodified CrossPoint on seven BMPs.

- [ ] Put a few `.bmp` files in the `sleep` folder in the card's root, set Settings > Display >
  Sleep Screen to Custom, and put the device to sleep: the images must look as before.
- [ ] Copy the seven files made with `python3 emu/make_bmp_files.py bmp` to the card and open them
  one by one from the file list: they must look right in the image viewer; `7-cut.bmp` must show
  only in part, and the device must not get stuck.
- [ ] The book cover thumbnails on Home must look as before (no need to delete a cache: Home reads
  these BMPs again each time it draws).

## Sleep overlay header through `Bitmap` ("sleep: read the overlay BMP header with Bitmap")

SleepActivity's own BMP header reader for the transparent sleep overlay was removed; the header is
now read with `Bitmap` (that is, with `bembeyaz-bmp`). The only difference: a 32-bit file whose
header declares more than 256 palette colors is no longer drawn as an overlay (it was already
refused as a normal sleep image); `test/bmp_reader` pins this. On the emulator, `emu/sleep.txt` drew
the overlay the same as unmodified CrossPoint.

- [ ] Put a 32-bit `sleep-overlay.bmp` with transparent areas in the card's root (for example
  `bmp/overlay/card/sleep-overlay.bmp`, made with `python3 emu/make_bmp_files.py bmp`), set
  Settings > Display > Sleep Screen to "Transparent" (this option has no Turkish translation), and
  put the device to sleep by holding the Power button. With the generated file, a colored ring that
  fades toward the inside must show on top of the screen from before sleep; outside the ring, the
  earlier screen must stay as it was.

## Bottom-up picture clipping fixed ("fix: draw bottom-up pictures that start below the screen")

A bottom-up picture (most BMPs are stored this way) that ran past the bottom of the screen, that is,
whose first row in the file fell below the screen, was not drawn at all before: the gray-tone path
said "below the screen" at the first row and stopped. The same picture stored top-down was drawn
with its visible part. Now the bottom-up picture reads and skips those rows and draws the rows that
come back onto the screen. Today's screens always fit the picture on the screen, so no visible
difference is expected; on the emulator the image viewer, sleep and the home screen with covers are
the same as the previous version. A crate test and the comparison with the old code pin this case.

- [ ] If there is a BMP larger than the screen that is shown without clipping (e.g. a large image
  with Crop on the Sleep Screen), it must look as before or more correct (its bottom part not cut
  off, its visible part drawn).

## Dithering in Rust ("rust: our own dithering")

Turning pictures into the panel's four gray levels or into black and white (the BMP image viewer
and the sleep screen's custom images, and the conversion of JPEG and PNG covers and home screen
thumbnails to BMP) is now in the new `bembeyaz-dither` crate: Atkinson and Floyd-Steinberg error
diffusion, 1-bit Atkinson, thresholds tuned to the panel, the natural palette and a brightness
adjustment (off at compile time). The noisy 1-bit threshold, which no path reached before either,
was removed. The `Bitmap`, PNG and JPEG cover converters now make one call per row or per 64-pixel
piece, not one per pixel; the memory of the error rows is in the single C++ `Ditherer` class (in
place of the old three classes; `Bitmap` holds it in place, with no new allocation or error path).
The Rust loop carries the errors by shifting them in registers, without indexing: there is no
bounds check and no panic path. The first version had made the cover thumbnail 10% slower; now it
is the same as before or a little faster (on the emulator the JPEG thumbnail takes 230–233 ms, old
232–235; in the image viewer seven BMPs are 128 ms faster in total). The old dithering classes and
functions were copied as they were to `test/dither`: on 4000 random pictures (each method, random
pieces, row boundaries) the output is the same byte for byte, and the bits of the last byte that lie
outside the picture do not change. All 21 faults put into the code on purpose were caught. The
841-case digest test of PNG→BMP and the BMP reader tests also pass unchanged. On the emulator the
image viewer, sleep, and the home screens with JPEG and PNG covers are the same as the previous
version pixel for pixel. Flash grew by 780 bytes on x4pro and 848 on x4c (separate loops per
method), static RAM did not change.

- [ ] Open 24-bit and 32-bit BMPs from Browse Files: the gray tones and the dither pattern must be
  as before.
- [ ] Set Sleep Screen to Custom and put a color BMP as `/sleep.bmp` (or in the `/sleep` folder);
  try Sleep Screen Cover Filter with None and Contrast, and Sleep Screen Cover Mode with Fit and
  Crop; also set Sleep Screen to Cover and go to sleep from a book: the images must look as before.
- [ ] Open books with JPEG and PNG covers that were not opened before, and go back to Home (the
  thumbnail is made there, during "Loading"): the covers must look as before, and the wait must not
  get noticeably longer.

## Cached image rows in Rust ("rust: our own cached image rows")

After chapter images are decoded for the first time (including the gray passes of the first
display), they are drawn from the cache (rows of 2-bit gray levels); writing these rows to the
screen is now in the `levels` module of `bembeyaz-gfx` (`GfxRenderer::drawLevelRow`). The
orientation is worked out once per row, only the columns that stay on the panel and in the current
gray-tone strip are visited, and each column is one bit step away from the previous one: the old
`DirectPixelWriter` loop unpacked the level and computed the position again for each pixel. Reading
the cache, and decoding the image the first time and writing it to the cache, did not change (the
JPEG and PNG converters still use `DirectPixelWriter`). The old writer was copied as it was to
`test/gfx_primitives`: on 20000 random rows (four orientations, black and white and the two gray
planes, absolute and difference gray planes, strips; only rows on the screen, because the old
writer did not support rows that run off the screen) the buffer is the same byte for byte. The
crate tests check the cutting of rows that run off the screen (a cached image can be one column or
one row wider, so for an image that touches the screen edge the old writer wrote this pixel to the
next panel row, and on the last row of a strip one byte past the buffer; now it is dropped). All 10
faults put into the code on purpose were caught (two of them only by these edge tests). On the
emulator, gray-plane drawing in chapters with JPEG and PNG images got 11–15% shorter (median per
page 97–98→85–86 and 86→73–74 ms), and all screenshots are the same as the previous version. Flash
grew by 460 bytes on x4pro and 480 on x4c, static RAM did not change.

- [ ] Open an EPUB with JPEG and PNG images and turn the pages with images back and forth: the gray
  tones and edges of the images must look as in the previous version (the gray tones are drawn from
  the cache on the first display too).
- [x] Open the same pages in the four orientations (the Reading Orientation setting): the image must
  be in its place, with no shift and no half-drawn strip.

## Drawing JPEG images in Rust ("rust: our own JPEG image sampling")

Chapter images, the cover on the Details screen of books in the OPDS catalog, and JPEGs that are
opened in the image viewer and made the sleep image there with "Set Cover" are drawn to the screen
in this way while they are decoded: the gray blocks from the decoder are brought to the output size
(bilinear when enlarging on both axes, a copy at 1:1, the nearest pixel in the other cases), turned
into the panel's four levels with a 4×4 Bayer pattern or, without dithering, with one-third
thresholds, and written row by row to the screen and to the cache band. All of this is now in the
`sample` module of `bembeyaz-dither`; the rows go to the screen as level rows, through the work of
the previous section. Before, there was one `DirectPixelWriter` call and one cache write per pixel;
now there are three calls per row of a block, in pieces of at most 512 columns
(`bb_dither_block_row`, `drawLevelRow`, `bb_dither_put_levels`). A bug in the old bilinear loop was
fixed: because the inverse scale was rounded down, the first column of a block could fall to -1 in
the source, and the inner loop (and, for one-column blocks, also the right-edge loop) read the byte
before the row (on the first row, one byte before the buffer). This could shift single pixels at
block edges by one level; now every column stays inside the block. The old callback was copied to
`test/image_sampling` (with these fixes marked): on 20000 random pictures (scales, crop, position,
running off the screen, four orientations, three planes, strips, block sizes, dithering on or off,
with or without cache) the screen and the cache are the same byte for byte. Of the 18 faults put
into the code on purpose, this test caught 16 and the crate tests caught one; the last one does not
change the result (255 and 254 fall to the same level). On the emulator the JPEG chapter and broken
JPEG scenarios are the same as the previous version pixel for pixel, and page times did not change
(decode time dominates). Flash grew by 440 bytes on x4pro and 448 on x4c, static RAM did not change;
there is a 128-byte buffer per callback on the stack.

- [ ] Open an EPUB with JPEG images for the first time (with no cache, the image is drawn while it
  is decoded): the images must look as before; enlarged small images must show no lines or shifts
  at block edges.
- [ ] Open the Details screen of a book with a JPEG cover in the OPDS catalog: the cover must look
  as before.
- [ ] Open a JPEG from Browse Files: it must look as before. First set Sleep Screen Cover Mode to
  Crop, use Set Cover on a JPEG larger than the screen and go to sleep; then repeat with Fit: the
  sleep image must look as before, with the correct crop.

## Drawing PNG images in Rust ("rust: our own PNG image sampling")

While PNGs (chapter images, the cover on the Details screen of books in the OPDS catalog, the image
viewer and the transparent sleep overlay `/sleep-overlay.png`) are decoded, each output row is now
drawn as for JPEGs, with three calls in pieces of at most 512 columns: `bembeyaz-dither` samples the
columns in the source row with a Bresenham step (column c reads the source pixel crop + c × visible
width / output width; the start of a piece is computed in closed form), turns them into levels with
a Bayer pattern or, without dithering, with the top two bits, and for transparent images turns the
alpha into a column mask with a 4×4 ordered threshold. The rows go to the screen as level rows; this
writer now takes a mask, and on masked rows (only the transparent overlay) it also writes white in
black and white (the old writer's `writeWhiteInBw`). So `DirectPixelWriter.h` and `DitherUtils.h`
were no longer used and were deleted; their old copies stay in the tests. Turning the source row
into gray, and the choice of rows, did not change. The old callback was copied as it was to
`test/image_sampling`: on 20000 random pictures (crop, fit and full size (including enlarging),
position, running off the screen, four orientations, three planes, strips, dithering, transparency,
cache; pieces of 24 columns in the test) the screen and the cache are the same byte for byte. Of the
13 faults put into the code on purpose, this test caught 11 and a crate test caught one (the
threshold pattern of a piece that does not start at a multiple of 4; in the converter, pieces
always start at a multiple of 4); the last one does not change the result. On the emulator the
chapters with PNG and JPEG images and the broken PNG are the same as the previous version pixel for
pixel, and the times did not change. On the emulator, the transparent PNG overlay (`emu/sleep.txt`,
the `overlay-png` card of `make_bmp_files.py`) was also drawn the same pixel for pixel as in the
version before this change (38e2e5ef). Flash grew by 1656 bytes on x4pro and 1624 on x4c (two write
loops, with and without a mask, and the sampling), static RAM did not change; there is a 192-byte
buffer per callback on the stack (like the JPEG buffer, it now starts zeroed).

- [ ] Open an EPUB with PNG images for the first time and turn the pages: the images must look as
  before.
- [ ] Open a PNG from Browse Files: it must look as before.
- [ ] Open the Details screen of a book with a PNG cover in the OPDS catalog: the cover must look as
  before.
- [x] Set Sleep Screen to Transparent (it has no Turkish translation), put a transparent
  `sleep-overlay.png` in the card's root (for example a ring with an opaque edge that gets more
  transparent toward the inside; there must be no `sleep-overlay.bmp` in the root, because that one
  is chosen first), and go to sleep: the overlay must sit on top of the screen as before; the screen
  below must show in the transparent areas, and opaque white areas must be white.

## PNG rows to gray in Rust ("rust: our own PNG row gray")

While PNGs are decoded, turning each source row into gray (PngToFramebufferConverter's
`convertLineToGray`: chapter images, the Details cover in the OPDS catalog, the image viewer, the
transparent sleep overlay) is now in the `gray` module of `bembeyaz-png`: luma by pixel type
(77/150/29 out of 256), scaling 1-, 2- and 4-bit samples to 255, the palette and the palette's
alpha, treating the tRNS color key as transparent, keeping the alpha apart for the overlay or
blending it onto white, and middle gray for unknown types. Samples are read without division;
indexes are bounds-checked, and reads outside the row give 0 (the old code assumed the row was long
enough). A row without a palette (a case that does not happen, because the decoder always gives a
palette) is now middle gray; before, rows that were not indexed were converted even without a
palette, and indexed rows were read like grayscale. The old function was copied as it was to
`test/image_sampling`: on 20000 random rows (five pixel types and two unknown types, 1, 2, 4 and 8
bits, a color key that matches or not, with or without alpha, overlay or not) the gray and the alpha
are the same byte for byte. 14 of the 15 faults put into the code on purpose were caught; the last
one does not change the result (at 1, 2 and 4 bits the largest value divides 255). On the emulator
the PNG book (every color type and bit depth, palette and color-key transparency, alpha) and the
broken PNG are the same as the previous version pixel for pixel, and the times did not change; the
transparent PNG sleep overlay (`emu/sleep.txt`) is also the same as in the version before the PNG
changes (38e2e5ef). Flash grew by 1332 bytes on x4pro and 1220 on x4c (separate loops with and
without alpha for each pixel type), static RAM did not change.

- [ ] Open an EPUB with PNG images (color, gray, palette and transparent-background images) for the
  first time: the images must look as before; transparent backgrounds must be white.
- [x] Transparent sleep overlay (`/sleep-overlay.png`, as in the last item of the previous section):
  the overlay must sit on top as before.

## Cover row scaling in Rust ("rust: our own cover scaling")

When covers are converted to BMP (home screen thumbnails, the Cover mode of the Sleep Screen),
bringing the rows to the output width is now in the `scale` module of `bembeyaz-dither`: the same
area-average loop of JpegToBmpConverter and PngToBmpConverter (summing the source columns that each
output column covers) is one function; dithering the averages in 64-pixel pieces is in the `scale`
module's `area_pixels` (called from C++ through `Ditherer::areaPixels`); and the linear row sampling
and the blending of two rows that JPEG uses when it enlarges progressive pictures are there too.
Which source row falls on which output row (the row state), the BMP writing and the 8-bit output
path (off at compile time) stayed in C++. Reads are bounds-checked. The old loops were copied as
they were to `test/dither`: on 4000 random cases (widths, the converters' scales or random scales
that overflow, sums of a few rows, two rows with five dithering methods, linear and blended rows)
the results are the same byte for byte; the 841-case digest test of the PNG cover converter also
passes unchanged. All 13 faults put into the code on purpose were caught. On the emulator the home
screens with JPEG and PNG covers are the same as the previous version pixel for pixel; the JPEG
thumbnail took 227 ms instead of 233, and the PNG thumbnail 388 ms instead of 397. Flash grew by
284 bytes on x4pro and 316 on x4c, static RAM did not change.

- [ ] Open books with JPEG and PNG covers that were not opened before, and go back to Home: the
  thumbnails must look as before.
- [x] Set Sleep Screen to Cover and go to sleep from a book; try Sleep Screen Cover Mode with Fit
  and Crop: the cover must look as before.
- [ ] If there is a book with a progressive JPEG cover, its thumbnail must also look smooth as
  before.

## Image size probe in Rust ("rust: our own image size probe")

While a chapter is built, the size of each image is found from its first bytes as they stream from
the EPUB, without opening the whole image (`ImageDimsProbe`): for JPEG, marker segments (EXIF/APPn
of any size, without buffering) are skipped up to a frame header (SOFn); for PNG, the width and
height are read from their fixed places in IHDR. This state machine is now in the new
`bembeyaz-image` crate; its state lives inside the C++ class in 28 bytes of opaque `bb_image_probe`
memory and is set up with `bb_image_probe_init` (no new allocation); the bounds check of the size
and the log line are in C++. The crate has a mini-fuzz test with shortened and damaged headers. The
old parser had no direct test; it was copied as it was to `test/image_probe`: on 20000 random
headers (JPEG markers of every kind and segment lengths, fill bytes, a frame header or, before it, a
scan or the end of the file, a PNG signature and IHDR, random damage and garbage, in random pieces)
the counts of bytes taken and the size found are the same. All 17 faults put into the code on
purpose were caught (one by the crate test). On the emulator the books with JPEG and PNG images and
the broken JPEG are the same as the previous version pixel for pixel, and the image extraction log
lines are the same too. Flash grew by 100 bytes on x4pro and 188 on x4c, static RAM did not change.

- [x] Open an EPUB with JPEG and PNG images for the first time: the images must be placed as before
  (correct sizes, page breaks as before), and the first opening must be as fast as before (if the
  probe fails, the whole image is extracted: this is correct, but it takes seconds per image).

## PNG cover samples without division ("png: cover samples without division")

The cover reader of `bembeyaz-png` now reads the samples in 1-, 2- and 4-bit gray and palette rows
without division (by bit shifting), and scales the gray with a factor that divides 255; the row
filter is also computed with the pixel step known not to be zero. The results are the same; only
the divide-by-zero panic paths and the divisions went away: flash shrank by 240 bytes on x4pro and
244 on x4c. The 841-case digest test of the PNG cover converter (every color type and bit depth)
and the crate tests pass unchanged; on the emulator the PNG book is the same pixel for pixel.

- [ ] The thumbnail of a book with a 1-, 2- or 4-bit gray or palette PNG cover must look as before.
