# Hardware tests: fonts and decompression

These tests cover DEFLATE decompression (the ROM's inflate and the PNG decoder's inflate), the
readers for `.cpfont` fonts on the SD card, and the compressed fonts built into the firmware.

## DEFLATE and fonts (b340e76, fdd0780)

- [ ] Time to open a book and build a chapter; compare it roughly with the previous version
  (`Time to parse and build pages` in the log). EPUB files, fonts and the dictionary are now
  decompressed with the ROM's inflate: see the "Inflate in the ROM" section below.
- [ ] Reading with a font loaded from the SD card: glyphs must be drawn without damage, and page
  turns must be as fast as before.
- [ ] A book with a PNG cover: the cover must show.
- [x] Dictionary (.dict.dz): word lookups must bring the definitions.

## Decompression ("fix: inflate refuses a distance before the start of the stream")

These two fixes now apply only to PNG pictures (`bembeyaz-png`); they show up in hand-damaged
files, and host tests cover them. The damaged EPUB test is in the "Inflate in the ROM" section
below.

- [x] There is nothing else to test on the device: it is enough that the `png-broken.epub` test
  in the "PNG images" section of [images.md](images.md) does not hang. Intact books are not
  affected by these changes.

## Decompression speed ("rust: inflate for the PNG decoder")

- [ ] Delete the cache of a large book (Delete Book Cache in the reading menu while the book is
  open) and open it: the pages must look as before; the `Time to parse and build pages` time in
  the log must not be longer than with the previous version (in the emulator the difference is
  within 3 ms). The cover thumbnails (PNG covers) must show too.

## Inflate in the ROM ("inflate: InflateStream decodes with the ESP32-S3 ROM's miniz")

Files inside an EPUB (chapters, OPF, table of contents, CSS, picture files), the glyph groups of
compressed fonts (the built-in reading fonts and downloaded `.cpfont` files), the chapters that
KOReader sync reads and the dictionary (.dict.dz) definitions are now decompressed with the miniz
in the S3 ROM (before, with our own Rust decoder). The inside of PNG pictures is still decoded by
`bembeyaz-png`. Signs of a problem: the book does not open, a page is missing or damaged, glyphs
are damaged or missing, a dictionary definition does not come, a hang or a restart; `Inflate
failed` in the log for an intact file.

- [ ] Delete the cache of a few books (Delete Book Cache in the reading menu while the book is
  open), open them and move between chapters: pages, table of contents, styles and pictures must
  be as before. The `Time to parse and build pages` time in the log must not be longer than with
  the previous version (in the emulator it stayed the same).
- [ ] Copy the `inflate.epub` made with `python3 emu/make_inflate_epub.py emu/books/inflate` to
  the card and open it: the stored chapter and the ~150 KB level-9 chapter must look the same as
  the screenshots in the output of
  `emu/compare.sh emu/inflate.txt x4pro emu/books/inflate/inflate.epub`
  (`emu/runs/inflate-inflate-*/report.html`).
- [ ] Reading with the built-in fonts (different sizes, bold/italic) and with a downloaded font:
  glyphs must be drawn without damage, and page turns must be as fast as before.
- [x] Dictionary (.dict.dz): look up a few words, and also a word with a long definition; the
  definitions must come in full.
- [x] KOReader sync: sending and receiving the position must work.
- [ ] A damaged EPUB: in a copy of a book, change a few bytes in the middle of the file with a hex
  editor (keep the ZIP directory at the end of the file intact; a file cut in half never reaches
  inflate) and open the chapters: there must be no hang or restart; the damaged chapter must
  either give an error (`Inflate failed: ...` in the log) or show damaged text. The ROM's miniz
  treats some damaged data more loosely than our own decoder (e.g. a distance that reaches before
  the start of the stream is read as 0).
- [ ] With a BLE remote connected, open a book whose cache was deleted, and look up a word in the
  dictionary: the remote must stay connected, and page turns must work (the decoder state grew
  from ~4.5 KB to ~11 KB; it lives in the borrowed framebuffer or in PSRAM).

## Reading the SD card font header in Rust ("rust: our own .cpfont header reader")

When a `.cpfont` font on the card is opened, one part reads its header and the styles' table of
contents, checks the count limits and the interval table, and works out the positions of the
sections in the file and the content digest (the font ID in the section cache is computed from
it). This part is now in the `cpfont` module of `bembeyaz-font`; file reading, memory allocation,
table sharing between styles and the glyph, kerning and ligature readers stayed in C++. 920 host
tests pass, including the SD font tests. During development, a diff test compared the old C++
code with the Rust code on 60,000 generated font files (some damaged or cut), on four real fonts
(Liberation Serif with four styles, Noto Sans Latin/Greek/Cyrillic, Noto Serif ASCII, the 10 MB
Noto Sans CJK) and on 9,200 damaged copies of them: the results were the same. In the emulator, a
book read with an SD font, and the fallback from a damaged font to the built-in font, looked the
same as with unmodified CrossPoint.

