//! `BookmarkFile`'s JSON (`src/util/BookmarkFile.cpp`, `/.crosspoint/bookmarks/<book>.json`): a
//! book's bookmarks, each a position (an `XPath`, a percentage, the chapter and pages, and the
//! visible-text offset when it has one), a summary and an optional name. Every field is read with
//! its plain fallback; cutting a name to `MAX_NAME_LENGTH`, and refusing to save a longer one, stay
//! in C++.

use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_f32, or_integer, or_str, parse};

/// A bookmark. `S` is the text: owned when read, borrowed (or owned) when written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bookmark<S = Vec<u8>> {
    pub xpath: S,
    pub summary: S,
    /// Empty when it has none (and then not written).
    pub name: S,
    pub percentage: f32,
    pub spine_index: u16,
    pub chapter_page_count: u16,
    pub chapter_progress: u16,
    /// The visible-text offset, for a bookmark that has one (written only then).
    pub visible_text_offset: Option<u32>,
}

/// A bookmark of a file's item, each field read with its plain fallback: texts `| ""` (up to a
/// NUL, as C++ reads a `const char*`), the percentage `| 0.0f`, the pages `| uint16_t(0)`, and the
/// offset, when it is not null, `| uint32_t(0)`. An item that is not an object gives every fallback
/// (and no offset).
fn bookmark(item: &Value) -> Bookmark<&[u8]> {
    let text = |key: &[u8]| or_str(item.get(key), b"");
    let page = |key: &[u8]| or_integer(item.get(key), 0u16);
    let offset = item.get(b"vo");
    Bookmark {
        xpath: text(b"xpath"),
        summary: text(b"summary"),
        name: text(b"name"),
        percentage: or_f32(item.get(b"percentage"), 0.0),
        spine_index: page(b"si"),
        chapter_page_count: page(b"pc"),
        chapter_progress: page(b"pp"),
        visible_text_offset: (!Value::is_null(offset)).then(|| or_integer(offset, 0u32)),
    }
}

/// Reads a bookmarks file's text (`deserializeJson`) and hands `each` every item of its
/// "bookmarks" array (none when it has no such array), with how many there are; the texts are views
/// into the parsed text, valid during the call.
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; `each` is not called, and C++
/// then has no bookmarks.
pub fn read_each(text: &[u8], mut each: impl FnMut(&Bookmark<&[u8]>, usize)) -> Result<(), Error> {
    let doc = parse(text)?;
    let list = items(doc.get(b"bookmarks"));
    for item in list {
        each(&bookmark(item), list.len());
    }
    Ok(())
}

/// [`read_each`]'s bookmarks, copied out.
///
/// # Errors
/// As [`read_each`].
pub fn read(text: &[u8]) -> Result<Vec<Bookmark>, Error> {
    let mut list = Vec::new();
    read_each(text, |b, _| {
        list.push(Bookmark {
            xpath: b.xpath.to_vec(),
            summary: b.summary.to_vec(),
            name: b.name.to_vec(),
            percentage: b.percentage,
            spine_index: b.spine_index,
            chapter_page_count: b.chapter_page_count,
            chapter_progress: b.chapter_progress,
            visible_text_offset: b.visible_text_offset,
        });
    })?;
    Ok(list)
}

fn write_bookmark<S: AsRef<[u8]>>(w: &mut Writer, b: &Bookmark<S>) {
    let page = |w: &mut Writer, key: &[u8], n: u16| {
        w.key(key);
        w.unsigned(n.into());
    };
    w.object(|w| {
        w.key(b"xpath");
        w.string(b.xpath.as_ref());
        w.key(b"percentage");
        w.f32(b.percentage);
        w.key(b"summary");
        w.string(b.summary.as_ref());
        if !b.name.as_ref().is_empty() {
            w.key(b"name");
            w.string(b.name.as_ref());
        }
        page(w, b"si", b.spine_index);
        page(w, b"pc", b.chapter_page_count);
        page(w, b"pp", b.chapter_progress);
        if let Some(offset) = b.visible_text_offset {
            w.key(b"vo");
            w.unsigned(offset.into());
        }
    });
}

/// Writes a bookmarks file's text (`serializeJson`) for `bookmarks`; none when memory ran out.
#[must_use]
pub fn write<S: AsRef<[u8]>>(bookmarks: &[Bookmark<S>]) -> Option<Vec<u8>> {
    let size: usize = bookmarks
        .iter()
        .map(|b| 128 + b.xpath.as_ref().len() + b.summary.as_ref().len() + b.name.as_ref().len())
        .sum();
    let mut w = Writer::with_capacity(16 + size);
    w.object(|w| {
        w.key(b"bookmarks");
        w.array(|w| bookmarks.iter().for_each(|b| write_bookmark(w, b)));
    });
    w.finish()
}
