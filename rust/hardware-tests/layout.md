# Hardware tests: text layout and the chapter parser

Tests for how chapter text is laid out: paragraphs, CSS, hyphenation, bidirectional text and table
column widths. Most sections cover the chapter parser, which moved to Rust one piece at a time.

## Paragraph layout ("rust: our own paragraph layout")

- [ ] A few books with different alignments (justified, left-aligned, centered): line breaks and
  word spacing must be as before; the page count (1/N in the footer) must not change.
- [ ] With Hyphenation on, one Turkish and one German book: the hyphens must be in the same places.
- [ ] With Focus Reading on: the start of each word must be bold, and the alignment must not shift.
- [ ] A Japanese/Chinese book (better if it has ruby), a Korean book and an Arabic/Hebrew book:
  line breaking and right-to-left order must be as before.
- [x] A book with footnote links: tapping a link must open the footnote.
- [ ] A long chapter with a font loaded from the SD card: the time to build the first page
  (`Time to parse and build pages`) must not be longer than before.
- [ ] A chapter that is one very long paragraph (e.g. a Chinese novel where a whole chapter is one
  paragraph): it must open, with no restart.

## CSS (46ebf6e)

- [ ] A book with heavy styling (headings, indents, alignment, bold/italic classes): it must look
  as before. The CSS cache format did not change; the cache must not be rebuilt.

## Hyphenation (609d473)

- [ ] With Hyphenation on, a Turkish and an English book: lines must break with hyphens, and the
  page layout must be as before.

## Bidirectional text (1b556ee, 3987490, 09112bb)

- [ ] A book with an Arabic or Hebrew font loaded: word order, bracket directions, numbers and
  Arabic letter joining must be correct.

## Table column widths in Rust ("rust: grid table column widths")

The pre-scan done before a table's rows are laid out (the longest word and the longest unbroken
line of each column) and the sharing of the screen width among the columns are now in the `table`
module of `bembeyaz-layout`. 890 host tests pass, the chapter parser's table tests among them.
During development, a diff test compared the Rust code with the old C++ code on 300,000 generated
tables (some broken, some over the 32 KB limit) and on 101 real tables from four books: the
measurements and the widths were the same. In the emulator, `emu/compare.sh` with
`test/epubs/crosspoint-table-edge-cases.epub` gave the same screens as unmodified CrossPoint over
12 pages.

- [ ] Copy `test/epubs/crosspoint-table-edge-cases.epub` to the card and open it (if you opened it
  before, first use Delete Book Cache in the reader menu): columns must be wide or narrow to match
  their content; the "Four columns" table must show its four columns side by side; the rows of the
  "Too many columns" table must be stacked one under another, with no cell lost.
- [ ] Delete the cache of a book with tables (e.g. a programming book), then open it in landscape
  and with a larger font: the table columns must fit on the screen, and the device must not hang.

## Chapter word cutting in Rust ("rust: our own chapter word cutting")

When a chapter opens, its text is cut into words. The byte loop that does this (the
`characterData` of ChapterHtmlSlimParser) is now in the `words` module of `bembeyaz-html`: a space
ends a word; a no-break space (U+00A0, U+202F) becomes a space word joined to the words on both
sides of it; a BOM is dropped; a word that fills the 200-byte buffer is cut at a code point
boundary, and the rest follows joined to it; `white-space: pre` keeps every space as a word, turns
a tab into four spaces and a line end into a line break; each word is added with the visible
position of its first byte. The word buffer, adding the word to the paragraph, line breaking and
capturing the drop cap stayed in C++; Rust reaches them through callbacks. The commit before this
one (f559aecd) pinned the cutting with 4 tests; in total 1013 host tests pass. During development,
a one-off diff test (not kept in the tree) ran the old and the new parser on 3000 random chapters
(random tags, inline styles, drop cap rules, tables, and text split at random bytes, in the middle
of UTF-8 sequences): every page that came out, in the form it is stored in the section cache, was
the same byte for byte. It was checked first that the test caught all the bugs planted on purpose
in eight branches of the old code (tab, the two no-break spaces, BOM, UTF-8 cut, drop cap cancel,
footnote link, `pre` line), and then that it also caught a bug planted in the new code. In the
emulator, the reading, clipping, XML and PNG scenarios, and opening the bidirectional, CSS,
hyphenation and XML books, gave the same screens as the previous version; the section cache that
four books built on the card was the same byte for byte, and the section build times stayed the
same (±1 ms). On x4pro, flash grew by 616 bytes; RAM did not change.

