# Device test plan for version 0.0.3

At release 0.0.3, the hardware tests had 255 unticked items (they were then one file,
`rust/DONANIM-TESTLERI.md`; they are now the subject files in this folder). This plan splits them
into groups that can be tried on the device in the same session; each group names the
hardware-test sections that its items come from. When an item is tried, it is ticked (`[x]`) in
its subject file under `rust/hardware-tests/`; if a problem shows up, write a note under that
item. The items that need a time measurement are at the end, in a separate session.

The groups are in order of importance: the first four cover all of daily use; if problems show
up, they show up most there.

## Comparison with 0.0.2 in the emulator

Before the release, all emulator scenarios were run against 0.0.2 (f3776317) with
`emu/compare.sh`; the 124 screenshots of the 22 scenarios are identical: settings, BLE settings,
reading (Çiçekli Şiir, the CSS, bidirectional and hyphenation books), the XML book, opening a book,
inflate, JPEG, broken JPEG, CSS-sized JPEG, PNG, broken PNG, the BMP viewer, the transparent sleep
overlay (BMP and PNG), the dictionary (plain text and HTML/dictzip), adding and deleting
clippings, the library, XTC. What the emulator cannot see (network, BLE, real SD card speed, touch
drags, real books) is in the sessions below.

## 0. Preparation

- **Backup.** The current firmware stays in the other slot (the SD card update does not erase
  it); still, keep the latest release from `~/Documents/bembeyaz/firmware-arsiv/` on the card.
- **Flashing.** Put `purestWhite-next-0.0.3.bin` in the card's root, and choose it with Settings >
  System > SD Card Firmware Update. About must show "purestWhite next 0.0.3"; Start Other Firmware
  must show the earlier firmware by name.
- **Test card.** Copy the contents of the `~/Documents/bembeyaz/test-karti-0.0.3/` folder to the
  card's root (it has no settings file; your settings do not change):
  - `kitaplar/`: the books of the emulator scenarios (CSS, bidirectional, hyphenation, XML,
    inflate, JPEG, CSS-sized JPEG, PNG, the dictionary book, Çiçekli Şiir) and the test books from
    `test/epubs/` (tables, superscript/subscript, images, kerning…); `kitaplar/bozuk/` has broken
    files.
  - `resimler/`: 1, 2, 4, 8, 24 and 32-bit BMPs, and the cut `7-cut.bmp`.
  - `kaplama/`: transparent sleep overlays (`sleep-overlay.bmp`, `sleep-overlay.png`); when you
    test, move one of them to the root; do not have both in the root at once (the BMP is chosen
    first).
  - `xtc/`: 1-bit and 2-bit XTC books.
  - `.dictionaries/plain` and `.dictionaries/html`: plain-text and HTML (dictzip) test
    dictionaries.
  - `.fonts/LiberationSerif/`: an SD card font.
- **Your own books.** Some items need real books: Turkish and English novels, an EPUB 2 (NCX
  only), a book with footnotes, a book with tables, Japanese/Chinese (with ruby), Arabic or
  Hebrew, a large book with hundreds of chapters, one TXT and one Markdown book, a book with
  clippings. It helps if you opened some of them in the earlier version (for cache
  compatibility).
- **Serial log.** The items that need log lines need a development build (`pio run -e x4pro`) and
  `pio device monitor`; the USB connection works only through a hub, so these may wait until later
  (session 6).

## 1. Caches and first boot

Open books that were opened in the earlier version; then delete the cache of one of them and open
it again.

- Books opened with the earlier version must open without being laid out again, and the reading
  position must be kept.
- In the book whose cache was deleted, the table of contents, the chapter names and "Go to %" must
  be right.
- While a long chapter is still being laid out, leave and come back: the page where you stopped
  must open at once.
- Change the font size and go back: the chapter must be laid out again, and it must look right at
  the old size too.

