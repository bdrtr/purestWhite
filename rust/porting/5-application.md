# Stage 5: application

Stage 5 of the [porting order](../README.md#porting-order) covers the application layer (UI):
web paths, the library index, TXT and XTC books, KOReader sync, the clipping store, the
dictionary and the `src/` activities. ✅ marks a module that is done.

## Web paths ✅

The checks on paths that come from the network are in the `web` module of `bembeyaz-path`:

- the credential files (`/.crosspoint/wifi.json`, `opds.json`, `koreader.json`) cannot be
  reached with any spelling (`//`, `.`, `..`, upper/lower case, the spaces and dots that SdFat
  trims, 8.3 short names in the first two components);
- plugin paths;
- hidden names (names that start with a dot, `System Volume Information`, `XTCache`; these used
  to be separate lists in the web server and in WebDAV) and hidden components;
- turning the request's path into an absolute path;
- the path part of the WebDAV `Destination` URL;
- the URL encoding of paths.

`urlDecode` (Arduino) and request handling are in C++.

## LibraryIndex

- **Library text ✅**: the folding, letter group, person name, author key, search and surname
  sort functions of `LibraryText` are in `bembeyaz-library`.
- **Index file reader ✅**: the header validation and section layout of `LibraryFormat.h`, and
  the sort order, record, name, field, folder path and recently-read lookups of
  `LibraryIndexFile`, are in the `format` module of `bembeyaz-library`. The file handle and the
  `ClixHeader`/`ClixRecord` structs are in C++.
- **Index writer ✅**: the `build` module of `bembeyaz-library` holds these parts of
  `LibraryBuilder`: reading the title, the author key and the surname sort keys of the shown
  spelling from the stage file; the vote on one spelling per person; reading the modification
  times; and writing the records, the two permutations and the name blobs. In C++: walking the
  folders, reading the book metadata, the stage file (its layout is shared in `library.h`),
  matching with the old index, the memory and corruption decisions, `std::sort`, the header and
  the folder section, and putting the files in place.

## Txt

- **Text converter ✅**: the conversion of the TXT/Markdown body to XHTML in `TxtToHtml` is in
  the `text` module of `bembeyaz-html`. File reading, the buffers, the XHTML header and the
  cache version tag are in C++.

## Xtc

- **XTC/XTCH parser ✅**: `XtcParser` reads the header, the page table, the page headers and the
  chapter list with `bembeyaz-xtc`. Opening and closing the file is in C++.

## KOReaderSync

- **Chapter XPath resolver ✅**: `ChapterXPathResolver` streams the chapter's XHTML into
  `bembeyaz-kosync`, which finds the KOReader path for the paragraph, the visible character
  position and the progress. In the other direction, `ProgressMapper` reads the incoming path
  with the `locate` module of `bembeyaz-kosync` and searches for it in the chapter. This is not
  an XML parser but a loose byte scanner that comes from the old code: it can count CDATA, and a
  `>` in a comment or in quotes, differently from the sending half. It may move to
  `bembeyaz-xml` later; a round-trip test ties the two halves together. Reading the chapter from
  the book, the Section cache and the page mapping are in C++.
- **Progress comparison ✅**: `compareProgress` and `selectRemoteRecord` in `ProgressComparison`
  call the `progress` module of `bembeyaz-kosync`, which decides which of two reading positions
  is further on: by chapter, then page or text offset when both chapters are trusted, otherwise
  by percentage within 0.001. `test/progress_comparison` pins the old answers.
- **Sync messages ✅**: `KOReaderSyncMessages` (the account body, the progress update's body and
  the reading of a progress answer, apart from the HTTP in `KOReaderSyncClient`) calls the
  `messages` module of `bembeyaz-kosync`, over `bembeyaz-jsondoc`, which reads and writes JSON as
  ArduinoJson 7.4.2 does (its leniencies, its conversions of a field of another type, its float
  digits). `test/kosync_messages` writes ArduinoJson's answers on 4,000 seeded steps and 10 edge
  steps to `rust/crates/bembeyaz-jsondoc/testdata/kosync-corpus.txt`, which both crates' tests
  replay; its `check_shim.sh` runs the same steps through the C++ shim over Rust. The rest of the
  firmware's JSON is still ArduinoJson's.

## ClippingStore

- **Clipping store ✅**: reading and writing the per-book clipping file (header, records and
  texts; versions 1–4 are read, version 4 is written) and scanning the IDs in the delete log are
  in `bembeyaz-clippings`. In C++: opening the files, the renames with the backup and temporary
  files, the clippings in memory, writing to the delete log, making IDs, moving books, and the
  word check for highlights in `ClippingStore.h`.

## Dictionary

- **StarDict dictionary ✅**: these parts are in the new `bembeyaz-dict` crate, which reads the
  files through a callback that reads at an offset: reading the 64-bit offset and HTML
  definition information from `.ifo`; the header of the sampled indexes (`.qidx`, `.sidx`) and
  scanning the source while they are built; binary search in the samples and a scan of at most
  256 entries; going from a synonym to its entry by the entry's ordinal number; cleaning the
  word; and English stem variants. Finding the dictionary folder, opening the files, the memory
  checks and reading the definition are in C++.
- **dictzip ✅**: reading the gzip header and the `RA` chunk table of `.dict.dz` files, and
  working out which chunks a range is read from, how much to skip and how much to take, are in
  the `dictzip` module of `bembeyaz-dict`. The chunk table's memory, the heap check and
  inflating the chunks (InflateStream) are in C++.
- **HTML definitions to XHTML ✅**: `DictHtmlPages` turns StarDict HTML, and the HTML
  descriptions on the Details screen (tag soup), into XHTML for the chapter parser to read; this
  conversion is in the `xhtml` module of `bembeyaz-html`. The temporary file, the heap checks and
  the pagination are in C++.
- **HTML to plain text ✅**: turning definitions and descriptions that cannot be laid out as
  styled pages into plain text with `HtmlToPlainText` is in the `plain` module of
  `bembeyaz-html`. Allocating the string at the size of the input is in C++.

## Settings files

The stores built on `lib/Serialization/PersistableStore.h` keep their files in `/.crosspoint/` as
JSON. A store whose JSON is read and written in Rust gives `toJsonText`/`fromJsonText` in place of
`toJson`/`fromJson` (`fromJsonText` returns nullptr, or ArduinoJson's error name, and may call
`requestResave()`); `PersistableStore` keeps the file steps, the lock, the parse-error log and the
resave for both kinds. The stores go to the `bembeyaz-store` crate one by one (a module with
`read` and `write` each), over `bembeyaz-jsondoc` (its `|` and `is<T>` are pinned by
`test/json_conversions`). Their C ABIs are `rust/include/bembeyaz/store_*.h`, with the
`bb_store_write` callback of `store.h`: a store with a fixed set of fields crosses as a `repr(C)`
struct; a list (recent books, servers) or a large table (`CrossPointSettings`) will cross one
item or field per callback, as `bb_kosync_field` does. The oracles share `test/json_oracle`.

- **App state ✅**: `CrossPointState` (`state.json`: the open book, the reader's load count, the
  rings of recent sleep images and their legacy `lastSleepImage`) calls the `state` module of
  `bembeyaz-store`. `test/crosspoint_state` writes the C++ store's answers on 1,500 seeded steps
  and 13 edge steps to `rust/crates/bembeyaz-store/testdata/state-corpus.txt`, which the crate's
  tests replay; its `check_shim.sh` runs the same steps through the C++ shim over Rust.

## `src/` activities

The activities under `src/` are also part of this stage.
