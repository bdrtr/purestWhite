# Hardware tests: dictionary

These tests cover StarDict dictionary look-up: the dictionary index and search, the dictzip reader,
and how HTML definitions and descriptions are turned into XHTML or plain text. See
[README.md](README.md) for how hardware tests work.

## Dictionary index and look-ups in Rust ("rust: our own dictionary index")

In StarDict dictionaries (`/dictionaries/<folder>/`), these steps are now in the new
`bembeyaz-dict` crate: reading the 64-bit offset and HTML definition information from the `.ifo`;
building the sampled `.qidx` and `.sidx` indexes during "Indexing Dictionary..." at the first
opening (the offset of every 256th entry); the binary search and the scan of at most 256 entries
in a look-up; going from a synonym (`.syn`) to its entry by the entry number; stripping
punctuation from the looked-up word and making it lower case; and the English stem variants (-'s,
-’s, -ies, -es, -s, -ed, -ing). The files are given to Rust through a callback that reads at an
offset; the callback reports a failed seek (past the end of the file, or a broken cluster chain)
apart from a read error, because a failed seek to the start of the scan counts, as before, as a
read error and not as "Not Found". Words are now read in 32-byte pieces, not byte by byte, and are
written, as before, into the 256-byte buffer in the dictionary object (the stack frames of Rust's
look-up functions are 112 and 144 bytes). Finding the dictionary folder, opening the files, memory
checks, dictzip and reading the definition stayed in C++. The dictionary code had no tests before:
the old code was copied as it was into `test/dictionary` and is compared with the real
`Dictionary.cpp` on the same in-memory file system. On 1000 random dictionaries (random headwords
and headwords longer than 255 bytes, synonyms, `.ifo` variants, a cut-off `.idx`, broken offsets,
damaged, cut or deleted indexes, read errors, and broken clusters where a seek fails after some
offset), each with 60 look-ups (upper and lower case, quotes and punctuation, English suffixes,
made-up and very long words), the index files, the indexing result, and the result, definition and
headword of every look-up are the same. Of 24 bugs put into the code on purpose, 21 were caught
(12 by this test, 9 by the crate tests); 3 do not change the result. The old code's check for
repeated variants could never trigger, so it was removed (a crate test pins this). A difference
may remain on read errors: on the SD card, a 32-byte read that touches a bad sector fails as a
whole; the old byte-by-byte read could still find an entry that ended before the bad sector.
Reading definitions from `.dict.dz` dictionaries (dictzip) is not in this test; that path did not
change. Later, in the emulator, `emu/dictionary.txt` (with the plain and HTML/dictzip cards of
`emu/make_dictionary_cards.py`: stems, synonyms, words with quotes and apostrophes, a definition
over 16 KiB, a word that is not found) gave output that was the same pixel for pixel as the
version before the four dictionary changes (da4da51c). Flash grew by 2152 bytes on x4pro and by
2156 bytes on x4c; static RAM did not change.

- [ ] Set up a StarDict dictionary that was not indexed before (Dictionary; or delete the `.qidx`
  and `.sidx` files in its folder) and look up words in a book with Look Up: on the first look-up,
  "Indexing Dictionary..." should show, then the definitions should come as before.
- [ ] A dictionary indexed by the old version should open without being indexed again (the index
  format did not change).
- [x] Words in quotes, words that start with a capital letter, and plural words or words with
  -ed/-ing should be found by their stem forms; a word that is not in the dictionary should give
  "Not Found".
- [x] In a dictionary with a `.syn` file, looking up a synonym (for example an irregular form)
  should bring up its main entry.
- [ ] In both `.dict` and `.dict.dz` dictionaries, look-ups should be as fast as before or faster;
  especially in a large dictionary (`.idx` larger than 20 MB) on a FAT32 card, try words that are
  found and also words that are not found even with stem variants (each entry is read from its own
  offset; a rewind at a cluster boundary can walk the FAT chain from the start).

## Turning dictionary HTML definitions into XHTML in Rust ("rust: our own dictionary XHTML")

In StarDict dictionaries with HTML or XDXF definitions (`sametypesequence=h` or `x` in the
`.ifo`), the definition (and the description on the Details screen of books in the OPDS catalog
and of HTML items in the plugin catalog) is turned into clean XHTML that the chapter parser can
read, before it is laid out as formatted pages: it is wrapped in `<html><body>`, tag names are made
lower case (cut at 15 characters), empty elements are closed (`<br>` → `<br/>`) and their closing
tags (`</br>`) are dropped, comments, doctype and processing instructions are dropped, and a `<` or
`&` that is not markup or an entity reference is written as `&lt;` or `&amp;`. This conversion is
now in the `xhtml` module of `bembeyaz-html`; writing to the temporary file with a 128-byte
buffer, heap checks and pagination are in C++. Text pieces are now written in one go, up to the
next markup or the next character written as an escape, not character by character. The old
function was copied as it was into `test/dict_html`: on 50000 random tag soups (all kinds of tags,
quoted attributes, unclosed tags, comments, entities, upper case, names longer than 15
characters, UTF-8, random bytes; sometimes with the write cut off halfway) the written file and
the result are the same. Of 17 bugs put into the code on purpose, 16 were caught; the remaining one
does not change the result (the closing tags of empty elements are already dropped early). Later,
in the emulator, `emu/dictionary.txt` (with the plain and HTML/dictzip cards of
`emu/make_dictionary_cards.py`: stems, synonyms, words with quotes and apostrophes, a definition
over 16 KiB, a word that is not found) gave output that was the same pixel for pixel as the
version before the four dictionary changes (da4da51c). Flash grew by 428 bytes on x4pro and by 432
bytes on x4c; static RAM did not change.