- [ ] Delete the cache of a few books and open them (Delete Book Cache in the reader menu): the
  text, the line breaks and the pages must be as in the previous firmware.
- [ ] Open a book with no-break spaces (French books usually use them with « » and before the
  marks ; : ! ?): the two sides of a no-break space must stay on the same line.
- [ ] Open a chapter with long text that has no spaces (for example Chinese or Japanese): the text
  must appear complete, with no broken characters.
- [ ] Open a chapter that uses a `<pre>` code block or `white-space: pre` (better if it has tabs):
  spaces, tabs and lines must be kept.
- [x] Open a book whose CSS has `::first-letter` or `initial-letter` (or that uses a drop cap
  span): the drop cap must look as before.

## Drop cap capture in Rust ("rust: our own initial letter capture")

Capturing the text of the drop cap is now also in the `words` module of `bembeyaz-html`: a drop
cap span takes its own text (at most three code points and 12 bytes); a `::first-letter` rule skips
the paragraph's leading spaces and takes the leading punctuation and the first letter; a capture
that is longer, or that (in a span) meets a space, is given up, and its text goes back to the word.
The capture state is kept in `bb_html_initial_letter`, which has the same layout as `DropCapState`
in C++; placing and drawing the drop cap stayed in C++. The commit before this one (df2b06b4)
pinned the capture with 4 tests; in total 1017 host tests pass. The chapter parser's diff test,
with the text split at whole characters as the XML parser does, found the old and the new pages
the same byte for byte on 3000 random chapters; a bug planted in the code on purpose was caught.
When the text was split artificially in the middle of UTF-8 sequences, 10 of the 3000 cases
differed: when the old code decided whether the captured character was punctuation, it read past
the end of the piece; the new code does not. When the old code was changed to read only inside the
piece, all these differences went away. The XML parser never splits text in the middle of a
character, so this difference does not appear in books. In the emulator, the reading scenario and
opening the CSS book with a drop cap and the bidirectional and hyphenation books gave the same
screens, and the CSS book's section cache was the same byte for byte. On x4pro, flash grew by 208
bytes; RAM did not change.

- [x] Open a book whose CSS has `::first-letter` or `initial-letter` (or that uses a drop cap
  span): the drop cap and the quote mark or punctuation before it must look as before; the rest of
  the paragraph must flow beside the letter. (Such a book can be made with
  `python3 emu/make_css_epub.py <folder>`: it has a `p.drop::first-letter` rule.)

## Inline style stack in Rust ("rust: our own inline style stack")

While a chapter is parsed, each open element that sets the text style (blocks, bold, italic,
underline or strikethrough, footnote link, superscript and subscript, styled span, `small`/`big`)
puts a style entry on a stack. The style of the words is found by merging these entries in order on
top of the block's style: the lines add up and a child cannot remove a line of its ancestors,
superscript and subscript replace each other, and bold and italic are inherited and can be changed.
Building and merging the entries is now in the `inline` module of `bembeyaz-css`; the stack is kept
in C++, with `bb_css_inline_entry` entries shared with Rust. The commit before this one (f960d2c9)
pinned the merging with 4 tests; in total 1021 host tests pass. The chapter parser's diff test,
with `u`, `s`, `strike`, `del`, `big`, `ins` and direction, alignment, decoration and weight styles
added to the tag pool, found the old and the new pages the same byte for byte on 4000 random
chapters; a bug planted in the code on purpose was caught (a second bug, which removed the rule
"superscript clears subscript", made no visible difference: in the word style, superscript already
wins over subscript). In the emulator, the reading and clipping scenarios and opening the CSS,
bidirectional, hyphenation and XML books gave the same screens as the previous version. On x4pro,
flash grew by 568 bytes (the C++ object shrank by 1073 bytes); RAM did not change.

- [ ] Open books with bold, italic, underlined and struck-through text, superscripts and subscripts
  (footnote numbers), small caps and right-to-left text (`python3 emu/make_css_epub.py <folder>`
  makes some of them): these styles must look as before.