Hardware-test sections: [First boot][first-boot]; [Section file tables][caches]; [Section
pages][caches]; [Book metadata cache][caches]; [A damaged cache file no longer crashes the
reader][parsing]; [container.xml][parsing]; [OPF][parsing]; [Table of contents readers][parsing];
[Table of contents label][parsing]; [Inflate in ROM][fonts] (item 1); [Compression speed][fonts].

## 2. Reading and page layout

With `kitaplar/css-test.epub` (Embedded Style on), your own novels and your book with footnotes.

- **Styles.** Alignment, indents, margins, heading sizes, bold/italic/lines, superscript and
  subscript, small caps, lists (including nested numbered lists), `pre`, `<hr>`, hidden elements;
  framed and gray boxes (in a box that runs over more than one page, the edges at the cut are
  open).
- **Drop cap.** The size, the top and baseline alignment, the indent of the lines next to it;
  change the font size and the font; a paragraph with a drop cap that falls at the end of a page.
- **Page breaks.** A first or last line left alone, and headings at the bottom of a page, must move
  to the next page; when a line with a footnote link is carried over, the footnote must be in the
  list of the right page.
- **Links.** Footnote links and internal links must go to the right place (by touch, with Touch
  Reader Controls on); moving through the "Links and footnotes" list must leave no trace.
- **Tables.** `crosspoint-table-edge-cases.epub` and `test_tables.epub`; in portrait and landscape,
  with a large font.
- **Focus Reading**, hyphenation on and off, a book with non-breaking spaces (French), TXT and
  Markdown.

Hardware-test sections: [XML parser][parsing]; [Path resolution][parsing]; [Paragraph
layout][layout]; [CSS][layout]; [Chapter text is cut into words][layout]; [Capturing the drop
cap][layout]; [Inline style stack][layout]; [Inline font size][layout]; [The size of the span that
holds the block][layout]; [Block style][layout]; [Element classification][layout]; [Lines carried
at a page break][layout]; [Table column widths][layout]; [Table row geometry][layout]; [Tag names
checked one by one][layout]; [List markers and ruby text][layout]; [Empty bytes of page
links][caches]; [Framed boxes][layout]; [Drop cap placement][layout]; [Links and footnotes carried
at a page break][layout]; [TXT and Markdown][parsing].

## 3. Languages and fonts

- Turkish, English and German books with hyphenation on; `hyphenation-test.epub`.
- Japanese/Chinese (with ruby), Korean, Arabic or Hebrew (also `bidi-test.epub`).
- The built-in fonts in four sizes, bold/italic, the "fi"/"fl" ligatures; the four Reading
  Orientations; Text Anti-Aliasing on and off.
- `.fonts/LiberationSerif` as the reading font: kerning pairs ("To", "AV"), bold/italic, no new
  layout when the book is opened again; a CJK SD font and an SD font with ligatures, if you have
  them.

Hardware-test sections: [Hyphenation][layout]; [Bidirectional text][layout]; [DEFLATE and
fonts][fonts]; the SD card font's [header, kerning, glyph readers, interval table, page set,
advance width table][fonts]; [SD font glyphs whose bitmap is shorter than their size][fonts];
[Compressed built-in fonts][fonts]; [Glyphs][drawing]; [Scaled glyphs][drawing]; [Drawing
primitives][drawing].

## 4. Images, covers and the sleep screen

- **Chapter images.** `jpeg.epub` (11 chapters), `png.epub` (13 chapters), `jpeg-sized.epub`
  (images sized with CSS), `inflate.epub`, `test_mixed_images.epub`; on the first open and from
  the cache, in Night Mode, in the four orientations. Compare how they look with the emulator
  reports (`emu/runs/jpeg-jpeg-*`, `png-png-*`, `jpeg-sized-*`).
- **Broken images.** `kitaplar/bozuk/jpeg-broken.epub` and `png-broken.epub` must open without
  freezing.
- **Image viewer.** The BMPs in `resimler/` (Night Mode on and off; `7-cut.bmp` partly), a JPEG
  and a PNG; Set Cover.