- [x] In a StarDict dictionary with HTML definitions (for example one made from Wiktionary), look
  up words with Look Up: the definitions should show formatted as before (bold, italic, lists, line
  breaks); they should not fall back to plain text.
- [ ] In the OPDS catalog, open the Details screen of a book with an HTML description, and the
  full description opened with the Details button there: the description should show formatted as
  before and should not fall back to plain text.

## Turning HTML into plain text in Rust ("rust: our own HTML plain text")

HTML dictionary definitions that cannot be laid out as formatted pages (longer than 16 KiB, or
whose formatted layout fails) and Details descriptions are shown as plain text: tags are dropped;
`p`, `h1`–`h6` and `hr` become a paragraph, and `br`, `div`, `li` and `tr` a line break (they do
not pile up; in Details, line breaks are later turned into spaces); tab and CR become a single
space; named and numeric entities are decoded; trailing spaces and line breaks are dropped. This
conversion is now in the `plain` module of `bembeyaz-html`. Because the text never grows (every
entity is longer than its value; every line break is no longer than the tag or line break it
replaces), Rust writes into the string that C++ allocates at the input's size; the old code also
allocated room at the input's size. The existing 6 tests pass unchanged; the old function was
copied as it was into `test/html_to_plain_text`, and on 50000 random pieces (tags, names with
upper case and spaces, named and numeric entities, out-of-range and surrogate code points, tab,
CR, line break, UTF-8, random bytes) the output is the same and never longer than the input. All
16 bugs put into the code on purpose were caught. Later, in the emulator, `emu/dictionary.txt`
(with the plain and HTML/dictzip cards of `emu/make_dictionary_cards.py`: stems, synonyms, words
with quotes and apostrophes, a definition over 16 KiB, a word that is not found) gave output that
was the same pixel for pixel as the version before the four dictionary changes (da4da51c). Flash
grew by 384 bytes on x4pro and by 372 bytes on x4c; static RAM did not change.

- [x] In a dictionary with HTML definitions, look up a word that has a very long definition
  (larger than 16 KiB): the definition should show as plain text, split into lines and paragraphs
  as before, with its entities (for example `&amp;`, `&#233;`) decoded.
- [ ] If the Details screen of a book with an HTML description in the OPDS catalog falls back to
  plain text when memory is low (for example in a catalog opened while a large book is being
  read), the description should show as before: in one piece, without tags, with its entities
  decoded.

## Dictzip header and chunk plan in Rust ("rust: our own dictzip tables")

In `.dict.dz` dictionaries (dictzip: chunks compressed independently), these steps are now in the
`dictzip` module of `bembeyaz-dict`: reading the gzip header and the `RA` chunk table (FEXTRA
fields, version, chunk size and count, at most 8192 chunks, field lengths that must agree, name,
comment and header CRC, the trailing ISIZE), and working out from which chunks a definition is
read, and how much of each is skipped and how much is taken. The memory of the chunk table and the
heap check before it, inflating the chunks (InflateStream) and the file are in C++. The existing 8
DictZip tests pass unchanged with a real dictionary file; the old header reader was copied as it
was into `test/dict_zip`, and the chunk loop too, with only the chunk inflation turned into a
callback. On 20000 random headers (bad magic byte, missing FEXTRA, other fields, two `RA`, wrong
version, a chunk count of zero or over 8192, lengths that do not agree, name, comment, CRC, zero
ISIZE, cut-off file, read error, not enough memory for the chunk table) the result, the error kind
and the table are the same; on valid headers, the chunk plan of more than 70 thousand ranges
(skipped and taken bytes, sometimes with the inflation of a chunk failing) is also the same. Of 24
bugs put into the code on purpose, 19 were caught; the other 5 do not change the result
(unreachable or needless branches). Later, in the emulator, `emu/dictionary.txt` (with the plain
and HTML/dictzip cards of `emu/make_dictionary_cards.py`: stems, synonyms, words with quotes and
apostrophes, a definition over 16 KiB, a word that is not found) gave output that was the same
pixel for pixel as the version before the four dictionary changes (da4da51c). If parsing fails,
the half-built chunk table is now freed at once. The crate also has a mini-fuzz test for dictzip.
Flash grew by 528 bytes on x4pro and by 520 bytes on x4c; static RAM did not change.

- [x] In a compressed `.dict.dz` StarDict dictionary, look up a few words with Look Up (short and
  long definitions, words from the start and from the end of the dictionary): the definitions
  should come as before.
