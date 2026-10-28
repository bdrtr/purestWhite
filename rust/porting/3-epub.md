# Stage 3: the EPUB core

Stage 3 of the [porting order](../README.md) moves the EPUB core to Rust: the modules of Epub (all
sections below except the last two) and of Section (the last two). It is the largest part (~19k
lines), so it moves sub-module by sub-module. ✅ marks a module that is done.

## Book metadata cache ✅

In `bembeyaz-book`'s `cache` module: the records of BookMetadataCache's `book.bin` and of the two
temporary files it is built from; reading the file header and the metadata; spine and table of
contents entries through the LUT; cumulative size windows and their lookups; building `book.bin`
(the LUTs, each spine item's table of contents entry, the cumulative sizes); and finding the spine
item of a table of contents href (with a hash index or by scanning the file). The format's version
is there too.

In C++: the files and their buffers, memory (index, window and mapping tables), the sizes in the
ZIP (batch lookup included), the 400-item threshold, sorting the index, the lock and the NFC
composition of titles.

Reading and writing use the buffered reader and writer of `bembeyaz-io`, shared with
`bembeyaz-section`.

## Table of contents readers ✅

`bembeyaz-toc` replaces TocNcxParser and TocNavParser. It reads the XML with `bembeyaz-xml` and the
paths with `bembeyaz-path` directly. Both are thin classes on top of the `TocParser` layer.

## HTML entity table ✅

`bembeyaz-html`. `lookupHtmlEntity`/`htmlEntityText` are a layer for the C++ callers.

## CSS parser ✅

The cache format is the same. File operations are in C++.

## Hyphenation ✅

The hypher tries are in the crate. The selected language is in C++.

## OPF ✅

The package document. The manifest is kept in memory instead of a temporary file on the SD card,
limited to 2 MiB. Path resolution is `bembeyaz-path`; FsHelpers uses it too.

`META-INF/container.xml` is also in `bembeyaz-opf`, in its `container` module; ContainerParser is a
thin C++ layer on top of it.

## ParsedText ✅

Paragraph layout: tokens, line breaking, hyphenation, justification, bidi, ruby. Measurements come
from GfxRenderer through a callback.

The chapter parser now builds its paragraphs with `bembeyaz-chapter`'s `Text`. The C++ `ParsedText`
stays for the settings screen's preview and for the tests (link targets and size slots exist there
a second time).

## Lines carried at the page end ✅

In `bembeyaz-layout`'s `carry` module: ChapterHtmlSlimParser's decision, when a page breaks, to
carry to the next page a paragraph's lone first or last line together with its neighbours, and a
heading's lines together with what follows (the lines of a paragraph with a drop cap stay in place).

In the pager (below): how many of the page's last elements can be carried, the positions and
visible-text positions of the last lines, and whether a heading fits on one page together with an
image.

Moving the lines is in C++.

## Table column widths and row geometry ✅

In `bembeyaz-layout`'s `table` module: ChapterHtmlSlimParser's table pre-scan, the CSS auto table
width sharing, and the geometry of a grid row (column widths, left edges mirrored in a right-to-left
row, the width of cell borders, the row's top and bottom padding, the decision to stack the row's
cells one under the other instead of in a grid). Reading the file and the text width come through
callbacks.

Laying out the cells is in `bembeyaz-chapter`, drawing the borders in C++, and whether the row fits
on the page and breaking the page in the pager.

## Splitting the chapter text into words ✅

The byte loop in ChapterHtmlSlimParser's `characterData` is in `bembeyaz-html`'s `words` module:
spaces, no-break spaces (U+00A0, U+202F), the BOM, cutting a full word buffer at a code point
boundary, `white-space: pre` spaces, tabs and lines, visible-text positions, and capturing the text
of the drop cap (at most three code points and 12 bytes; with `::first-letter`, the leading
punctuation and the first letter; the capture state is `InitialLetter`).