- [x] Choose a font downloaded from Settings > Manage Fonts, or placed under `/.fonts` (or
  `/fonts`), as the reading font in Text Settings > Font, and open a book: the text must show in
  that font, and bold and italic parts in the right style.
- [x] Open a book with this font, close it, restart the device and open the book again: it must
  open at once, without the chapters being built again (`Parameters do not match` must not show
  in the log). If the device comes from a version older than 5f321eb, the book cache is built
  again once at the first open after the install, because of the `BOOK_CACHE_VERSION` increase.
- [ ] If you have a Chinese, Japanese or Korean SD font, open a book with it: the characters must
  look as before.

## The SD card font's kerning in Rust ("rust: our own .cpfont kern reader")

While you read with a font on the card, one part builds, for each page, a mini kerning matrix with
only that page's letter pairs: it reads the class tables in blocks of 64 entries, keeps a block
index that skips unneeded blocks on later pages, renumbers the classes and reads the matrix rows.
This part is now in the `kern` module of `bembeyaz-font`. Opening and reading the file (the read
and seek callbacks), allocating memory for the tables, the ligature reader (it copies the table
without parsing it) and the glyph readers stayed in C++. 926 host tests pass, including six new
tests (0bd8c048) for damaged kerning and ligature data. During development, a diff test compared
the old C++ code with the Rust code in 120,000 generated cases (random, some with damaged, cut or
out-of-order class tables; memory allocation errors) and on 7,470 pages of four real fonts: the
results, the tables and the order of file reads and seeks were the same. In the emulator, the
pages read with an SD font and the kerning lines in the log were the same as with unmodified
CrossPoint.

- [ ] Choose a font on the card as the reading font (the Liberation Serif `.cpfont` can be made
  with the SD card font command in [rust/README.md](../README.md)). On the same page, compare the
  pairs "To"/"Ta" with "Th", and "AV"/"AT" with "AN": the arm of the T must reach over the o and
  the a, and A and V must come closer to each other; "Th" and "AN" must keep normal spacing, and
  letters must not overlap. Do this on the first page the book opens on, a few pages later and
  after going back; at a bold place and at an italic place; and on a page with Turkish letters
  ("Tü", "Yı"). Change the font size and go back, then look again.
- [ ] With the development build (`pio run -e x4pro`; release builds do not write these lines)
  and the serial log open (`pio device monitor`), go a few pages forward in a chapter, then back:
  as you turn pages, `[SDCF] Built mini kern: N×M matrix` lines must show;
  `Failed to read kern classes`, `Failed to allocate mini kern`, `Failed to seek to kern row` or
  `Failed to read kern row` must not show.

## SD font glyphs with a short bitmap are drawn empty ("fix: draw nothing for .cpfont glyphs …")

If a `.cpfont` glyph declares fewer bitmap bytes than `width × height` pixels, the renderer read
past the end of the bitmap and drew other bytes from memory on the screen (in a damaged or
malicious font). Such a glyph is now drawn empty; its advance width is kept, so the page layout
does not change. None of the 75,825 glyphs of the four real fonts made by the converter is in this
state, so nothing changes for intact fonts. 927 host tests pass, including one new test. In the
emulator, with a font whose "e" glyph was damaged, the old version drew striped smudges instead of
"e" and this version left a space; with an intact font the pages are the same as with unmodified
CrossPoint.

- [x] Read a few pages with an intact font on the card: letters must show in full (no letter may
  be empty).

## The SD card font's glyph readers in Rust ("rust: our own .cpfont glyph readers")

The following is now in the `glyph` module of `bembeyaz-font`: the part that decodes the glyph
records of a font on the card (including drawing a glyph whose bitmap is shorter than its size as
empty), the reading of these records in three places (the page cache, the advance width table for
layout, and loading on the fly a glyph that is not in the page cache), and the reading of page
bitmaps into a buffer. File reading, memory allocation and buffer policy, sorting the order of
reads, the bitmap read of a glyph loaded on the fly and the code point lookup stayed in C++. 931
host tests pass, including four new tests (e3faa164) for a glyph table and bitmaps cut in half.
During development, a diff test compared the old C++ code with the Rust code on 20,000 generated
fonts (some with damaged sizes, a short bitmap or a cut file; memory allocation errors were
simulated too) in 160,240 steps in total, and also on four real fonts: the return values, glyph
metrics, bitmap bytes, advance widths and the order of reads and seeks in the file were the same.
In the emulator, the pages read with intact and damaged fonts and the SD font log lines were the
same as with unmodified CrossPoint; with the short-bitmap font the screens were the same pixel for
pixel as with the previous version.

