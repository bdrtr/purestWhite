# Gradual move to Rust

CrossPoint stays a working firmware; modules move to Rust one at a time.
At every step the device boots, the host tests pass, and the user can go back.

## Method

1. A C++ module is chosen (from the bottom of the dependency graph upward).
2. Its logic is written under `rust/crates/bembeyaz-<module>` as a hardware-independent
   `no_std` crate, and unit tests are added with `cargo test`.
3. `bembeyaz-ffi` exports it as `bb_<module>_*` C functions;
   their declarations are in `rust/include/bembeyaz/<module>.h`.
4. The public API of the C++ header **does not change**; the `.cpp` file becomes a thin shim that
   calls Rust. So the calling code stays untouched.
   New crates written for the HAL (stage 6) are named `purestwhite-<module>`, their C ABIs
   `pw_<module>_*` and their headers `rust/include/purestwhite/<module>.h`; the existing
   `bembeyaz-*` crates keep their names. All of them are still exported from the single
   staticlib `bembeyaz-ffi`.
5. The existing host tests (`test/`, gtest) now test Rust. If the behavior differs,
   it is caught here. When the tests pass, the old C++ body is deleted.

The shims go away as their callers move to Rust. In the end `main` moves to Rust
and the Arduino layer goes away.

## Build

- Firmware: `pio run -e x4pro` (or `x4c`). `scripts/build_rust.py` builds `bembeyaz-ffi` for
  the board's MCU and links it:
  - ESP32-S3 → `cargo +esp ... --target xtensa-esp32s3-none-elf -Zbuild-std=core,alloc`
  - ESP32-C3 → `riscv32imc-unknown-none-elf` (stable). There is no env for this board any more;
    the line stays as a template for a future board with a RISC-V chip.
- Host tests:
  `cmake -S test -B build/test && cmake --build build/test && ctest --test-dir build/test`
  CMake builds the Rust library for the host and links it into every test.