The word buffer, starting the capture and closing the span are in `bembeyaz-chapter`. Drawing the
drop cap is in C++.

## Inline style stack ✅

In `bembeyaz-css`'s `inline` module: how ChapterHtmlSlimParser builds the inline style entries of
the open elements (block, `u`/`s`, footnote link, `b`, `i`, `sup`/`sub`, `small`/`big` and span),
and how it merges them over the block's style into the words' style (bold, italic, decoration
lines, small caps, white space, direction, alignment, superscript and subscript).

The stack is in `bembeyaz-chapter`.

## Inline font size ✅

In `bembeyaz-css`'s `inline` module: finding the block's font size (the sizes of the open inline
elements; the CSS `font-size` directly when it is in rem, otherwise relative to the parent; the
h1–h6 default ratios of headings; the 0.5–3 limit), the word's inline font size from the stack, the
size of a span that holds all of the block's text, and the default ratios of `small`/`big`.

In `bembeyaz-chapter`: choosing a sized font, the size slots (including the 0.5–3 limit on the
word's size after the slot is chosen), and tracking whether the span holds the block. The font
itself comes through a callback.

## Block style ✅

In `bembeyaz-css`'s `block` module: building BlockStyle from CSS (converting margins, paddings and
the indent from em, rem, points, percent and pixels to whole pixels; the 2em limit on the side
spacing; the reader's alignment setting (when "Book's Style" is not selected); page breaks;
direction), merging nested blocks horizontally and vertically, and adding the bottom spacing.

The `BlockStyle` struct is in C++. Rust reads and writes it as `bb_css_block`, whose layout is
checked on both sides.

## Element classification ✅

In `bembeyaz-html`'s `element` module: how ChapterHtmlSlimParser finds, from the tag name, an
element's heading level and whether it is a block, bold, italic, underlined or struck through, an
image, a table, or an element whose text is not shown; reading `colspan`/`rowspan` values; and
checking whether a link stays inside the book. `VisibleTextUtils.h` was deleted.

The 19 tag names the parser checks one by one (`a`, `hr`, `span`, `sup`/`sub`, `ruby`/`rt`,
`html`/`body`, the list names, `p`, `pre`, `br` and the table names) are also compared through a
name id that the classification carries. The case-insensitive check for the opening `body` is in
`bembeyaz-chapter`.

## List markers and ruby text ✅

Choosing and writing a list item's marker (none in a `list-style-type: none` list, a bullet, or a
number and a period) is in `bembeyaz-html`'s `list` module. Stripping ruby text of leading and
trailing spaces, tabs and line breaks, and reducing those inside it to a single space, is in the
`words` module.

The stack of open lists is in `bembeyaz-chapter`.

## Framed boxes ✅

In `bembeyaz-layout`'s `boxes` module, for ChapterHtmlSlimParser's elements with a border or a
shade (a light gray background): the stack of open boxes (at most 8), adding the border to the
padding and keeping the text at least 4 pixels away from the frame, each box's part on a page when
the page breaks (the cut edges stay open), and the height of a box without content.

The stack is inside the pager. Placing a box without content on the page is in the pager too.
Giving an empty block's spacing to the box is in `bembeyaz-chapter`. Building the `PageBorderBox`
objects is in C++.

## Pager ✅

The chapter parser's page cursor, in `bembeyaz-layout`'s `pager` module, inside the chapter parser:
the next free line on the page, when the page is full and which last lines move to the next page
when it breaks, the first visible character each page shows, the number of pages completed (for
anchors), keeping heading lines together with what follows, and the positions of lines, horizontal
rules, table row separators, the drop cap, images and boxes without content.

The page and element objects stay in C++. The pager has them opened, moved and published through
`bb_chapter_host` callbacks. Each element put on the page is reported to it, so that it knows
itself how many of the page's last elements are text lines.

The loop that places a table row's lines and the page break requests are in `bembeyaz-chapter`.

