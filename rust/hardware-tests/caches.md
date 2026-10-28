# Hardware tests: book caches and clippings

These tests cover the files the reader keeps on the SD card for each book: the section files and
their pages, the book metadata cache (`book.bin`) and the clipping store.

## Section file tables in Rust ("rust: our own section file tables")

While a book is read, the pages of each chapter are written to a section file in the cache on the
SD card (`sections/<n>.bin`). The code that reads this file and writes it at the end of a build is
now in the `file` module of `bembeyaz-section`. It covers the file's header (including the check
of the layout settings), the progress data of a build that was not finished, and the tables of
pages, anchors (footnote and table-of-contents targets), paragraphs, list items and visible text
positions. The format's version (60) and its history are there too. Opening and renaming the file,
running the build and the page contents stayed in C++. Up to 4eacfd40 there were no tests for
Section at all. That commit's 14 tests and this version's 5 regression tests build sections with
the real HTML parser and pin the file format and every lookup; 969 host tests pass in total. A
diff test compared the old C++ code with the Rust code on random sections and on damaged files:
both sides wrote the same file, byte for byte. All differences in reading were deliberate fixes
for damaged files. The old behavior was undefined, different on the device and on the computer,
or a hang, so this port did not keep it; it fixed each case with its own regression test:

- if the byte of one of the on/off settings is not 0 or 1, the file counts as laid out with other
  settings (the old code read it as a bool, which is undefined behavior);
- table positions close to 4 GB do not overflow and point to the start of the file;
- a page position that points inside the header is not read as a page;
- the table-of-contents lookup does not allocate and read gigabytes of memory because of a damaged
  anchor name length; it stops there (in the diff test the old code got stuck for minutes).

In the emulator, the reading and book-opening screens were the same as unmodified CrossPoint's,
and the reading screens the same as the previous version's. The page drawing time did not change
(1,259–1,260 ms instead of 1,260–1,261 ms).

- [x] Install the new version, then open a book that was opened with the previous firmware (one
  chapter fully laid out, one long chapter only partly): it must open at once, without being laid
  out again, and the log must not show `Unknown version` or `Parameters do not match`.
- [x] Open a book and read a few pages, leave the book and open it again: it must open at once, on
  the same page (the section is read from the cache).
- [ ] While reading, change the font size (Reader Font Size) from the text menu (in the Turkish UI
  this row shows as "Arayüz Yazı Boyutu" for now): the chapter must be laid out again and the pages
  must use the new size; going back to the old size must also look right.
- [ ] While a long chapter is still being laid out, leave the book and open it again: the page
  where you stopped must open at once; when you get close to the end of the saved pages, the
  remaining pages must be laid out in the background, and you must be able to read to the end of
  the chapter.
- [ ] In a book where one section file has several table-of-contents entries, jump to a
  subheading from the table of contents, and follow a footnote link in the text: both must go to
  the right place. Turn a few pages and open the table of contents again: the cursor must be on
  the subheading you are in.
- [ ] If KOReader sync is used, send the position with "Upload local progress", go to another
  chapter, open sync again and choose "Apply remote progress": it must go back to the page you
  sent.
- [ ] In the serial log of the development build (`pio run -e x4pro`), these lines must not show:
  `[SCT] Deserialization failed` (apart from `Parameters do not match`, which shows the first
  time each section is opened after a settings change), `Failed to write the section tables` or
  `Failed to write LUT due to invalid page positions`. When you leave a long chapter for the first
  time, `Suspended build: … pages persisted` must show.

## Empty bytes of page links are zeroed ("fix: zero page link hrefs in the section cache")

Each page in the section file stores the target of its internal links in a 256-byte field. The
bytes after the NUL at the end of the target were not initialized: whatever was in memory at that
moment (the text of other words, addresses) was written to the file, so the same book gave a
different file each time it was laid out. Now the field is initialized with zeros. 975 host tests
pass, including 6 new host tests (`test/page_codec`) that pin the page format byte for byte and
every validation branch; in the emulator the reading screens are the same.

- [x] In a chapter with internal links (footnote, table-of-contents link), tap the links (X4 Pro)
  or use link selection: they must go to the right place.

## Section pages in Rust ("rust: our own section pages")