## Inline font size in Rust ("rust: our own inline font scale")

While a chapter is parsed, the font size of each block is found like this: the parent block's size
is multiplied by the sizes of the open inline elements (an element in rem sets the value anew),
then the block's CSS `font-size` (directly if it is in rem, otherwise relative to the parent) or
the default h1–h6 ratios of headings are applied; the result is clamped between 0.5 and 3. The size
of the words in spans that carry an inline `font-size` is found the same way, by multiplying the
sizes on the stack with the block's size. These two calculations are now in the `inline` module of
`bembeyaz-css`; choosing the sized font and the size slots in the paragraph stayed in C++. The
commit before this one (a360179b) pinned the exact values of headings and inline sizes with 3
tests; in total 1024 host tests pass. The chapter parser's diff test, adding artificial sized fonts
to three quarters of the random chapters, found the old and the new pages the same byte for byte
on 4000 random chapters; a bug planted in the heading ratios on purpose was caught (a second bug,
which broke how the element after a zero-size element starts again from the paragraph's size, did
not show in the diff test; a Rust unit test catches it). In the emulator, the reading and
book-opening scenarios gave the same screens as the previous version with the CSS, bidirectional,
hyphenation and XML books. These two functions are the first to pass a `float` by value through the
FFI; the firmware's disassembly showed that GCC and Rust carry the argument and the return value in
the same registers (if this were wrong, the size of headings and sized spans would be broken). On
x4pro, flash grew by 240 bytes; RAM did not change.

- [x] Open a book with headings (h1–h6), paragraphs with a `font-size`, and spans with a larger or
  smaller font (`small`/`big` or with a `font-size`) (`python3 emu/make_css_epub.py <folder>` makes
  some of them): the font sizes must look as before.

## Font size of a span that holds the block, in Rust ("rust: our own block span font scale")

A span with a `font-size` that opens a paragraph and holds all its text (e.g. `<p><span
style="font-size: 2em">Birinci Bölüm</span></p>`) sizes the whole block: the span's size is found
as a multiple of the size of the paragraph, or of such spans around it (directly if it is in rem),
and clamped between 0.5 and 3. This calculation and the default ratios of `small` (0.83) and `big`
(1.2) are now in the `inline` module of `bembeyaz-css`, next to the heading ratios; tracking whether
the span holds the block stayed in C++. The commit before this one (aeab5b62) pinned the span's
size with 1 test; in total 1025 host tests pass. The chapter parser's diff test found the old and
the new pages the same byte for byte on 5000 random chapters; three bugs planted in the code on
purpose (the upper limit lowered to 2.9, `small` set to 0.84, a rem size also multiplied) were
caught. In the emulator, the reading and book-opening scenarios gave the same screens as the
previous version with the CSS, bidirectional, hyphenation and XML books. Passing a `float` as the
argument and the return value of `bb_css_span_scale` was checked again in the firmware's
disassembly. On x4pro, flash grew by 160 bytes; RAM did not change.

- [ ] Open a book whose chapter title is a large span inside a paragraph (e.g. `<p><span
  style="font-size: 2em">Birinci Bölüm</span></p>`) and that uses `small`/`big`: the title and
  these texts must be sized as before.

## Block style in Rust ("rust: our own block style")

The layout style of each block element (paragraph, heading, `div`, quote, list item…) is built
from its CSS: the top, bottom and side margins, the padding and the first-line indent are turned
from em, rem, points, percent or pixels into whole pixels, rounded toward zero; each side margin is
limited to 2em; the reader's alignment setting wins over the book's (unless "Book's Style" is
selected); page breaks and the direction are taken. In nested blocks, the side margins add up, the
top and bottom margins collapse to the larger one, the top and bottom padding add up, a page break
before the parent passes to the child, and the indent, alignment, direction and font size pass from
the parent to a child that does not set them. These calculations are now in the `block` module of
`bembeyaz-css`. The `BlockStyle` struct stayed in C++; Rust reads and writes it as `bb_css_block`,
whose layout is checked on both sides; the three bodies that go to Rust moved from the header to the
new `BlockStyle.cpp`. The commit before this one (fbbd39e4) pinned the behavior with 4 tests, and
this commit added a test that checks that an empty style gives the default block; in total 1030
host tests pass. The chapter parser's diff test, with large side margins, point and percent values,
negative indents and page breaks added to the random styles, found the old and the new pages the
same byte for byte on 5000 random chapters; all eight bugs planted in the code on purpose (the side
margin limit, the point and percent conversion, the indent, the indent and the page break passing
from the parent, the top margin collapse, the page break after the block) were caught; the bug in
the side margin limit only showed once these styles were added. In the emulator, the reading and
book-opening scenarios gave the same screens as the previous version with the CSS, bidirectional,
hyphenation and XML books. On x4pro, flash grew by 544 bytes (the C++ parser shrank by 585 bytes);
RAM did not change.