## The chapter parser's flow ✅

In `bembeyaz-chapter`'s `chapter`, `flow`, `text`, `place`, `table` and `blocks` modules:
ChapterHtmlSlimParser's handling of XHTML events, applying block and inline styles, building
paragraphs and breaking them into lines (`Text`, the counterpart of ParsedText), the drop cap,
lists, table cells and rows, images, framed boxes, horizontal rules, and laying all of it out onto
pages.

The interface to the C++ side is in the `host` module; its ABI is `chapter_parser.h`. Strings given
to the C++ side end with NUL, as C strings, because `TextBlock` and `Page` take them that way.

In C++, through `bb_chapter_host` callbacks: reading the file, the `Page` and element objects,
building lines as `TextBlock`s, reading image sizes, and font metrics.

## Anchors and footnote references ✅

In the new `bembeyaz-chapter` crate's `anchors` and `footnotes` modules, read through `chapter.h`:

- the page each `id` in the chapter falls on (it waits until the previous block is placed on a page;
  a section that the table of contents starts begins on a new page; ids in `span`s and ids past the
  first 1024 are dropped unless the table of contents names them);
- for footnote references, their number (with spaces and square brackets dropped), their target,
  and their wait until the line that shows them is put on a page.

Names and references are in a single buffer, each ending with NUL (before, each anchor was a
`std::string` and each pending footnote was 292 bytes). Keeping the link in the block is in
`bembeyaz-chapter` too.

Adding footnotes to the page and writing the anchor file are in C++.

## Drop cap layout ✅

How many lines the drop cap spans, from the style of `::first-letter` or of the span that opens the
paragraph (`initial-letter`, a font size of at least 1.5 em, or `float: left`), is in
`bembeyaz-css`'s `inline` module.

The letter's scale (its top aligned with the first line's capitals, its baseline with the last
line's; 1–16 times), its width (with one space after it, at most half the room) and its baseline
are in `bembeyaz-layout`'s `drop_cap` module. Glyph metrics come through a callback.

Choosing the font and laying out the lines beside it is in `bembeyaz-chapter`, building the
`PageDropCap` in C++, and its position and the page break in the pager.

## Image size ✅

In `bembeyaz-css`'s `image` module: finding the size at which an image in the chapter is shown,
from the CSS `width` and `height` (em, rem, points, pixels; a percentage width is of the containing
block, a percentage height of the page), keeping the image's ratio on the side that is not given,
or from the image's own size when there is no CSS; and fitting it into the container and the page.

Finding the container's width is in `bembeyaz-chapter`. Reading the image's size and building the
`PageImage` are in C++. Its position and the page break are in the pager.

## Links and footnotes carried at the page end ✅

In `bembeyaz-section`'s `notes` module: when a page breaks, separating out the links that reach
below the top of the lines carried to the next page, and the footnotes they point to (a footnote
that a remaining link also points to stays on both pages).

Adding the carried links to the new page, shifted, is in `Page::moveNotesBelow`. Moving the lines
is in C++.

## Section: section file tables ✅

In `bembeyaz-section`'s `file` module: checking the header and the layout settings of a cached
section file, the progress record of a build left half done, reading and searching the tables of
pages, anchors, paragraphs, list items and visible-text positions, and, at the end of a build,
writing these tables and committing the file.

The format's version and its history are there too. A C++ layout change that changes the pages
raises the version there.

In C++: opening files, renaming, running the build, and the lookups made during the build in the
in-memory tables and anchors (except the visible-position scan).

## Section: section pages ✅

In `bembeyaz-section`'s `page` module: reading, checking and writing a page body (lines and the
TextBlock arena, ruby, line style, word fonts; image, horizontal rule, drop cap, border box;
footnotes and links).

It builds the objects through C++ callbacks (`bb_page_builder`). C++ allocates the memory (arena,
ruby, word fonts, image paths, footnote and link arrays). Drawing is in C++.