- [x] With a font on the card, open a book that was not opened with this font before, or change
  the font size and go back (the chapter is laid out again); go a few pages forward and back:
  letters, bold and italic parts must show in full, words must not overlap, and lines must not go
  past the right edge.
- [ ] Make an SD font with CJK characters (e.g. Noto Sans CJK) the reading font, open a Chinese
  or Japanese book for the first time and move ahead a few chapters: the serial log of the
  development build (`pio run -e x4pro`) must show `[SDCF] Overflow: loaded U+…` lines (glyphs
  that are not in the page cache are loaded this way); the characters must show in full, with no
  empty boxes or �.
- [ ] The serial log must not show `Prewarm: failed`, `short glyph read`, `short bitmap read`,
  `Overflow: failed` or `buildAdvanceTable: failed` lines.

## The SD card font's interval table in Rust ("rust: our own .cpfont interval tables")

When a font on the card is loaded, each style's interval table (which character maps to which
glyph) is now read and checked in the `intervals` module of `bembeyaz-font`. When possible, the
table is converted to the compact 16-bit form; styles with the same intervals share one table.
The part that looks up a character's glyph in this table is in the same module too. The table's
memory (PSRAM) is allocated in C++ as before. 934 host tests pass, including two new tests
(19d1638d), for two styles with the same intervals that do not fit the compact form because one
of them has more than 65,535 glyphs and for styles with an empty table, and a test that checks
that the table memory is freed when the second read gets an SD error. During development, a diff
test compared the old C++ code with the Rust code. On loading, on 60,000 generated fonts, four
real fonts and damaged copies of them, with simulated memory allocation and read errors, the
tables, the sharing and the order of reads in the file were the same; in the kerning reader the
results were the same in 127,470 setups, and in the glyph readers in 160,240 steps. In the
emulator, the screens and the SD font log lines with intact, damaged and short-bitmap fonts were
the same as with the previous version.

- [ ] Choose the Liberation Serif `.cpfont` (made with the command in
  [rust/README.md](../README.md)) as the reading font, or open a book with it: the serial log of
  the development build (`pio run -e x4pro`) must show
  `Style 1: sharing style 0's 28-interval table`, `Style 2: …` and `Style 3: …` lines; in bold,
  italic and bold-italic places the letters must be right and complete, and no wrong character
  may show.
- [ ] With a CJK font, read a few pages in a Chinese or Japanese book: the characters must show
  complete and right.
- [ ] The serial log must not show `invalid interval`, `Failed to read interval`,
  `Failed to read compact interval`, `Failed to allocate … intervals` or
  `Failed to seek … intervals` lines.

## The SD card font's page set in Rust ("rust: our own .cpfont page codepoint set")

The data logic that decides which glyphs to keep in memory before a page is drawn is now in the
`page` module of `bembeyaz-font`: collecting the unique code points from the page text (at most
512, including U+FFFD and the output code points of ligatures whose two parts are both on the
page), checking whether the page in memory already covers the new request, merging the two,
mapping code points to glyphs, and building the mini interval table. Sorting, memory allocation
and buffer policy stayed in C++. 939 host tests pass, including four new pin tests (1124c056) and
a code point order test. During development, a diff test compared the old C++ code with the Rust
code on 20,000 generated fonts (with ligatures, and with pages that have more than 512 code points
or a lot of text) in 160,152 steps in total, and also on four real fonts: the return values, glyph
metrics, bitmap bytes, advance widths and the order of reads and seeks in the file were the same.
In the emulator, the screens and the SD font log lines with intact, damaged and short-bitmap font
cards were the same as with the previous version.