- [ ] Open a book with margins, indents, centered or right-aligned paragraphs, quotes, nested lists
  and page breaks (`python3 emu/make_css_epub.py <folder>` makes some of them); switch the
  **Paragraph Alignment** setting between "Book's Style" and another value and try again: the
  spacing, indents and alignment must look as before.

## Element classification in Rust ("rust: our own chapter element classification")

The chapter parser decides from the name of each opening and closing element what it is: `h1`–`h6`
count as a heading level; `p`, `li`, `div`, `br`, `blockquote`, `ul`, `ol`, `pre`, `figure` as a
block; `b`/`strong` as bold; `i`/`em` as italic; `u`/`ins` as underline; `del`/`s`/`strike` as
strikethrough; `img`/`image` as an image; `table`/`tr`/`td`/`th` as a table. These names are
compared case-sensitively. The text of `head`, `style`, `script`, `title` and `rp` elements is not
shown, whether their name is written in upper or lower case. `colspan` and `rowspan` are read only
if they are all digits; otherwise they are 1 (0 or a value that overflows means the rest of the
table). A link counts as inside the book if it does not start with `http://`, `https://`,
`mailto:`, `ftp://`, `tel:` or `javascript:`. These decisions are now in the `element` module of
`bembeyaz-html`. C++ calls the classification once at the opening and once at the closing and
looks at the flags (before, it scanned the tag lists one by one with `strcmp`); the tags that are
checked one by one, such as `a`, `hr`, `span`, `sup`/`sub`, `ruby`/`rt`, stayed in C++;
`VisibleTextUtils.h` was deleted. The commit before this one (9c1b95ec) pinned the behavior with 4
tests; in total 1034 host tests pass. The chapter parser's diff test, with `rp`, `title`, `th`,
`figure`, `ol`, `image` and the upper-case `STRONG`, `B` added to the tag pool, found the old and
the new pages the same byte for byte on 5000 random chapters. Of the seven bugs planted in the code
on purpose, five (those in the classification) were caught; the two planted in the link check did
not show in random chapters, and the pinning test and a Rust unit test catch them. In the emulator,
the reading and book-opening scenarios with the CSS, bidirectional, hyphenation and XML books, and
the JPEG and PNG scenarios with books that have images, gave the same screens as the previous
version. On x4pro, flash grew by 24 bytes; RAM did not change.

- [ ] Open a book with headings, bold, italic, underlined and struck-through text, images, tables
  (with cells that have `colspan`), ruby (furigana) text, footnotes and external links: all must
  look as before; the ruby brackets (`rp`) must not show, footnote links must open, and external
  links must not go to a place inside the book.

## Lines carried at a page break, in Rust ("rust: our own page break carry")