The code that reads, checks and writes the body of each page in the section file (lines and their
words, ruby, line style, per-word fonts; pictures, horizontal rules, drop caps, frame boxes;
footnotes and links) is now in the `page` module of `bembeyaz-section`. C++ builds the objects;
C++ allocates the memory for the arena, ruby, word fonts, picture paths and the footnote/link
arrays; drawing stayed in C++. A page is now read through a 256-byte buffer; before, each field
was a separate `HalFile::read` call (the arena and the word font table were already one read
each). The old behavior is kept: a field where the file ends is read as zero. Deliberate
differences: an on/off byte in the line style or in a frame box that is not 0 or 1 now counts as
on (the old code read it as a bool, which is undefined behavior); when writing, any write that
fails now makes the page fail (the old code did not check the result of most writes). 5 new tests
(picture, cut picture path, three word font slots, on/off byte, blocks read one after another)
were added to the 6 tests of `test/page_codec` from b14b2167; 979 host tests pass. During
development, a one-time diff test (not kept in the tree) compared the bytes that the old and the
new code decoded and wrote back for 8,000 random and damaged pages: they were the same except on
56 pages, and on those 56 pages the only difference was the on/off bytes. In the emulator, the
reading, book-opening and PNG and JPEG picture book screens were the same as unmodified
CrossPoint's, and the reading screens the same as the previous version's. The page drawing time
(not counting loading the page from the SD card; loading was not measured) did not change
(1,257–1,258 ms instead of 1,260–1,261 ms).

- [ ] The section format did not change (version 60): after installing the new version, open a
  book that was opened with 089e7ae5 or later: the pages must look as before, without being laid
  out again; the serial log must not show `[SCT] Deserialization failed` or
  `Cache not found, building`.
- [ ] So that the new writer also runs in the steps below, delete the book's cache (Delete Book
  Cache) or start with a book that was never opened; after reading, leave the book and open it
  again.
- [x] In a book with pictures, the pages with pictures and their placeholders must be in the right
  place.
- [x] With Embedded Style on in Settings, in a book with CSS (bold, italic, underline and
  strikethrough, superscript and subscript, small caps, words in different font sizes; paragraphs
  that start with `::first-letter` or a drop cap; boxes with borders or a light background), the
  pages must look the same when the book is opened again. `<hr>` lines must show even with
  Embedded Style off.
- [ ] With Focus Reading on, in a book in Latin script, the start of each word must still be bold
  after the book is opened again.
- [ ] With an SD font that supports CJK chosen as the reading font, in a Japanese book with
  furigana (e.g. an Aozora Bunko EPUB), the ruby texts must be in place after the book is opened
  again.
- [x] In a chapter with footnotes and internal links, the links must still go to the right place
  after the book is opened again.
- [ ] The serial log of the development build (`pio run -e x4pro`) must not show
  `[PGE] Deserialization failed`, `[PGE] Serialization failed`, `[TXB] OOM` or
  `[ERS] Failed to load page from SD` lines.

## Book metadata cache in Rust ("rust: our own book metadata cache")

When a book is opened for the first time, a `book.bin` is written to the cache: title, author,
language, cover, spine items (section files and their cumulative sizes) and the table of
contents. The code that reads and writes the format of this file and of the two temporary files it
is built from is now in the `cache` module of `bembeyaz-book`. So are the cumulative size lookups
that the progress percentage is based on, and the mapping of table-of-contents entries to section
files. The files and their buffers, finding the sizes in the ZIP, the 400-item threshold, sorting
the index, the lock and NFC composition of titles stayed in C++. Reading and writing use the
buffered reader and writer of the new `bembeyaz-io` crate, which is shared with
`bembeyaz-section`. The build uses 4 KB buffers as before; reads use small buffers instead of one
file read per field. For books with 400 or more spine items, the index is now in one block of
PSRAM instead of internal RAM. This change adds no tests; the 13 tests of
`test/book_metadata_cache` pass, including the 8 format tests of 5618273f, 987 host tests in
total. During development, a one-time diff test (not kept in the tree) used the final code to
build 400 random books (including books with 400 or more spine items, text books and NFD titles)
with the old and the new code: the `book.bin` files were the same byte for byte. On damaged
copies, all but one of 10,578 reads were the same (with intermediate versions, the same was true
for 550 books). In that one read, the last record of a size window was cut at the end of the
file: the old code counted it as read on the computer (because a seek past the end of the file
was allowed); now it counts as not read, as on the device. A damaged record in the temporary files
now makes the build fail (`book.bin` is deleted); the old code allocated memory during the build
without checking the length. In the emulator, all five scenarios (reading, book opening, PNG, JPEG
and XML) were the same as both unmodified CrossPoint and the previous version; free memory grew by
about 600 bytes.