- [ ] Choose a font on the card as the reading font, go a few pages forward and back in a Latin
  book, and open a page again: letters must show in full; in the serial log of the development
  build (`pio run -e x4pro`), `[SDCF] Overflow: loaded U+…` lines must not increase compared with
  the previous version (the page's glyphs are loaded beforehand), and `Prewarm: failed` or
  `Failed to allocate mapping array` lines must not show.
- [ ] Choose an SD font with ligatures (e.g. Literata in sd-fonts.yaml; the Liberation Serif
  `.cpfont` in [rust/README.md](../README.md) has no ligatures) as the reading font and open a page
  with "fi" and "fl": the log must show `[SDCF] Ligatures loaded: N` (N > 0) and must not show
  `Overflow: loaded U+FB01` or `U+FB02`; the ligatures must be drawn as one glyph.
- [ ] Choose an SD family that covers Greek and has a 12 pt size (e.g. Literata) as the reading
  font, open the chapter list of a book with Greek chapter titles and scroll down (the list draws
  the letters that are not in the built-in font with this font): the rows must show in full, with
  no empty boxes.

## The SD card font's advance width table in Rust ("rust: our own .cpfont advance table")

While a chapter is laid out, a table is kept so that the width of each letter is not read from
the SD card again and again. The logic of this table is now in the `advance` module of
`bembeyaz-font`: collecting the unique code points from the words (including the capital forms of
letters in small-caps text, space and hyphen), looking up in the table, giving letters that are
not in the font the width of U+FFFD, and limited merging. The 4096 and 768 limits, the table's
memory, growing it and the sorting stayed in C++. The small-caps mapping (`utf8SmallCapsUpper`)
is now also only in Rust; a test checks that it matches the old C++ over the whole Unicode range.
943 host tests pass, including three new pin tests (77ce50f5) and this mapping test. During
development, a diff test compared the old C++ code with the Rust code on 20,000 generated fonts
(with packed words, small caps, space, hyphen, extra text, and texts with more than 4096 unique
code points) in 160,248 steps in total, and also on four real fonts: the return values, glyph
metrics, bitmap bytes, advance widths and the order of reads and seeks in the file were the same.
In the emulator, the screens and the SD font log lines were the same as with unmodified
CrossPoint; the chapter layout time did not change (2,508 ms in the previous version, 2,512 ms in
this one).

- [x] Choose a font on the card as the reading font, and open a book that was not opened with this
  font before, or change the font size and go back to the book (the chapter is laid out again):
  words must not overlap, lines must not go past the right edge, and the spaces in justified lines
  must be as before.
- [ ] Copy the `css-test.epub` made with `python3 emu/make_css_epub.py emu/books/css` to the card;
  with Embedded Style on, choose a font on the card as the reading font and open the book: the
  small-caps heading "Stillerle düzen" must not overflow, and letters must not overlap ("i"
  becomes "I" as before, not "İ").
- [ ] The `Time to parse and build pages` time in the serial log of the development build
  (`pio run -e x4pro`) must not be clearly longer than with the previous version;
  `buildAdvanceTable: unique codepoint cap`, `failed to allocate mappings` or
  `mergeIntoAdvanceTable: alloc failed` lines must not show.

## Built-in compressed fonts in Rust ("rust: our own compressed font page cache")

`FontDecompressor` decompresses the glyphs a page needs from the compressed reading fonts built
into the firmware (Noto Sans 12–18 pt; the UI's Noto Sans 8 and Ubuntu fonts are not compressed
and do not go this way). Its data logic is now in the `compressed` module of `bembeyaz-font`:
collecting the needed glyphs from the page text (including ligature outputs), finding the groups
that hold them, the place of each glyph in the decompressed group, removing the row padding from
a bitmap whose rows are aligned to bytes, and building and searching the page table. Memory
allocation, decompression (tinfl in the ROM), the page slots and the on-the-fly group cache stayed
in C++. 950 host tests pass, including seven new tests (44073139) for the page cache, fonts with a
`glyphToGroup` table, ligatures, the slot limit, a damaged group, a glyph that is in no group, and
an uncompressed font. During development, a diff test compared the old C++ code with the Rust
code on the firmware's 21 built-in fonts (16 of them compressed), on copies of the compressed ones
with a `glyphToGroup` table (same groups) and on damaged copies, with simulated memory allocation
errors, in 129,526 steps: the return values, bitmaps and statistics were the same. In the
emulator, the reading screens were the same as unmodified CrossPoint's, and the settings screens
the same as the previous version's; the page drawing time was 2–3 ms longer than in the previous
version (1,258–1,260 ms instead of 1,256–1,257 ms, ~0.2%; the difference is in the drawing
passes).

- [ ] With Reader Font Family set to Noto Sans in Settings, turn pages in a few books at all four
  sizes (12, 14, 16, 18): letters and bold, italic and bold italic parts must be complete; in an
  English book the "fi", "fl", "ff", "ffi" ligatures must look right; letters must not shift or
  show lines.
- [ ] While reading, open the "Go to %" window, and in Settings open a range selection window: the
  large heading (Noto Sans 18) must look right. The font preview in Settings, the book's Details
  screen and a dictionary definition must also look right.
- [ ] The serial log of the development build (`pio run -e x4pro`) must not show
  `[FDC] Decompression failed`, `[FDC] Glyph … not found in any group`, `[FDC] Failed to allocate`,
  `page buffer slots full` or `Inflate state OOM` lines.
