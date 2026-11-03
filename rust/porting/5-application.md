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
resave for both kinds. The stores go to the `bembeyaz-store` crate one by one (a module with `read`
and `write` each), over `bembeyaz-jsondoc` (its `|` and `is<T>` are pinned by
`test/json_conversions`). Their C ABIs are `rust/include/bembeyaz/store_*.h`, with the
`bb_store_write` callback and `bb_store_text` of `store.h` (on the C++ side,
`lib/Serialization/StoreText.h`): a store with a fixed set of fields crosses as a `repr(C)` struct;
a list crosses as an array of item structs when written and one item per callback when read, the C++
store giving its limit (`MAX_RECENT_BOOKS`) so it is defined once, when every item read is kept;
when the limit counts only the items C++ keeps (Wi-Fi's `MAX_NETWORKS`, and the OPDS token and LCP
stores'), Rust hands over every item and C++ applies the limit in its callback. The text is written
by `bembeyaz_jsondoc::Writer` straight into one buffer, and a store's `write` returns `None` when
memory runs out, so the old file is kept. For the stores whose decisions are still C++'s (Wi-Fi,
OPDS, the token and LCP stores), passwords and other secrets stay obfuscated in C++
(`ObfuscationUtils`, keyed by the eFuse MAC) and cross as their stored texts; C++ reads a password
from them with `obfuscation::readStoredPassword` (`StoredPassword.h`). Every store obfuscates for
saving with `ObfuscatedTexts` (`StoreText.h`), which also refuses the save when obfuscating fails.
There, a legacy shape is upgraded, and resaved, in C++ when it depends on a decoded value or on C++
state (the OPDS plain `password`), and a field whose type or presence C++ decides on crosses as a
`bb_store_number` with its kind (missing, a `u32`, another type; `bembeyaz_store::field`): the Wi-Fi
password's `password_len` and `password_crc32`. So the rule is: a field read with a plain `|
literal` that the store decides nothing on crosses as the value Rust read that way, whatever its
type (texts, bools, the bookmarks' and CrossPointState's numbers); a field C++ decides on crosses
with its kind.

The direction is for Rust to own a store's decisions, and the C++ store to give only the
obfuscation. KOReader's are Rust's alone, in a module of their own beside the file format's
(`bembeyaz_store::koreader_settings`): Rust reads the file, decodes each secret through a
`bb_store_decode` callback (`store.h`; on the C++ side `decodeStoredSecret` in `StoreText.h`; Rust's
side is `bembeyaz_store::password`, whose `Secret` wipes a plain secret when dropped), makes the
decisions (`Number`'s `byte` is `as<uint8_t>()`, and `byte_or` a `| fallback` for a `u8`), and hands
C++ the settings in one callback, with the resave and what a check changed, for its log. The
bare-metal firmware can use the same functions with its own decoder. Wi-Fi's decisions are still in
both languages (`wifi_credentials::keep`, used by `purestwhite-wifistore`), so a change to one must
be made in the other, until `keep` reports why it drops a network, for the C++ store's log, and C++
calls it. ABI types stay in the store's `.cpp`, out of its header. Each store's file format (its
keys and their order) is Rust's; `CrossPointSettings` is the exception: its keys come from C++'s
`SettingsList` table, shared with the web settings page, so it will read through a parsed document
with typed getters instead: `*_new` parses the text in Rust's heap and `*_free` frees it, and every
view it gives is valid until then. A number comes with its kind and `byte`, a text with its kind,
and a bool or byte with its `| default`; a key's presence can be asked too. It will write through a
C ABI over `bembeyaz_jsondoc::Writer` or a list of typed key/value entries, and C++ keeps its table
and its migrations. After it no store reads an ArduinoJson document:
`readDocFromFile`/`writeDocToFile`, `StoresJsonDoc` and the `JsonVariantConst` `extractPassword`
overloads go. ArduinoJson itself stays in the firmware for its other users (the web server,
clippings sync, LCP, plugins, font download, the BLE page turner's settings). `BookmarkFile` (a list
of items with a float and optional members, and no store class) has a module of its own,
`bookmarks`. A nested list with a fixed limit is a fixed array and a count inside its item
(`bb_store_header`, shared by the OPDS servers and the KOReader server). The oracles share
`test/json_oracle`; from the Wi-Fi store on, a store's Rust test replays `read` and `write` against
the stored-text layers the driver prints, and leaves the C++ glue to `check_shim.sh` (the OPDS test
still mirrors that glue).

- **App state ✅**: `CrossPointState` (`state.json`: the open book, the reader's load count, the
  rings of recent sleep images and their legacy `lastSleepImage`) calls the `state` module of
  `bembeyaz-store`. `test/crosspoint_state` writes the C++ store's answers on 1,500 seeded steps
  and 13 edge steps to `rust/crates/bembeyaz-store/testdata/state-corpus.txt`, which the crate's
  tests replay; its `check_shim.sh` runs the same steps through the C++ shim over Rust.
- **Recent books ✅**: `RecentBooksStore` reads and writes its JSON (`recent.json`: up to 10
  books with their path, title, author and cover path) through the `recent_books` module of
  `bembeyaz-store`. Adding, moving and pruning them stays in C++. `test/recent_books` writes
  the C++ store's answers on 1,000 seeded steps and 10 edge steps to
  `rust/crates/bembeyaz-store/testdata/recent-books-corpus.txt`; its `check_shim.sh` runs them
  through the C++ shim over Rust.
- **OPDS servers ✅**: `OpdsServerStore` reads and writes its JSON (`opds.json`: up to 8 catalogs
  with their name, URL, user name, password and two extra headers) through the `opds_servers` module
  of `bembeyaz-store`, the password and header values as stored: their obfuscation, and the resave
  of a legacy plain password (`obfuscation::readStoredPassword`, `StoredPassword.h`), stay in C++.
  `test/opds_servers` goes through the store's `loadFromFile` and `addServer` over a host stand-in
  for the obfuscation, so PersistableStore's file steps and the resave are in its answers too: 1,000
  seeded steps and 12 edge steps, in `rust/crates/bembeyaz-store/testdata/opds-servers-corpus.txt`;
  its `check_shim.sh` runs them through the C++ shim over Rust.
- **Wi-Fi networks ✅**: `WifiCredentialStore` reads and writes its JSON (`wifi.json`: the saved
  networks, each password obfuscated with its length and CRC-32, and the network connected last)
  through the `wifi_credentials` module of `bembeyaz-store`. Rust hands over every item of the file
  with its texts as stored and each number with its kind (`bb_store_number`,
  `bembeyaz_store::field`); C++ decodes the password and decides which networks to keep (the length
  and checksum checks, at most 8 kept) and whether to save again, as before. `test/wifi_credentials`
  goes through the store's `loadFromFile`, `addCredential`, `setLastConnectedSsid` and
  `removeCredential` over the host stand-ins of `test/json_oracle`: 1,000 seeded steps and 11 edge
  steps, in `rust/crates/bembeyaz-store/testdata/wifi-credentials-corpus.txt`. The Rust tests replay
  the stored-text layers the driver prints (what a file holds, what a save writes); `check_shim.sh`
  checks the C++ store's answers through the shim. Its keep-or-drop decisions are also in Rust,
  `wifi_credentials::keep` (the password decoder given), checked against the same corpus's answers
  with the host stand-in; the bare-metal firmware's `purestwhite-wifistore` keeps its networks
  through `keep`, so it makes the C++ firmware's decisions, with its own limits on top: it leaves
  out a name longer than Wi-Fi's 32 bytes, refuses a file over 16 KiB, and reports running out of
  memory.
- **KOReader sync account ✅**: `KOReaderCredentialStore` reads and writes its JSON (`koreader.json`:
  the sync account, its password obfuscated, the server, its type, the document match method, the
  sync behavior, two flags and two extra headers) through the `koreader_credentials` module of
  `bembeyaz-store`, and decides on it in Rust (`koreader_settings`, `store_koreader_credentials.h`'s
  `bb_store_koreader_credentials_settle`): the password and header values decoded through C++'s
  obfuscation (a `bb_store_decode` callback), the upgrades of a pre-v2 config (the old default
  server pinned) and of a pre-v3 one (the server type from the URL), the check of each setting
  (`as<uint8_t>()` for `serverType`), the resave, and the config version every save writes. C++
  applies the settings through its setters and logs what a check changed; header slots the file
  does not have keep what they held, as before. `getBaseUrl` is Rust's too (`base_url_parts`,
  `bb_store_koreader_base_url`), so the bare-metal firmware can use it. `test/koreader_credentials`
  goes through `loadFromFile`, the setters with `saveToFile`, and `clearCredentials`: 1,000 seeded
  steps and 10 edge steps, in `rust/crates/bembeyaz-store/testdata/koreader-credentials-corpus.txt`,
  each answer but `long`'s with `getBaseUrl()`; the Rust tests replay its stored-text layers, every
  successful load's settings and resave, and every base URL, and `check_shim.sh` the C++ store's
  answers through the shim.
- **OPDS tokens, LCP passphrases and LCP device ✅**: `OpdsTokenStore` (`opds_tokens.json`),
  `LcpPassphraseStore` (`lcp_passphrases.json`) and `LcpDeviceStore` (`lcp_device.json`, moved out
  of `LcpDevice.cpp` into its own files) are lists of records of texts, or one record, and share the
  `records` module of `bembeyaz-store` (`store_records.h`): each store's list and fields are Rust's
  (`records::Store`; C++ names the store and indexes the fields by the header's numbers), Rust reads
  every record's fields with `| ""` and writes the ones C++ gives. Which records are kept (an empty
  URL or provider, a passphrase that does not decode, at most 8 kept; `KeptRecords`, `StoreText.h`)
  and the obfuscation stay in C++. `test/token_stores` drives all three: 700 seeded steps and 23
  edge steps, in `rust/crates/bembeyaz-store/testdata/token-stores-corpus.txt`; the Rust tests
  replay its record layers, and `check_shim.sh` the C++ stores' answers through the shims.
- **Bookmarks ✅**: `BookmarkFile` (a book's `bookmarks/<book>.json`; no store class: it reads and
  writes through `PersistableStoreBase`'s text steps) reads and writes its JSON through the
  `bookmarks` module of `bembeyaz-store`: each field with its plain fallback (the percentage
  `| 0.0f`, the pages `| uint16_t(0)`, the visible-text offset when it is not null), an empty name
  and a missing offset not written. Cutting a name to 128 bytes, and refusing to save a longer one,
  stay in C++. `test/bookmark_file` drives `load` and `save`: 1,000 seeded steps and 10 edge steps,
  in `rust/crates/bembeyaz-store/testdata/bookmark-file-corpus.txt`; the Rust tests replay its
  bookmark layers, and `check_shim.sh` the C++ side's answers through the shim.

## `src/` activities

The activities under `src/` are also part of this stage.