When the chapter parser breaks a page, it carries some of the page's last lines to the next page:
the first line of a paragraph that would be left alone at the bottom of the page; the last line
that would open the next page by itself, together with the line next to it (both, if they are the
paragraph's two lines on the page); and, if the start of a paragraph is carried, the lines of the
heading just above it. The carried lines can only come from the last text lines of the page that
the parser remembers (at most six), and the page is never left fully empty. An image or a paragraph
with a drop cap can also take the heading above it along when it does not fit; the drop cap
paragraph's own lines are not carried. These decisions are now in the `carry` module of
`bembeyaz-layout` (its ABI is the new `carry.h`). Finding how many of the page's last elements can
be carried, carrying the lines and their links, and keeping the positions of the last lines stayed
in C++. The commit before this one (9cd2d95b) pinned the decisions with 2 tests; in total 1036 host
tests pass. The chapter parser's diff test found the old and the new pages the same byte for byte
on 5000 random chapters; all five bugs planted in the code on purpose were caught. In the emulator,
the reading and book-opening scenarios with the CSS, bidirectional, hyphenation and XML books, and
the JPEG scenario with the book that has images, gave the same screens as the previous version. On
x4pro, flash grew by 216 bytes; RAM did not change.

- [ ] In a book with headings and long paragraphs, go forward a few pages: single lines of
  paragraphs left alone at the bottom or the top of a page, and headings at the bottom of a page,
  must be carried to the next page as before.

## Table row geometry in Rust ("rust: our own table row geometry")

When the chapter parser lays out a table row as a grid, it finds each column's width (the width
planned by the pre-scan, if there is one, otherwise an equal share of the page width), its left
edge (in a right-to-left row the columns are mirrored), the width of the cell frame (it goes one
pixel into the next column and stops at the page edge) and the largest top and bottom padding that
the row sets aside. The cells are stacked one under another if the row has a single cell, if a
cell's side padding fills its column, or if an unplanned column is narrower than three lines'
height. This calculation is now in the `table` module of `bembeyaz-layout`, next to the plan of
the column widths; laying out the cells, breaking the page and drawing the frames stayed in C++.
The commit before this one (f17b4572) pinned the row geometry with 6 tests; in total 1042 host tests
pass. The chapter parser's diff test, with well-formed tables added to the random chapters (cells
with or without frames, right-to-left rows, cells with different padding) and page widths between
120 and 481 pixels, found the old and the new pages the same byte for byte on 5000 random chapters;
all five bugs planted in the code on purpose were caught (before these tables were added, none of
them showed). In the emulator, the reading and book-opening scenarios gave the same screens as the
previous version with the CSS, bidirectional and XML (with tables) books. On x4pro, flash grew by
384 bytes; RAM did not change.

- [ ] Open a book with tables of two to four columns, with and without frames (a right-to-left
  table among them), in portrait and in landscape: the columns, frames and cell padding must look as
  before; with a large font size, or in rows with a single cell, the cells must be stacked one under
  another.

## Element names checked one by one, in Rust ("rust: our own chapter element names")

On opening and closing elements, the chapter parser still checked the names `a`, `hr`, `span`,
`sup`, `sub`, `ruby`, `rt`, `html`, `body`, `li`, `ol`, `ul`, `p`, `pre`, `br`, `table`, `tr`, `td`
and `th` one by one with `strcmp`. Now the classification also gives a name ID for these 19 names,
and C++ compares this ID (the names are still case-sensitive; at the opening, `body` is still
looked for without regard to case). The commit before this one (72df85eb) pinned with 1 test that
six of these names worked only in lower case; this commit also added a test that checks that the
`id` of `span`s does not count as an anchor (the diff test could not see this, because it does not
produce `id`s). In total 1044 host tests pass. The chapter parser's diff test found the old and the
new pages the same byte for byte on 5000 random chapters; of the six bugs planted in the name IDs on
purpose, five were caught, and the new test catches the one for `span`. In the emulator, the
reading and book-opening scenarios with the CSS, bidirectional, hyphenation and XML books, and the
JPEG scenario with the book that has images, gave the same screens as the previous version. On
x4pro, flash shrank by 96 bytes; RAM did not change.

- [ ] Open a book with lists (numbered and bulleted), horizontal rules, line breaks, preformatted
  (`pre`) text, superscripts and subscripts, ruby text, tables, and headings that the table of
  contents or footnotes link to: all must look as before, and links must go to the right place;
  after a KOReader sync, the reader must return to the same paragraph.

## List markers and ruby text in Rust ("rust: our own list markers and ruby text cleanup")

The chapter parser puts a marker at the start of each list item (not in lists with
`list-style-type: none`): a bullet (•) in unordered lists and in items outside a list, and in
ordered lists the next value of the list's own counter followed by a period (nested lists keep
their own counters). Ruby (furigana) text is also trimmed of leading and trailing spaces, tabs and
line ends, and each run of these inside it is reduced to a single space. These are now in
`bembeyaz-html`: the marker in the `list` module, the ruby text in the `words` module. The stack of
open lists stayed in C++; each list is kept in a `bb_html_list` struct shared with Rust. Ruby text
is now cleaned in place, without being copied to a new string. During the port, a bug was made that
would break the end of the marker for numbers 10,000 and above, and a Rust unit test caught it; now
only C++ puts the NUL at the end of the marker. The commit before this one (765dba1e) pinned the
behavior with 2 tests; in total 1046 host tests pass. The chapter parser's diff test found the old
and the new pages the same byte for byte on 5000 random chapters; of the five bugs planted in the
code on purpose, four were caught, and the pinning test catches the one planted in
`list-style-type: none`, which the random styles do not have. In the emulator, the reading and
book-opening scenarios gave the same screens as the previous version with the CSS, bidirectional
and XML books. On x4pro, flash grew by 20 bytes; RAM did not change.

