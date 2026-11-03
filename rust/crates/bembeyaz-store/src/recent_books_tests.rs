//! `RecentBooksStore`'s own answers replayed: every step of testdata/recent-books-corpus.txt
//! (written by `test/recent_books/make_corpus.py`). The store's list steps (adding a book moves it
//! to the front, the list keeps 10) are mirrored here as the C++ store does them.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::corpus::{bytes, fuzz, hex, replay};
use crate::recent_books::{Book, read, write};
use std::format;
use std::string::String;
use std::vec::Vec;

const CORPUS: &str = include_str!("../testdata/recent-books-corpus.txt");
/// The C++ store's `MAX_RECENT_BOOKS`.
const MOST: usize = 10;

fn json_of(books: &[Book]) -> Vec<u8> {
    write(books).unwrap()
}

fn answer(books: &[Book]) -> String {
    let mut out = format!("{}", books.len());
    for b in books {
        let fields = [&b.path, &b.title, &b.author, &b.cover_bmp_path].map(|f| hex(f));
        out.push(' ');
        out.push_str(&fields.join(","));
    }
    format!("{out} | {}", String::from_utf8(json_of(books)).unwrap())
}

/// `addBook`, as the C++ store does it (its files are all present in the oracle).
fn add(books: &mut Vec<Book>, book: Book) {
    if let Some(at) = books.iter().position(|b| b.path == book.path) {
        books.remove(at);
    }
    books.insert(0, book);
    books.truncate(MOST);
}

fn lengths(books: &[Book]) -> String {
    format!(
        "{} {} {}",
        books.len(),
        books[0].title.len(),
        json_of(books).len()
    )
}

fn long(books: &mut Vec<Book>, n: usize) -> String {
    let title = "t".repeat(n);
    let json = format!("{{\"books\":[{{\"title\":\"{title}\"}}]}}");
    let read_answer = match read(json.as_bytes(), MOST) {
        Ok(read) => {
            *books = read;
            lengths(books)
        }
        Err(e) => format!("err {}", e.name()),
    };
    let book = Book {
        path: b"/long".to_vec(),
        title: title.into_bytes(),
        ..Book::default()
    };
    add(books, book);
    format!("{read_answer} | {}", lengths(books))
}

fn step(books: &mut Vec<Book>, a: &[&str]) -> String {
    match a[0] {
        "from" => match read(&bytes(a[1]), MOST) {
            Ok(read) => {
                *books = read;
                answer(books)
            }
            Err(e) => format!("err {}", e.name()),
        },
        "add" => {
            let f: Vec<Vec<u8>> = a[1..5].iter().map(|h| bytes(h)).collect();
            let book = Book {
                path: f[0].clone(),
                title: f[1].clone(),
                author: f[2].clone(),
                cover_bmp_path: f[3].clone(),
            };
            add(books, book);
            answer(books)
        }
        "long" => long(books, a[1].parse().unwrap()),
        other => panic!("unknown step {other}"),
    }
}

#[test]
fn every_step_of_the_corpus_is_the_stores() {
    let mut books = Vec::new();
    replay(CORPUS, 1000, |a| step(&mut books, a));
}

#[test]
fn broken_recent_files_are_read_without_a_fault() {
    fuzz(CORPUS, 20, |input| {
        let _ = read(input, MOST);
    });
}