- [ ] A book opened with 5f321ebd or later must open in the new version (version 11 did not
  change): the title, author and cover must look the same; the serial log must show
  `[BMC] Loaded cache data` and must not show `[EBP] Cache not found, building` or
  `[BMC] Cache version mismatch`.
- [ ] Delete a book's cache (Delete Book Cache) and open the book: the table of contents and the
  chapter names must be right. In the reading menu, go to 50% with "Go to %": the percentage in the
  status bar must show about 50%, and the chapter you reach must be near the middle of the book in
  the table of contents.
- [ ] Open for the first time a book with 400 or more section files (count them on a computer
  with `unzip -p kitap.epub '*.opf' | grep -c '<itemref'`): the log must show
  `[BMC] Using fast index` and `Using batch size lookup`, and the table of contents must map to
  the right chapters; `[EBP] Total indexing completed in … ms` must not be clearly longer than with
  the previous firmware.
- [ ] Open a TXT and a Markdown book for the first time; the progress percentage must grow as you
  read; after you close the book and open it again, the log must not show
  `[EBP] TXT/MD cache invalid or outdated`.
- [ ] The serial log of the development build (`pio run -e x4pro`) must not show
  `[BMC] Cache file damaged`, `[BMC] Failed building book.bin`, `[BMC] Could not index`,
  `[BMC] Could not read the spine` or `[BMC] Failed writing` lines.

## Clipping store in Rust ("rust: our own clipping store")

The clippings saved while reading are kept on the card in one file per book
(`/.crosspoint/clippings/epub_….bin`): the book's title and author, the place of each clipping
(section, page, words, source position), its sync state and its text. The code that reads and
writes this file, and the code that scans the journal of IDs of deleted clippings that wait for
sync (`….bin.deleted`), is now in `bembeyaz-clippings`. Opening the files, safe replacement with
a backup and a temporary file, the clippings in memory, writing to the journal and making IDs
stayed in C++. Old version 1, 2 and 3 files are read as before and are converted to version 4 on
the first write. Writing now goes through a 128-byte buffer instead of one file write per field,
and reading takes about one file read per record. `Serialization.h`, which is no longer used, was
deleted. The previous commit pinned the file format with 7 tests (version 4 with an independent
decoder, hand-built version 1, 2 and 3 files, damaged files, the deletion journal); 1001 host
tests pass in total. During development, a one-time diff test (not kept in the tree) ran the old
and the new code in 12,000 random cases (each with one clipping file and one deletion journal):
valid and damaged files, loading, sync, deletion, changes from the remote side, write errors;
about 30,000 operations and 33,000 journal reads. Every output and every written file was the
same byte for byte; a bug put into the code on purpose was caught in the first cases. The new
`emu/clipping.txt` scenario clips a word, opens the book again (the highlight comes back),
removes the clipping with a long press and opens the book once more; this scenario and the
reading, book opening, PNG, JPEG and XML scenarios gave the same screens and the same free memory
as the previous version. On x4pro, flash shrank by 372 bytes; RAM did not change.

- [ ] Open a book with clippings saved with the previous firmware: the highlights must show in the
  same places; in the "View Clippings" list in the reading menu, each clipping's chapter and text
  preview must be right, and opening one must show its full text.
- [ ] Long-press a word and save a new clipping with Clip, close the book and open it again: the
  highlight must stay and must show in the list. Long-press the highlight to remove it; after you
  open the book again it must be gone; the texts of the other clippings must not be damaged.
- [ ] If clipping sync is set up: add a clipping and sync, then delete a synced clipping and sync
  again: the deletion must reach the server, and the clipping must not come back on the next
  sync.