- [ ] Open a book with numbered (nested ones too), bulleted and unmarked lists and ruby (furigana)
  text: the numbers, the bullets and the ruby text must look as before.

## Framed boxes in Rust ("rust: our own framed boxes")

The chapter parser puts a frame box (`PageBorderBox`) around an element that has a border or a
shade (a light gray background): the border is added to the element's padding, and the text is
kept at least 4 pixels away from the frame; at most 8 nested boxes are framed, and a box narrower
than 8 pixels is not framed. If a box spans more than one page, each page draws its own part of it,
and the edges cut at the page break stay open; an empty box (such as a line drawn with a border)
takes up as much space as its padding. The stack of open boxes and these calculations are now in
the `boxes` module of `bembeyaz-layout` (its ABI is the new `boxes.h`); the stack lives in a
228-byte opaque store inside the parser (before, it was a vector allocated on the heap). Building
the `PageBorderBox` objects, placing an empty box on the page and giving it the spacing of the
empty block stayed in C++. A new test was added that pins the edges cut when the page breaks; in
total 1064 host tests pass. Shaded, double, dashed, dotted and one-sided boxes, and boxes that are
narrow or have thick padding, were added to the chapter parser's diff test; on 1600 random chapters
the old and the new pages are the same byte for byte. Of the 16 bugs planted in the code on purpose,
this test caught 11 and the crate tests caught the other 5. In the emulator, the CSS book (framed
and shaded boxes) is the same pixel for pixel as the previous version in the reading and
book-opening scenarios. Flash grew by 484 bytes on x4pro and 416 bytes on x4c; static RAM did not
change.

- [x] In an EPUB with boxes that have a border or a gray background (e.g. note or quote boxes), go
  to a place where a box runs over more than one page: the frame must be drawn on each page as
  before, and the top and bottom edges cut at the page break must stay open. If an empty line drawn
  with a border falls at the very bottom of the page, it must be drawn on the next page as before.

## Drop cap placement in Rust ("rust: our own drop cap placement")

The chapter parser can make the first letter of a paragraph a drop cap: if the CSS
`::first-letter` has an `initial-letter`, a font size of at least 1.5 em, or `float: left`, or if
a span like this opens the paragraph. How many lines the letter spans (none if `initial-letter` is
less than 2; as many as the font size, between 2 and 4; 3 for a float with no size) is now in the
`inline` module of `bembeyaz-css`. The letter's scale (its top lines up with the capital letters of
the first line, and its baseline with the baseline of the last line; between 1 and 16 times), its
width (together with a space after it; at most half of the available room; the lines beside it
move right by this much) and the place of its baseline are also now in the new `drop_cap` module of
`bembeyaz-layout` (its ABI is the new `drop_cap.h`). The fonts' glyph metrics come through a
callback; if a font has no 'H', the value is estimated as before. Choosing the font, breaking the
page, building the `PageDropCap` and laying out the lines beside it stayed in C++. A new test was
added that pins the geometry; in total 1065 host tests pass. The chapter parser's diff test, this
time with a fake renderer whose glyph metrics change with the letter and the font (with some glyphs
missing), found the old and the new pages the same byte for byte on 11,600 random chapters. Of the
19 bugs planted in the code on purpose, this test catches 11 and the crate tests catch the other 8
(tests were added for 3 of them). In the emulator, the CSS book with drop caps is the same pixel for
pixel as the previous version in the reading and book-opening scenarios. Flash grew by 280 bytes on
x4pro and 284 bytes on x4c; static RAM did not change.