- **Covers.** Books with JPEG and PNG covers (opened for the first time): the Home thumbnails in
  all themes (Classic, Lyra, Lyra Extended, RoundedRaff, Cover Grid), in Night Mode; the icon and
  the bookmark ribbon of a book with no cover; a PNG with the same name next to a TXT.
- **Sleep screen.** Cover (Fit and Crop), Custom (`/sleep.bmp` and the `/sleep/` folder; filters),
  Transparent (the BMP and the PNG in `kaplama/`; while reading, while rotated and on Home), the
  daisy logo.

Hardware-test sections: [PNG images][images]; [JPEG images][images]; [Broken JPEG][images]; [PNG
covers][images]; [PNG cover bit depth][images]; [BMP reader][images]; [Transparent sleep
overlay][images]; [Logo][logo]; [Rounded rectangles][drawing]; [Filled polygons][drawing];
[Pictures][drawing]; [Screen region copies][drawing]; [Bottom-up pictures][images];
[Dithering][images]; [Cached image rows][images]; [Drawing JPEG and PNG images][images]; [PNG rows
to gray][images]; [Scaling cover rows][images]; [Image size probe][images]; [Samples without
division in the PNG cover reader][images]; [Image size][layout].

## 5. Dictionary, library, clippings, XTC

- **Dictionary.** In the dictionary setting, first `plain`, then `html`; in `dict-book.epub`:
  "Stories", "walked", "“zebra”", "cat’s", "colour" (synonym), "running", "quixotic" (a definition
  longer than 16 KiB) and "qwertyuiop" (not found). On the first lookup, "Indexing Dictionary..."
  must show. If you have them: a real StarDict dictionary (with HTML definitions and `.dict.dz`),
  and a dictionary that the earlier version indexed.
- **Library.** The Title/Author/Recent tabs, the letter groups, the "cicek" search, the merging of
  author names, refresh; a card with hundreds of books.
- **Clippings.** The earlier version's clippings; adding, deleting, opening again; the clipping
  sync, if you have it.
- **XTC.** The two books in `xtc/`, the chapter list, the cover thumbnail.

Hardware-test sections: [Dictionary index and lookups][dictionary]; [Dictionary HTML
definitions][dictionary]; [HTML to plain text][dictionary]; [Dictzip][dictionary]; [Library
text][parsing]; [Library index file][parsing]; [Library index writer][parsing]; [Clipping
store][caches]; [Clipping texts][caches]; [Clipping file][caches]; [XTC/XTCH parser][parsing];
[XTC chapter list][parsing].

## 6. Network, sync, update

With a device that is connected to Wi-Fi and has the right time.

- **Name.** "purestWhite-<MAC>" on the router, `http://purestwhite.local`, Calibre Wireless.
- **File transfer.** Upload to a subfolder, listing and download, with the web server and with
  WebDAV.
- **OPDS.** Browsing, search and download in OPDS 1 and OPDS 2 catalogs; the JPEG/PNG cover and the
  HTML description on the Details screen (a long description too); a catalog with sign-in, if you
  have one.
- **Plugin Hub, Manage Fonts, LCP** book.
- **Certificates.** The badssl.com addresses, a self-signed home server (setting on and off).
- **KOSync.** The default server, sync.koreader.rocks and `kosync.ak-team.com:3042`; Smart Sync and
  Ask Every Time; the "resumed" lines; positions in both directions with KOReader (the middle of a
  paragraph, a list item, the end of a chapter, a chapter with a large image).
- **Update.** After 0.0.3 is published on GitHub, Check for Updates on a device that runs 0.0.2
  must offer 0.0.3; on the same version, it must not offer it; when Wi-Fi is cut during the
  download, the old version must start.