- [ ] The serial log of the development build (`pio run -e x4pro`) must not show
  `[CLIP] Clipping file damaged or cut`, `[CLIP] Failed to write clipping file` or
  `[CLIP] Invalid or unreadable clipping deletion journal` lines.

## Clipping texts are copied in one go ("perf: copy clipping texts in one read and one write")

When a clipping is added or deleted, the clipping file is written again from the start, and the
old texts are copied from the old file. The copy was done in 128-byte pieces: SdFat's one-sector
cache went back and forth between the two files for each piece, so a large store was very slow.
Now each text is copied with one read and one write, through a 4 KB buffer (the size of the
longest clipping text) that is allocated from PSRAM each time the file is written again; the
buffer is allocated only while writing, and only when the old file exists. Because the SD driver
uses its own DMA buffer, PSRAM is not a problem. If there is no room in PSRAM, the copy is done
in 128-byte pieces as before: slower, but the write does not fail (a new host test pins this;
1003 tests in total). The new `emu/make_clipping_store.py` builds a store with 1000 clippings of
4 KB (about 4 MB); `emu/clipping-large.txt` adds and deletes a clipping with this store. In the
emulator, adding went down from 90.4 s to 11.1 s and deleting from 90.9 s to 11.3 s (about 8
times faster); the screenshots and free memory did not change, and in the run that adds and
deletes, the files the two firmwares left on the card were the same byte for byte. In the
12,000-case diff test of the old and the new code, the outputs were again the same byte for byte.
Most of the remaining 11 seconds are single-sector SD commands at the sector edges of unaligned
texts; to go further, the unchanged parts of the file should be copied in sector-aligned blocks
(a separate job). On x4pro, flash grew by 152 bytes; RAM did not change.

- [ ] In a book with many clippings (dozens of long clippings), add and delete a clipping; note
  the times with the previous firmware and with this one (in the emulator, with a 4 MB store, it
  got about 8 times faster). Then open the book again: the texts of the other clippings in the
  "View Clippings" list must be right.
- [ ] If clipping sync is set up, sync a book with clippings: the clippings must go and come as
  before, and the serial log must not show `[CLIP] Failed to write clipping file`.

## The clipping file is written in whole blocks ("perf: write the clipping store in whole buffers")

When the clipping file is written again, the writes to the temporary file now go through an 8 KB
`BufferedFileWriter` buffer. This is above the framework's 4 KB internal RAM limit, so malloc
tries PSRAM first; if the buffer cannot be allocated at all, the writes go directly to the file as
before (a new host test checks this too). `BufferedFileWriter` now empties its buffer only when
it is full: every write except the last is exactly 8 KB and sector-aligned, and SdFat writes
these straight to the card without using its sector cache; the cache is left for reading the old
file. The bytes written to the file are the same. The book cache (`book.bin` build) and the
library index use the same class, so they now also make their writes in full buffer size. In the
emulator, with a 4 MB store, adding went down from 11.0 s to 7.7 s and deleting from 11.3 s to
8.0 s; ~2 s of this is touch and page drawing, and the remaining ~6 s is roughly the time to read
4 MB and write 4 MB. The same 8 KB buffer with the old emptying rule gave almost no gain (10.8 s
and 11.1 s): the gain comes from aligned writes. Reading the old file aligned too would save
~0.5 s more; this was not added. The clipping file and the `book.bin` that the two firmwares left
on the card were the same byte for byte; the screenshots were the same; in the clipping store's
diff test the outputs were again the same. 1006 host tests pass in total. On x4pro, flash grew by
100 bytes; RAM did not change.

- [ ] In a book with many clippings, add and delete a clipping; note the times with the previous
  firmware and with this one (in the emulator, with a 4 MB store, adding and deleting got ~30%
  shorter). After you open the book again, the texts of the other clippings must look right.
- [ ] Open a book and choose Delete Book Cache in the reading menu; when you are back on the home
  screen, open the book again: the table of contents and the chapters must be right. Upload a new
  book to the card with File Transfer (web) and open the library (or long-press a book in the
  library's "Recent" tab and choose "Refresh library"): the new book must show. The serial log
  must not show a `[BMC] Failed building book.bin`, `[BMC] Failed writing` or `[ERR] [LIBIDX]`
  line.