- [x] Open a book whose CSS has `::first-letter` or `initial-letter` (or that uses a drop cap
  span), and look at it a few times while changing the font size and the font: the drop cap must be
  as large as before, its top lined up with the capital letters of the first line, its base lined up
  with the base of the last line, and the lines beside it indented as before. A paragraph with a
  drop cap that falls at the bottom of the page must move to the next page together with its
  letter.

## Image display size in Rust ("rust: our own image display size")

When the chapter parser puts an image on the page, it finds its size like this: if the CSS has
both `width` and `height`, it takes both (em, rem, points or pixels; a percent width is of the
containing block, a percent height is of the page), and if they do not fit in the container or the
page, it shrinks both by the same ratio; if only one is there, it finds the other from the image's
aspect ratio and limits the side that does not fit; if the CSS has no size, it shrinks the image's
own size just enough to fit. This calculation is now in the new `image` module of `bembeyaz-css`.
Reading the image's size from the file, the container's width, breaking the page and building the
`PageImage` stayed in C++. The old calculation was copied as it was to
`test/css_parser/OldImageSize.h`, and on 200,000 random cases (zero, negative, fractional and very
large CSS values in every unit; image sizes that are zero, negative or up to 32767; lengths stored
with an undefined bit) the result is the same; in total 1067 host tests pass. Of the 17 bugs planted
in the code on purpose, this test caught 16; the other one did not change the result, and that
needless check was removed. A new book of images sized with CSS (`jpeg-sized.epub`, with its
scenario `emu/jpeg-sized.txt`) was added to the emulator; this scenario and the JPEG and PNG
scenarios are the same pixel for pixel as the previous version. Flash grew by 512 bytes on x4pro and
444 bytes on x4c; static RAM did not change.

- [x] In an EPUB that gives its images a width or a height in CSS (e.g. `width: 50%` or
  `height: 10em`), look at the pages with images: the images must look as large as before, with
  their proportions kept, and must not run off the page.

## Carried links and footnotes in Rust ("rust: our own carried links and footnotes")