Hardware-test sections: [Name][name]; [XML parser][parsing] (the OPDS, WebDAV and KOReader items);
[Path resolution][parsing] (WebDAV); [Server certificate checks][certificates]; [Time
sync][time-sync]; [TLS session resumption][resumption]; [KOSync: one connection][kosync];
[Replies without a body][no-body]; [Update over Wi-Fi][wifi-update]; [JSON parser][parsing]; [Long
OPDS descriptions][parsing]; [Version 0.0.2][version-002]; [KOReader chapter positions and
position finding][parsing].

## 7. BLE remotes

Pairing four remotes, the warning for a fifth; with a remote connected, opening a chapter with
PNGs and a book whose cache was deleted for the first time, a dictionary lookup.

Hardware-test sections: [BLE remotes][ble]; [PNG images][images] (the BLE item); [Aligned Rust
blocks][aligned]; [PNG covers][images] (the BLE item); [Inflate in ROM][fonts] (the BLE item).

## 8. Serial log and time measurements

With a development build and `pio device monitor`; compare with the earlier version on the same
books. Write it down if you see a clear slowdown, or one of the error lines named in the hardware
tests.

- `Time to parse and build pages` (a large book, a book with an SD font),
  `PNG/JPEG decoding complete - render time`, the cover conversion times, the `Page render:` times
  (portrait and landscape), the time to add and delete a clipping (a book with many clippings),
  the library refresh time.
- No error lines for fonts, sections, the book cache, clippings or the library.

Hardware-test sections: [DEFLATE and fonts][fonts]; [Compression speed][fonts]; [PNG and JPEG
images][images] (the speed items); [PNG covers][images] (speed); the log items of the [SD card
font sections][fonts]; [Compressed built-in fonts][fonts] (log); [Section file tables and Section
pages][caches] (log); [Book metadata cache][caches] (log); [the clipping sections][caches] (time);
[Library index writer][parsing] (time); [Glyphs][drawing] (time).

## 9. Broken files and recovery

- The books in `kitaplar/bozuk/`, and an EPUB whose middle was damaged with a hex editor: there
  must be no freeze or restart.
- Write down how you get out of the screen that freezes when Left (the SDK's Up) is held at boot;
  recovery mode with Right (the SDK's Down) + Power.

Hardware-test sections: [Compression][fonts]; [Inflate in ROM][fonts] (broken EPUB); [Two firmwares
side by side][two-firmwares].

Items that pass by themselves: the first item of "[Aligned Rust blocks][aligned]" and
"[PNG cover bit depth][images]" need nothing extra on the device; if no `Rust panic`, restart or
freeze is seen in the other sessions, they can be ticked.

[first-boot]: releases.md#first-boot
[two-firmwares]: releases.md#two-firmwares-side-by-side-feat-start-the-firmware-in-the-other-ota-slot
[name]: releases.md#name-purestwhite--purestwhite-next-version-001
[logo]: releases.md#logo-daisy
[version-002]: releases.md#version-002-release-002
[ble]: network.md#ble-remotes-598e41e-76620d4
[certificates]: network.md#server-certificate-checks-net-securenet-moves-into-the-fork-net-https-checks-server-certificates
[time-sync]: network.md#time-sync-fix-trustedtimesyncnow-reports-the-sync-it-waited-for
[resumption]: network.md#tls-session-resumption-net-secureclient-resumes-tls-sessions
[kosync]: network.md#kosync-one-connection-per-sync-net-one-kosync-connection-per-sync
[no-body]: network.md#replies-without-a-body-fix-204-304-and-head-replies-end-at-their-headers
[wifi-update]: network.md#update-over-wi-fi-our-own-release-channel-feat-update-over-wi-fi-from-bdrtrpurestwhite-releases
[aligned]: platform.md#aligned-rust-blocks-fix-aligned-rust-allocations-follow-mallocs-psram-rule
[parsing]: parsing.md
[caches]: caches.md
[layout]: layout.md
[fonts]: fonts.md
[images]: images.md
[drawing]: drawing.md
[dictionary]: dictionary.md
