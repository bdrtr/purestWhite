//! `RecentBooksStore`'s JSON (`src/RecentBooksStore.cpp`, `/.crosspoint/recent.json`): the books
//! opened last, most recent first. Adding, moving and pruning them stays in C++, which also keeps
//! how many the list holds (`MAX_RECENT_BOOKS`).

use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};

/// A recent book: its file, title, author and cover image path. `S` is the text: owned when read,
/// borrowed (or owned) when written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book<S = Vec<u8>> {
    pub path: S,
    pub title: S,
    pub author: S,
    pub cover_bmp_path: S,
}

/// Writes `recent.json`'s text for `books` (`toJson`, then `serializeJson`); none when memory ran
/// out.
#[must_use]
pub fn write<S: AsRef<[u8]>>(books: &[Book<S>]) -> Option<Vec<u8>> {
    let size: usize = books
        .iter()
        .map(|b| [&b.path, &b.title, &b.author, &b.cover_bmp_path].map(|f| f.as_ref().len()))
        .map(|lens| 64 + lens.iter().sum::<usize>())
        .sum();
    let mut w = Writer::with_capacity(16 + size);
    w.object(|w| {
        w.key(b"books");
        w.array(|w| {
            for book in books {
                w.object(|w| {
                    w.key(b"path");
                    w.string(book.path.as_ref());
                    w.key(b"title");
                    w.string(book.title.as_ref());
                    w.key(b"author");
                    w.string(book.author.as_ref());
                    w.key(b"coverBmpPath");
                    w.string(book.cover_bmp_path.as_ref());
                });
            }
        });
    });
    w.finish()
}

/// Reads `recent.json`'s text (`deserializeJson`, then `fromJson`): the first `most` items of its
/// "books" array (none when it has no such array), each item's fields read as strings, up to a
/// NUL as C++ reads a `const char*` (an item that is not an object, or a field that is not a
/// string, gives empty text).
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; the C++ store then keeps its
/// current books.
pub fn read(text: &[u8], most: usize) -> Result<Vec<Book>, Error> {
    let doc = parse(text)?;
    let field = |item: &Value, key: &[u8]| or_str(item.get(key), b"").to_vec();
    Ok(items(doc.get(b"books"))
        .iter()
        .take(most)
        .map(|item| Book {
            path: field(item, b"path"),
            title: field(item, b"title"),
            author: field(item, b"author"),
            cover_bmp_path: field(item, b"coverBmpPath"),
        })
        .collect())
}