When the chapter parser breaks a page and carries some of its last lines to the next page (a
paragraph's lone lines, headings), the links on those lines are carried too: the links that reach
below the top of the carried lines move to the next page, and the footnotes they point to become
footnotes of the next page; if a link that stays on the page also points to a footnote, the
footnote is on both pages. This split is now in the new `notes` module of `bembeyaz-section` (its
ABI is in `page.h`); the links and footnotes keep their order, and the split is done in place, with
no extra memory allocation. Adding the carried links to the new page, shifted, stayed in C++ in
`Page::moveNotesBelow` (the in-between list that was once set aside for carried links is gone), and
carrying the lines stayed in C++ in the parser. In total 1067 host tests pass. The chapter parser's
diff test (internal links make footnotes) found the old and the new pages the same byte for byte on
10,000 random chapters. Of the 10 bugs planted in the code on purpose, this test caught 7 and the
crate tests caught 2 (a test was added for one of them: links are added to the page from top to
bottom, so the carried ones are already at the end, and the diff test cannot see a broken order);
the remaining one does not change the result. In the emulator, the CSS book with footnotes and the
hyphenation book are the same pixel for pixel as the previous version. Flash grew by 812 bytes on
x4pro and 648 bytes on x4c; static RAM did not change.

- [x] In a book with footnotes, go to a place where a line with a footnote link is carried to the
  next page at a page break (e.g. the last line of a paragraph): tapping the link (with Touch Reader
  Controls on) must open the footnote as before; the footnote must show in the menu's "Links and
  footnotes" list on the page the carried line went to, and in the previous page's list only if a
  link there also points to it.

## Pager in Rust ("rust: our own chapter pager")

The chapter parser's page cursor (the next empty line, the lone lines and headings carried to the
next page when a page breaks, the first visible character of each page, and the place of framed
boxes, the horizontal rule, the table row separator, the drop cap and images) is in the new `pager`
module of `bembeyaz-layout`. The page objects stayed in C++. Every page of the 1600 chapters of
`test/chapter_corpus` is the same byte for byte; in the emulator, the screens of the chapter
scenarios are the same as with the previous commit. When memory runs out and a page or a box cannot
be built, the chapter build now stops with a readable error (before, `new Page()` crashed or the
element was dropped silently). Flash grew by 2.4 KB on x4pro (temporary FFI entry points; they will
go away when the parser's flow moves to Rust); static RAM did not change.

- [ ] Open a long novel and a book with images and tables, and turn pages through a few chapters:
  the first or last line of a paragraph must not be left alone on a page, a heading must not stay at
  the bottom of a page, framed boxes must go on with an open edge when the page breaks, and images
  must not run off the page.
- [ ] In the middle of a chapter, close the book and open it again: it must return to the same page
  (the visible positions of the pages); jump from the table of contents to a chapter and to a
  footnote (anchors).
- [ ] Compare the `Page render` times on the serial port with 0.0.3; the difference must be at the
  noise level.

## Anchors and footnotes in Rust ("rust: our own chapter anchors and note references")

The pages that the IDs in a chapter fall on, and the footnote references (number, target, and the
page of the line where the reference is shown), are in the new `bembeyaz-chapter` crate. The names
are kept in a single buffer: before, each anchor took 28 bytes (`std::string` and page) plus a
separate block for names longer than 15 characters, and each pending footnote took 292 bytes; the
288-byte record of the open link is also gone from the parser. The 1600 chapters of
`test/chapter_corpus` (TOC anchors and footnote links among them) are the same byte for byte; the
corpus caught 4 of the 4 bugs planted in the Rust code. Flash grew by 936 bytes on x4pro and 876
bytes on x4c.

- [ ] In a book with footnotes (e.g. one from Standard Ebooks), tap a footnote number to go to the
  footnote, then come back; the page's footnote list must show the right numbers.
- [ ] Jump from the table of contents to a subheading in the middle of a chapter: the right page
  must open; the chapters that the table of contents starts must begin on a new page.

## Chapter parser in Rust ("rust: our own chapter parser")

The whole flow of ChapterHtmlSlimParser (XHTML events, block and inline styles, building
paragraphs and breaking them into lines, drop caps, lists, tables, images, framed boxes, horizontal
rules, anchors, footnotes and placement on pages) is in `bembeyaz-chapter`. Only file reading, the
page and element objects (`Page`, `TextBlock`, `PageImage`...), reading the image size and the font
metrics stayed in C++; Rust asks for them through `bb_chapter_host` callbacks. Dictionary
definitions and the HTML descriptions on the Details screen also go through the same parser. The
1600 chapters of `test/chapter_corpus` (images, tables and footnotes among them) are the same byte
for byte; the corpus caught 6 of the 6 bugs planted in the Rust code. In the emulator, the screens
of 16 scenarios are the same as with the previous commit. Flash grew by 8.0 KB on x4pro and 7.8 KB
on x4c (the new Rust code is larger than the deleted C++ and FFI entry points); static RAM did not
change. When a chapter opens, the build runs on the 16 KB render task; the build that goes on in
the background while reading runs on the 8 KB loop task (`loop()`). The deepest call chain (text →
table cell → line breaking), computed from the disassembly, is about 0.45 KB deeper than before
(3.15 KB against 2.7 KB). The paragraph and the XML parser are kept on the heap (the paragraph was
already on the heap before, with `unique_ptr`). An SD read error now stops the build (before, the
parser was called with an invalid length).

- [ ] Open a few different books (a long novel, and books with images, tables, footnotes,
  right-to-left text, hyphenation and drop caps) and turn pages through a few chapters; the look
  must be the same as in 0.0.3.
- [ ] In Settings, change Reader Font Size, Line Spacing, Paragraph Alignment, Extra Paragraph
  Spacing and Paragraph Indentation, Character spacing and Word spacing, the Images setting
  (picture / alt text / none) and "Book's Style", then open a chapter again: the pages must be built
  with the new setting.
- [ ] Open a very large book that has a single chapter (the chapter is built in the background):
  the first pages must come at once, the estimated page count must get better as you go on, and
  there must be no crash while reading.
- [ ] Open a large chapter with many tables (e.g. a dictionary or a statistics book) with an SD
  card font, and read until the background build starts again (when you get near the end of the
  pages built so far): the build must finish without a stack overflow ("Stack canary" / "stack
  overflow").
- [ ] Look up a word in the dictionary (with a dictionary whose definitions are HTML), and open the
  Details screen of a book with an HTML description: the text must show on formatted pages.
- [ ] Compare the chapter build and `Page render` times on the serial port with 0.0.3; the
  difference must be at the noise level.
