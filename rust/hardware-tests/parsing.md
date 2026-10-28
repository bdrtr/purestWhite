# Hardware tests: parsing

These tests cover the readers of book and data formats: XML, EPUB package and container files,
JSON, tables of contents, XTC/XTCH, KOReader sync paths, the Library text and index, TXT and
Markdown, and the checks on web paths. See [README.md](README.md) for how hardware tests work.

## XML parser (3cfabb3, 5f321eb)

- [ ] Open a few books from different sources (EPUB 2 and 3, Calibre output, a book from a
  publisher); the pages and the table of contents should be as before.
- [ ] An EPUB 3 book that uses `<!DOCTYPE html>` (most new books): the chapters should open, marks
  such as `&nbsp;` and `&mdash;` should show correctly, and the table of contents should be
  complete (before: "Failed to index - invalid book" or an incomplete table of contents).
- [ ] OPDS catalog: browsing, search, download.
- [ ] The file list over WebDAV.
- [x] KOReader sync: sending and receiving the position.
- [ ] KOReader sync in a book with many HTML entities such as `&nbsp;` ("fix: KOReader sync
  counts HTML entities as the chapter shows them"): send the position from the middle of a
  chapter; KOReader should open the same place; a position coming from KOReader should also land
  on the same page. Before, in chapters without a DTD the position could not be sent at all, and
  in chapters with an external DTD every entity moved the position by one character.
- [ ] A very long chapter (hundreds of pages) and a book with large tables: there should be no
  restart or freeze while opening and reading (the parser's stack use went up).

## Path resolution ("rust: path helpers")

- [ ] A book whose chapter, image or link file names contain a space, `%20` or `../`: the
  chapters should open, the images should show, and the table of contents and footnote links
  should work.
- [ ] WebDAV and the web server: upload a file to a subfolder, list it, download it.

## OPF (package document) ("rust: our own package document reader")

- [ ] A few books in the Library: the title, the author and the series name and number should
  show as before (one book with a Calibre series and one with an EPUB 3 collection would be good).
- [ ] Cover images: one book with an EPUB 2 cover (`meta name="cover"`) and one with an EPUB 3
  cover (`cover-image`); the covers should show (in the cover grid on the Home screen too).
- [ ] A large book with many chapters (hundreds of manifest items): the first opening should not
  be slower than before, and the chapter order and the table of contents should be correct. Note
  the `OPF pass completed in` time in the log.
- [ ] A book with thousands of manifest items (an anthology, a dictionary or a comic EPUB): it
  should open; `Parse error` should not appear.

## JSON parser in Rust ("rust: our own JSON reader in place of the SDK's")

The SDK's streaming JSON parser (StreamingJsonParser) now runs on `bembeyaz-json`; `lib/JsonSax`
carries the SDK's API. Its users: OPDS 2 catalogs and publication pages, OPDS authentication
documents, plugin catalogs, LCP licenses, LCP device registration and unlock responses, and the
Wi-Fi update check. On the host, a differential test feeds the SDK's C++ implementation and the
Rust one with 20,000 generated and damaged documents, in random chunks, and compares every event
and error state. In the emulator, the JSON of the update check (from a fake release server) was
read correctly and the release was installed.

- [ ] Browse an OPDS 2 (JSON) catalog, open a publication's detail page, download a book. A
  description of several paragraphs should show (for descriptions longer than 2 KB, see
  [Long OPDS descriptions are shown cut][long-opds] below).
- [ ] If you have an OPDS catalog that needs a sign-in, the sign-in should work (the
  authentication document is JSON).
- [ ] The Plugin Hub catalog should be listed; a plugin should install.
- [ ] Borrow or download an LCP-protected book, enter the passphrase and open it (the license,
  the device registration and the /unlock response are JSON).
- [ ] Settings → System → Check for Updates should give a result without errors.

[long-opds]: #long-opds-descriptions-are-shown-cut-fix-a-long-json-string-value-is-cut-not-dropped

## Damaged cache file no longer crashes ("fix: a damaged cache string no longer aborts the reader")

If the length field of a string in the book cache was damaged (a bit flip on the SD card), the
reader asked for that much memory and crashed with `abort()`; the book gave a crash report at
every opening. In the emulator, with the title length in `book.bin` set to 0xFFFFFFF0, the device
crashed while opening the book. Now `readString`, which reads from `HalFile`, rejects a length
that goes past the end of the file, and `readPod` gives zero on a cut-off file; when
`BookMetadataCache::load` sees this, it rebuilds the cache. With the same damaged card the book
opened ("Cache file damaged" and the rebuild in the log).

- [ ] In normal use, opening a book, turning pages, and closing and opening it again should work
  as before (the cache should not be rebuilt for no reason: "Cache file damaged" should not appear
  in the log).

## Table of contents readers in Rust ("rust: our own NCX and nav TOC readers")

The book's table of contents document (EPUB 2 NCX or EPUB 3 nav) is now read with `bembeyaz-toc`;
the HTML entity table (`&nbsp;`, `&quot;`, `&mdash;` …) is in `bembeyaz-html`, and the chapter
text, the Details description, dictionary definitions and KOReader sync use it too. The 19 host
tests written against the old C++ readers pass unchanged. During development, a differential test
compared the old and new readers on 60,000 generated documents (half of them randomly damaged),
in random chunks (the name, depth, path and anchor of every table of contents entry, and every
write result, were the same; a deliberate change was caught); all 253 entities in the old table
give the same value. In the emulator, a book with nav and a copy of it with only an NCX gave the
same table of contents (5 entries) and the same screens as unchanged CrossPoint (the book from
`emu/make_xml_epub.py`).

- [ ] Delete the cache of an EPUB 3 book (with the book open, Delete Book Cache in the reading
  menu) or open the book for the first time; in the reading menu, open Select Chapter: the
  chapters should show with the right names and the right indent, and choosing one should go to
  that chapter.
- [ ] Do the same with an old EPUB 2 book that has only an NCX.
- [ ] In an EPUB 3 book (with nav) whose chapter names contain entities such as `&mdash;` and
  `&hellip;`, the names should show correctly in the table of contents.
- [ ] On the Details screen of a book whose description contains `&quot;`, `&amp;` and `&mdash;`,
  the description should show correctly (the mark, not the entity name).
- [ ] KOReader sync: the position should be carried over as before.

## Table of contents label at most 1 KB ("fix: a TOC label keeps at most 1 KB")

The name of a table of contents entry is now kept to at most 1024 bytes (it ends on a whole
character); before, it had no limit, and copying a damaged table of contents document with
megabytes of text could crash the device. Real chapter names are far below this.

- [ ] In a book with long chapter names, the table of contents should look as before.

## Long OPDS descriptions are shown cut ("fix: a long JSON string value is cut, not dropped")

A string value that did not fit in the JSON parser's 2047-byte token buffer was dropped
completely, as in the SDK: an OPDS 2 book description longer than 2 KB never showed. Now the value
is given cut at a UTF-8 character boundary; most readers already shorten their fields to their own
limit. Keys and numbers are dropped as before. Readers that keep the whole value (the OAuth token,
the plugin catalog) get such a long token or address cut: the sign-in or the download still
fails, but not at once; it fails at the request.

- [ ] On the detail screen of an OPDS 2 publication whose description is longer than 2 KB (in the
  publication JSON, a description longer than 2048 bytes), the start of the description should
  show under "About this book" (not empty); on the last page of the description opened with
  Details, there should be no half or broken character.
- [ ] At the end of a long HTML-formatted description, no half tag or entity text should show.
- [ ] In an OPDS catalog that needs a sign-in (OAuth), the sign-in should work as before.
- [ ] Browsing and downloading in an OPDS 2 catalog, and the Plugin Hub (the catalog and plugin
  downloads), should work as before.

## container.xml in Rust ("rust: our own container.xml reader")

`META-INF/container.xml`, which tells where the book's package document is, is now read with the
`container` module of `bembeyaz-opf`. The 10 host tests written against the old C++ reader pass
unchanged. During development, a differential test compared the old and new readers on 50,000
generated documents (some of them damaged; a path was found in 5,627 of them), in random chunks
(the path found and every write result were the same; a deliberate change was caught). In the
emulator, `emu/compare.sh` gave the same screens as unchanged CrossPoint.

- [ ] Delete the cache of a few books from different sources (with the book open, Delete Book
  Cache in the reading menu) or open the books for the first time: the books should open, and the
  title and the cover should show in the Library; "Could not find valid rootfile in
  container.xml" should not appear in the serial log.
- [ ] KOReader sync should work as before (it reads container.xml too).

## XTC/XTCH parser in Rust ("rust: our own XTC reader")

The file header, page table, page headers and chapter list of XTC/XTCH, Xteink's format of
pre-drawn pages, are now read with `bembeyaz-xtc`; opening and closing the file stayed in C++.
910 host tests pass, including the new `test/xtc_parser` tests; these new tests also pass with the
old code. During development, a differential test compared the old C++ code with the Rust code on
30,000 generated books (some of them damaged or cut off) and on 205 real XTCH books (every page,
reading in pieces, and the chapters): the results were the same. In the emulator,
`emu/compare.sh emu/xtc.txt` gave the same screens as unchanged CrossPoint on two books, a 1-bit
one and a 2-bit one, made with `emu/make_xtc_files.py`.

- [ ] Open a few `.xtc` and `.xtch` books: the pages should be drawn as before, and turning a page
  should be as fast as before; after you open a book, its title and author should show in the
  recently read books on Home. In the two books made with `python3 emu/make_xtc_files.py xtc`,
  every page has as many black bars as its page number.
- [ ] Open the chapter list in an XTCH book that has chapters: the chapters should be listed as
  before, and jumping to a chapter should open the right page.
- [ ] On Home, the cover thumbnail of an XTC book that was not opened before (or whose cache was
  deleted) should look as before (it is made from the book's first page).

## Bounded reservation for the XTC chapter list ("xtc: bound the chapter list reservation")

In an XTC book whose chapter table runs to the end of the file (the page table and the page data
come before the chapters), the chapter list reserved room in advance for every record the table
could hold; in a large file this meant megabytes of memory, and when the allocation failed the
device crashed. Now room for at most 256 chapters is reserved in advance, and the list grows as
needed. The new test in `test/xtc_parser` checks this.

- [ ] In an XTC or XTCH book with more than 256 chapters, the chapter list should open as before,
  and the last chapters should show too.

## KOReader chapter positions in Rust ("rust: our own KOReader XPath resolver")

The part of KOReader sync that turns a reading position into a path that KOReader understands
(such as `/body/DocFragment[3]/body/div[2]/p[4]/text()[1].12`) is now in `bembeyaz-kosync`;
reading the chapter from the book stayed in C++. 911 host tests pass, including the chapter XPath
tests. During development, a differential test compared the old C++ code with the Rust code on
20,000 generated chapters (nested elements, hidden elements, HTML entities, comments, CDATA,
damaged or cut-off documents) and on 1518 real chapters of four books, with paragraph, position
and progress targets: the results were the same. This code runs only when sending a position;
receiving a position from KOReader goes through other code.

- [ ] In Settings → CrossPoint Sync, set Server Type to "KoSync" and Sync Behavior to "Ask Every
  Time", and enter the account details. In an EPUB, go to the middle of a paragraph and to a list
  item; from the reading menu, choose "Sync Progress" → "Upload local progress". Open the same
  book in KOReader and pull the progress: the same paragraph and about the same line should open.
  Try it on the last page of a chapter too.
- [ ] (The unchanged direction; it should work as before) Move ahead in KOReader and choose "Apply
  remote progress" here: the same page should open.

## KOReader position after long element ("kosync: find positions held back to the end of a chapter")

If a chapter has an element longer than 1 KB (a long SVG path, an embedded image, a long style or
comment), the XML parser holds back what follows until the end of the chapter. If the position
was found only then, the old code ignored it, and sync fell back to the start of the chapter or to
a coarser position. Now it is found; the new test in `test/koreader_xpath_resolver` checks this.

- [ ] On the last page of a chapter with large embedded images or SVGs, choose "Sync Progress" →
  "Upload local progress": the same place should open in KOReader.

## Finding the position from KOReader in Rust ("rust: our own KOReader position locator")

In KOReader sync, the part that reads the position path from the server (such as
`/body/DocFragment[3]/body/div[2]/p[4]/text()[1].12`) and finds it in the chapter is now in the
`locate` module of `bembeyaz-kosync`; reading the chapter from the book and turning the found
position into a page stayed in C++. 912 host tests pass. During development, a differential test
compared the old C++ code with the Rust code on 30,000 generated chapters and paths (damaged ones
included) and on 64,504 paths made from 1518 real chapters of four books, with every search mode:
the results were the same. This code runs mostly when receiving a position; when sending, it is
used only as a last resort, if the chapter XPath resolver finds no path.

- [ ] In Settings → CrossPoint Sync, set Server Type to "KoSync" and Sync Behavior to "Ask Every
  Time" (the default, "Smart Sync", does not ask for this choice), and enter the account details.
  In KOReader, send the progress from the middle of a book, from a list item and from the start
  of a chapter; here, in the same book, choose "Sync Progress" → "Apply remote progress" from the
  reading menu: the same place should open each time.

## Library text in Rust ("rust: our own Library text")

The text functions that the Library uses for search and sorting (folding that removes accents,
letter group, fixing the author name, the author key that joins different spellings of the same
person, search match, sorting by surname) are now in `bembeyaz-library`. 912 host tests pass,
including the tests of the Library text, its builder and the index file. During development, a
differential test compared the old C++ code with the Rust code on 300,000 random texts (with
accents, decomposed, in various scripts, with broken bytes): the results were the same. In the
emulator, the Library's "Recent", "Title" and "Author" tabs looked the same as in unchanged
CrossPoint. The one difference: in a damaged index record, a book's letter group is no longer read
from bytes past the end of the record.

- [ ] Open the Library: the books should be sorted as before in the "Title" and "Author" tabs. In
  the "Title" tab, press and hold a book to open the letter groups: the groups should be as
  before; titles that start with Ç, Ö or Ş show in the C, O or S group, not as separate letters.
  In the "Author" tab, books with the spellings "Austen, Jane" and "Jane Austen" should show under
  one author.
- [ ] In Search, type "cicek" without accents: the book titled "Çiçek" should be found.

## Reading the Library index file in Rust ("rust: our own Library index reader")

The part that checks and reads the Library's index file on the card (`CLX1`) (header, sections,
book records, names, author, title, folder path, the order of recently read books) is now in the
`format` module of `bembeyaz-library`; opening the file, and the builder that writes the index,
stayed in C++. 912 host tests pass, including the format, index file and builder tests. During
development, a differential test compared the old C++ code with the Rust code on 20,000 generated
index files (some of them damaged or cut off): the results were the same.

- [ ] Open the Library, scroll in the "Recent", "Title" and "Author" tabs and open a book: the
  books, their names, their authors and the opened book should be as before.
- [ ] Open the "Recent" tab in the Library (with an empty search): the books you read last should
  be at the top and should not show a second time in the list below. Reverse the sort order: the
  books should be listed one by one, in the right order.
- [ ] The titles on the recently read shelf on Home should look as before.
- [ ] Open and close the Library, then open it again: on the second opening the index should not
  be rebuilt.

## Converting TXT and Markdown books in Rust ("rust: our own text to XHTML")

The part that turns the text of TXT and Markdown books into a readable page (line ends,
first-line indents, spaces, escapes for `&`, `<` and `>`, the byte order mark) is now in the
`text` module of `bembeyaz-html`. 912 host tests pass, including the TXT conversion tests. During
development, a differential test compared the old C++ code with the Rust code on 100,000 random
texts, with random read chunks: the results were the same. In the emulator, an indented TXT book
with Turkish characters gave the same pages as unchanged CrossPoint.

- [ ] Open a `.txt` book and a `.md` book (if you opened them before: with the book open, choose
  Delete Book Cache in the reading menu; the device goes back to the Home screen; open the book
  again): first-line indents, empty lines and Turkish characters should look as before.

## Library index writer in Rust ("rust: our own library index writer")

When the Library index (`/.crosspoint/library.idx`) is built, these steps are now in the `build`
module of `bembeyaz-library`: writing the records, the two sort permutations (author and arrival)
and the name blobs (path hash, file name, shown author, title, source author); reading from the
stage file the sort keys for the title, the author key and the surname of the shown spelling;
choosing the most used one among different spellings of the same person (on a tie the shorter
one, then the first in alphabetical order); and reading the modification times. Walking the
folders, reading the books' metadata, the stage file, matching with the old index (keeping the
arrival order of renamed books), memory allocation and damage decisions, sorting (`std::sort`),
the header and folder section, and putting the files in place stayed in C++. The commit before
this one (9421d9ca) pinned the index with 3 tests: a checker that decodes every section of the
index on its own; the recorded hashes of the index on generated cards (first build; after delete,
rename, add and touch; without metadata) and at every memory allocation failure point; and a
hand-checked spelling vote. In total, 1009 host tests pass. During development, a one-off
differential test (not kept in the tree) ran the old and new code on 1200 random cards (2957
builds; in 688 of them a memory, read, write or rename failure was injected): every result,
statistic and index was the same byte for byte; a bug put into the code on purpose was caught at
once. The new `emu/library.txt` scenario opens the Library, builds the index and opens it again;
on the `emu/books` card (15 books) and on a card with 600 TXT files, the screens were the same as
with the previous version, and the indexes were the same byte for byte except for the modification
times (the cards were prepared at different moments, so the times were shifted by 36–38 seconds);
the build time also stayed almost the same (with 600 books, the write stage took 4738 ms before
and 4697 ms now). On x4pro, flash grew by 1232 bytes (the Rust code is a little larger than the
C++ it replaced); RAM did not change.

- [ ] Open a Library built with the previous firmware: the books, the orders in the "Title",
  "Author" and "Recent" tabs, and the author names should look as before. Then add a book to the
  card (with File Transfer) and open the Library: the index is rebuilt with the new firmware; the
  orders and the author names should again be as before, and the new book should show.
- [ ] Put on the card a few EPUBs whose metadata spells the same author's name in different ways
  (for example "Lu Xun" and "Xun Lu"), and in the Library's "Recent" tab long-press a book and
  choose "Refresh library": they should all show under the same author, with the most used
  spelling; in the "Author" tab they should be sorted by surname; books without an author should
  stay at the end, under "Unknown author".
- [ ] On a card with hundreds of books, add or delete a book, then refresh the Library (long-press
  in the "Recent" tab, then "Refresh library"): the time should be close to that of the previous
  firmware, and `[LIBIDX] built:` should appear in the serial log; no `[ERR] [LIBIDX]` line should
  appear. (If nothing changed on the card, the index is not written again and
  `[LIBIDX] unchanged:` appears in the log.)

## Web paths in Rust ("rust: our own web path checks")

The web file manager, WebDAV and the plugin endpoints check incoming paths like this: the
credential files (`/.crosspoint/wifi.json`, `opds.json`, `koreader.json`, and the temporary and
backup files whose names start with them) cannot be reached with any spelling (`//`, `/./`, `..`,
upper and lower case, the leading spaces and the trailing dots and spaces that SdFat trims, 8.3
short names such as `CROSSP~1` in the first two components); names that start with a dot, and
`System Volume Information` and `XTCache`, cannot be changed or deleted (in the list, names that
start with a dot show only when Show Hidden Files is on, and the system names never show); the
request's path is turned into an absolute path (`..` is applied, a single `/` at the start, none
at the end); the path part is taken from WebDAV's `Destination` header; paths are URL-encoded
(space, `%`, `#`, `?`, `&` and non-ASCII bytes become `%XX`). These are now in the new `web`
module of `bembeyaz-path`; the list of hidden names, which the web server and WebDAV each kept
separately, is in one place. Arduino's `urlDecode` and the handling of requests stayed in C++. The
old code was copied as it was into `test/web_paths/OldWebPaths.h`, and on 300,000 random paths
and URLs (slashes, `.` and `..`, credential and system names, names with spaces and dots, short
names, `%` escapes, schemes, non-ASCII bytes) the results of all checks are the same; the crate
also has a mini-fuzz test. As before, names are compared up to the first NUL: the name
`XTCache%00x`, which comes in a request with `%00`, is protected too (found in review; it was open
in the first draft; the test now also makes raw NUL bytes). On a path with a NUL, the
hidden-component check also splits and searches what comes after the NUL, so it is only stricter
than before; URL encoding also stops at the NUL (the old code copied the NUL and what follows it).
In both cases, paths that come from requests are already cut at the first NUL. Flash grew by 640
bytes on x4pro and by 688 bytes on x4c; static RAM did not change.

- [ ] Connect to Wi-Fi and open the file manager at `http://purestwhite.local` in a browser: in
  the root of the card, `System Volume Information` and `XTCache` should never be listed, and
  `.crosspoint` should not be listed while Show Hidden Files is off; upload a file to a folder
  whose name has spaces and Turkish letters, download it, rename it and delete it.
  `curl -X POST -d 'path=/&name=XTCache%00' http://purestwhite.local/mkdir` should return 403,
  and no `XTCache` folder should be made in the root.
- [ ] Connect with a WebDAV client (for example on Linux, Files → `dav://purestwhite.local`
  instead of `davs://`, or `cadaver`): folders should be listed; a file whose name has spaces and
  Turkish letters should upload, move and copy; `.crosspoint` and its contents should not show and
  should not be changeable.