- Rust only: `cd rust && cargo test`
- The bare-metal firmware's application on the PC, as PNG files: see
  [Run in the host simulator](firmware/README.md#run-in-the-host-simulator).

## Checking on the emulator

[eink-qemu](https://github.com/serialx/eink-qemu) emulates the X4 Pro (display, SDMMC,
GT911, CW2017, deep sleep). `emu/compare.sh` runs the unmodified
CrossPoint build (the `../crosspoint-base` worktree) and this tree's build
side by side with the same scenario: screenshot differences, heap, render times.

```sh
pio run -e x4pro && emu/compare.sh            # the emu/reading.txt scenario
emu/compare.sh emu/other-scenario.txt x4pro book.epub

# Inflate: a PNG cover (zlib stream, bembeyaz-png), stored and level-9 chapters (InflateStream,
# the miniz in ROM)
python3 emu/make_inflate_epub.py emu/books/inflate
emu/compare.sh emu/inflate.txt x4pro emu/books/inflate/inflate.epub

# Right-to-left text (Arabic, Hebrew). The built-in fonts do not have these letters: the body
# is drawn as boxes, and the comparison measures the line layout and the title.
python3 emu/make_bidi_epub.py emu/books/bidi
emu/compare.sh emu/reading.txt x4pro emu/books/bidi/bidi-test.epub

# CSS: alignment, indent, margins, bold/italic, small caps, borders
python3 emu/make_css_epub.py emu/books/css
emu/compare.sh emu/reading.txt x4pro emu/books/css/css-test.epub

# Hyphenation: German compound words, hyphens, apostrophes; the card comes with the setting that
# turns hyphenation on
python3 emu/make_hyphenation_epub.py emu/books/hyphenation
emu/compare.sh emu/reading.txt x4pro emu/books/hyphenation/hyphenation-test.epub \
    emu/books/hyphenation/card/.crosspoint=/

# XML: container, package document, NCX and nav; HTML entities, CDATA, a table, UTF-16,
# ISO-8859-1, an internal DTD and a paragraph longer than 32 KB. The broken twin must give the
# same errors when it opens; the entity twin's first chapter and nav document use HTML entities
# without a DTD (in the old version the chapter does not open, and NCX is read instead of nav).
python3 emu/make_xml_epub.py emu/books/xml
emu/compare.sh emu/xml.txt x4pro emu/books/xml/xml-test.epub
emu/compare.sh emu/open-book.txt x4pro emu/books/xml/broken/xml-broken.epub
emu/compare.sh emu/open-book.txt x4pro emu/books/xml/entities/xml-entities.epub

# Chapter pictures (bembeyaz-png): every color type and bit depth that the converter decodes,
# transparency, a large picture that is scaled down; a PNG cover (the thumbnail on Home, the
# `cover` reader). The broken twin's picture is cut 10 bytes before the end of its data: the old
# version hangs while opening it.
python3 emu/make_png_epub.py emu/books/png
emu/compare.sh emu/png.txt x4pro emu/books/png/png.epub
emu/compare.sh emu/png-broken.txt x4pro emu/books/png/png-broken.epub

# BMP (bembeyaz-bmp): 1, 2, 4, 8, 24 and 32-bit pictures in the image viewer, bottom-up and
# top-down; the last one's pixel data is cut in the middle (both versions fail after 199 rows,
# `row 199` in the log; the rows below are drawn).
python3 emu/make_bmp_files.py emu/books/bmp
emu/compare.sh emu/bmp.txt x4pro emu/books/bmp/*.bmp
# Transparent sleep overlay (/sleep-overlay.bmp, 32-bit BGRA): holds Power down to put the
# device to sleep.
emu/compare.sh emu/sleep.txt x4pro emu/books/bmp/overlay/card/sleep-overlay.bmp \
    emu/books/bmp/overlay/card/.crosspoint=/

# XTC/XTCH (bembeyaz-xtc): two books, 1-bit and 2-bit, are opened; two pages are turned in each.
python3 emu/make_xtc_files.py emu/books/xtc
emu/compare.sh emu/xtc.txt x4pro emu/books/xtc/book.xtc emu/books/xtc/book.xtch

# SD card font (bembeyaz-font): the default book is read with a .cpfont font on the card.
# lib/EpdFont/scripts/fontconvert_sdcard.py makes the font (it needs freetype-py); here with the
# four styles of Liberation Serif. In the broken twin, the first style's second interval overlaps
# the first: both versions reject the font (`invalid interval layout at 1` in the log) and read
# with the built-in font. This font does not have the book's Korean letters: on the good card,
# the same `No glyph` lines appear in the logs of both versions.
f=/usr/share/fonts/liberation/LiberationSerif
python3 lib/EpdFont/scripts/fontconvert_sdcard.py --intervals latin-ext --size 14 \
    --regular $f-Regular.ttf --bold $f-Bold.ttf --italic $f-Italic.ttf \
    --bolditalic $f-BoldItalic.ttf -o emu/books/LiberationSerif_14.cpfont
python3 emu/make_sd_font_cards.py emu/books/LiberationSerif_14.cpfont LiberationSerif 14 emu/books/sdfont
book="$(python3 emu/make_utf8_epub.py emu/books)"
emu/compare.sh emu/reading.txt x4pro "$book" \
    emu/books/sdfont/good/.fonts=/ emu/books/sdfont/good/.crosspoint=/
emu/compare.sh emu/reading.txt x4pro "$book" \
    emu/books/sdfont/bad/.fonts=/ emu/books/sdfont/bad/.crosspoint=/
# In the short-bitmap twin, the regular style's "e" glyph declares a 1-byte bitmap: the old
# version draws the bytes past the bitmap instead of "e", and this version leaves "e" empty (the
# page screens differ on purpose).
emu/compare.sh emu/reading.txt x4pro "$book" \
    emu/books/sdfont/short/.fonts=/ emu/books/sdfont/short/.crosspoint=/

# JPEG (bembeyaz-jpeg): grayscale, 4:4:4/4:2:2/4:2:0, progressive, restart, CMYK, large pictures
# decoded at 1/2, 1/4 and 1/8 scale; a JPEG cover (the thumbnail on Home, decoded at full size).
# In the broken twin's picture, a header segment runs past the end of the file: the old version
# hangs while opening it.
python3 emu/make_jpeg_epub.py emu/books/jpeg
emu/compare.sh emu/jpeg.txt x4pro emu/books/jpeg/jpeg.epub
emu/compare.sh emu/jpeg-broken.txt x4pro emu/books/jpeg/jpeg-broken.epub

# Broken EPUBs (each on its own card):
python3 emu/make_bad_epubs.py emu/books/bad
emu/compare.sh emu/open-book.txt x4pro emu/books/bad/max-size-cover/max-size-cover.epub
```

For a port that keeps the behavior, the expected result is "N of N screenshots
identical". If an intentional fix produces a difference, the commit message
explains that difference.

Changes that wait to be tried on the device collect as hardware tests, by subject, under
`rust/hardware-tests/` (see [hardware-tests/README.md](hardware-tests/README.md)); every commit
adds its own item.

## Rules

- **We use what ESP-IDF offers; we write our own when it brings a benefit.** If ESP-IDF or the
  S3 ROM already offers a job (e.g. the miniz inflate in ROM: `InflateStream`), we use it; we
  write our own code only when there is a measured benefit (speed, memory, behavior), and we give
  the reason. Example: the PNG decoders stay on `bembeyaz-inflate`, because they decode rows
  straight from the inflate window without copying them, and the miniz 1.15 in the ROM does not
  have the checks that give the same result as PNGdec (zlib) on broken input (rejecting a
  distance that reaches before the start of the stream, and a block with no end-of-block code:
  `e734c19`, `b109503`). In the Rust code linked into the firmware (the dependencies of
  `bembeyaz-ffi`) there are no third-party crates for algorithms: the algorithms we write
  ourselves are written from the standard (UAX #9, RFC 1951 …) to be as small and as fast as
  possible. The hardware layer, however, uses the ecosystem's interfaces: drivers are written
  with the `embedded-hal` and `embedded-hal-async` traits (these are traits only and bring no
  code); only these trait crates are linked into the ESP-IDF firmware through `bembeyaz-ffi`,
  while esp-hal and Embassy go only into the separate bare-metal firmware (ESP-IDF owns the
  peripherals and the interrupts). In the bare-metal firmware too, the algorithms outside the
  esp-hal and Embassy family (drivers, memory allocator, executor) are ours (the FAT file system
  `purestwhite-fat`); the `sdio` crate for the SD card's protocol, which esp-hal also uses, is
  from that family's driver layer. Direct dependencies are pinned to an exact version
  (`=1.0.0`), the others by `Cargo.lock`; so the firmware build needs
  crates.io (or cargo's cache). The crates used as references in tests (`zip`; and
  miniz_oxide, which compresses the test data and is the decoder that takes the place of
  `bembeyaz-inflate` in the comparison test) will also be replaced over time with fixtures that
  we make ourselves.
- **A port first keeps the old behavior.** Oracle tests written against the old code
  pass unchanged; departures from the standard are fixed later, each in its own commit.
  The chapter parser's oracle is `test/chapter_corpus`: 1600 random chapter and style files go
  through `parseAndBuildPages`, and the bytes of every page, the arguments of the page callback
  and the anchors are compared with digests recorded against the parser as it was before the
  port started. Because it does not depend on the parser's inner structure, it must pass
  unchanged at every slice of the port. If a digest does not match, `CHAPTER_CORPUS_DUMP=1`
  prints a digest per case (compare the output of the two trees to find the first case that
  differs), and `CHAPTER_CORPUS_CASE=n` writes that case's chapter.
  Picture sizes are read from the PNG and JPEG headers that the corpus's `Epub` stub gives; there
  is no decoder, and a picture whose header cannot be read falls back to its alt text.
- **The Rust side is not a word-for-word translation.** Crates use slices, iterators, `Option`
  and `Result`; there are no NUL-terminated strings, no special values like -1 and no silent
  errors. The FFI layer and the shim carry the C form of the boundary (C strings, `int` return
  codes). There are five exceptions:
  `bembeyaz-layout`: because the renderer and `TextBlock` take C strings, it keeps, measures and
  gives words in NUL-terminated form. `bembeyaz-json`: a token stays in the parser's buffer and
  the SDK callers read it as a C string, so a NUL follows it; `Token::bytes()` gives the view
  without the NUL. `bembeyaz-html`: entity values sit in 4-byte zero-padded fields, and a NUL
  follows them for `lookupHtmlEntity`, which returns a C string; `Value::text()` gives them
  without the NUL. The `locate` module of `bembeyaz-kosync`: to give exactly the same result as
  the scanner of `ProgressMapper`, it carries that scanner's fields as they are (-1 values for
  "absent", 12-byte NUL-padded tag names). `bembeyaz-chapter`: anchor names, footnotes, link
  targets and the words given to the C++ side end with a NUL, because `TextBlock` and `Page`
  take them as C strings; the C++ side's answer "the font does not have this size" is also 0.
  The API of `bembeyaz-jpeg` follows this rule, but its inner structure follows JPEGDEC function
  by function and keeps its memory layout (reads that run from the read buffer into the Huffman
  tables, pixels written over the coefficients): even on broken files, the oracle decides the
  output. Changing this layout can change the output on broken files; such a change is made in
  its own commit.
- **Bugs that we find become separate commits.** Each fix comes with a test that catches
  the bug, and the fix shows as an intentional difference in the emulator comparison.
- `cargo clippy --all-targets` (`clippy::pedantic` in the crates) must stay free of warnings.
  When `purestwhite-*` crates or `rust/firmware` change, also run
  `cd rust/firmware; cargo clippy --release -- -D warnings`. Every `purestwhite-*` crate and
  `rust/firmware` set `too-many-lines-threshold = 50` (code lines only) in their clippy.toml: no
  god functions; a file that grows past one job is split into submodules. The `bembeyaz-*` crates
  keep clippy's default limit (100) on purpose: they follow the structure of the C++ they replace,
  function for function, so that the two can be read side by side. When that C++ is deleted, the
  crate is split to the 50-line limit too.
- A single Rust staticlib is linked into the firmware (`bembeyaz-ffi`). A new module
  does not become a separate staticlib; it is added to `bembeyaz-ffi` as a dependency.
- The Rust side uses the C++ heap (`device.rs`: `malloc`). A block that needs an alignment wider
  than 4 bytes is cut from a `malloc` block that is `align` bytes larger; so every block lands
  where `malloc` chooses: on the S3, a request over 4096 bytes goes to PSRAM first
  (`lib/BlePageTurner/src/RadioPolicy.h`).
  A panic calls `abort()`; it produces the normal backtrace and core dump.
- Data at the boundary: a slice (`ptr` + `len`) goes in; output is either written into the
  caller's buffer or appended to a `std::string` (or a `std::vector`) through an `emit`
  callback; a value that Rust holds can also be read as a view that stays valid until the next
  call (`bb_opf_field`; C++ copies it at once). Rust does not return memory that C++ would
  release with `free`; when Rust returns an opaque pointer, C++ gives it back to Rust
  (`*_free`).
- A Rust value that keeps state between calls either sits in opaque memory that C++ provides
  (`bb_css_rules`), or sits in Rust's heap and C++ owns it through a pointer
  (`bb_xml_new`/`bb_xml_free`). The first is for a value that has a fixed size and must sit
  inside a C++ object or in a given piece of memory; the second is for state that is Rust's
  internal business and grows. In both cases the heap blocks that Rust holds belong to Rust: the
  value is set up once with `*_init` (`*_new` in Rust's heap), `*_free` is called before its
  lifetime ends, and it is not copied (the C++ class cannot be copied or moved). State that holds
  no heap (`bb_css_tokenizer`) needs no release; the Rust side checks this at compile time with
  `needs_drop`.
- The crates that handle untrusted input (`bembeyaz-json`, `bembeyaz-toc`, `bembeyaz-zip`,
  `bembeyaz-inflate`, `bembeyaz-png`, `bembeyaz-jpeg`, `bembeyaz-bmp`, `bembeyaz-font`,
  `bembeyaz-xtc`, `bembeyaz-kosync`, `bembeyaz-library`, `bembeyaz-bidi`, `bembeyaz-css`,
  `bembeyaz-hyphenation`, `bembeyaz-xml`, `bembeyaz-opf`, `bembeyaz-path`, `bembeyaz-layout`,
  `bembeyaz-section`, `bembeyaz-chapter`, `bembeyaz-book`, `bembeyaz-clippings`, `bembeyaz-io`,
  `bembeyaz-gfx`, `bembeyaz-image`, `bembeyaz-dict`) forbid `clippy::indexing_slicing`,
  `unwrap_used` and `panic`; they do not allocate unbounded memory from a size read from a file
  (an allocation is capped by a fixed upper limit or by the size of input already in memory).
  Each carries a mini-fuzz test that checks there is no panic on truncation and on single-byte
  corruption. The same three lints are denied in `bembeyaz-html`, `bembeyaz-dither` and every
  `purestwhite-*` crate; of those, `purestwhite-fat` reads untrusted data (the SD card) and is
  tested on damaged volume images instead.
- A large value (e.g. the ~4.5 KB inflate state) must be built in place: Rust does not
  guarantee this, and the value may first be built on the stack and then copied.
  `scripts/build_rust.py` measures the stack frames of the Rust functions in every firmware
  build (`pio run`) and rejects any frame over 2 KB.
- Integers, pointers and `f32` pass as function arguments and return values; structs pass only
  in memory, with their layout checked on both sides (`CssStyle`). That a `float` passed by
  value goes the same way in both compilers on Xtensa was confirmed in the firmware's
  disassembly with `bb_css_word_scale` and `bb_css_block_scale` (now deleted): GCC carries the
  argument and the return value in general registers (the caller's `a10` and `a12`), and Rust
  takes and gives them from the same registers with `wfr`/`rfr`. The other direction (the
  chapter parser's `line_height` and `font_variant` callbacks) is also the same in the
  disassembly: Rust puts the `float` into the caller's `a12` with `rfr`, and C++ takes it from
  its own `a4`.

## Porting order

By dependencies (leaves first):

| Stage | Modules | Note |
| --- | --- | --- |
| 1 — plain text | UTF-8, XML parser, MiniBidi, JSON parser: [1-text.md](porting/1-text.md) | No hardware, has tests |
| 2 — compression/data | ZipFile, InflateStream, Serialization: [2-data.md](porting/2-data.md) | Borrowing the framebuffer and the inflate call are in C++ (`InflateStream`). ContentProtection (SDK) uses its own C copy of miniz |
| 3 — EPUB core | Epub (book metadata cache, TOC, CSS, OPF), the chapter parser, Section: [3-epub.md](porting/3-epub.md) | The largest part (~19k lines); submodule by submodule |
| 4 — images/fonts | PNG, JPEG and BMP decoders, dithering, fonts, GfxRenderer: [4-images-fonts.md](porting/4-images-fonts.md) | Framebuffer boundary |
| 5 — application | Web paths, LibraryIndex, Txt, Xtc, KOReaderSync, ClippingStore, Dictionary: [5-application.md](porting/5-application.md) | UI |
| 6 — platform | HAL, FreeInk SDK drivers, `main` → bare-metal Rust (esp-hal + Embassy): [6-platform.md](porting/6-platform.md) | Arduino goes away |
