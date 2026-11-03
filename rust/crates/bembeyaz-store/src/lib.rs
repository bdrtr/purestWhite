//! The firmware's settings files (`/.crosspoint/*.json`, written by the stores built on
//! `lib/Serialization/PersistableStore.h`) read and written as the C++ stores did with
//! ArduinoJson: the same JSON out, the same values from any file, a field of the wrong type
//! falling back to its default. One module per store, each with `read` and `write`; each one's
//! golden corpus is that store's C++ answers.
//!
//! - [`state`]: `CrossPointState` (`state.json`): the open book, the recent sleep images; corpus
//!   from `test/crosspoint_state/make_corpus.py`.
//! - [`recent_books`]: `RecentBooksStore` (`recent.json`): the books opened last; corpus from
//!   `test/recent_books/make_corpus.py`.
//! - [`bookmarks`]: `BookmarkFile` (`bookmarks/<book>.json`): a book's bookmarks; corpus from
//!   `test/bookmark_file/make_corpus.py`.
//! - [`koreader_credentials`]: `KOReaderCredentialStore` (`koreader.json`): the KOReader sync
//!   account and settings as stored; corpus from `test/koreader_credentials/make_corpus.py`.
//! - [`records`]: the OPDS tokens, the LCP passphrases and the LCP device identity, records of
//!   texts; corpus from `test/token_stores/make_corpus.py`.
//! - [`opds_servers`]: `OpdsServerStore` (`opds.json`): the OPDS catalogs, their sign-ins and
//!   headers as stored; corpus from `test/opds_servers/make_corpus.py`.
//! - [`wifi_credentials`]: `WifiCredentialStore` (`wifi.json`): the saved Wi-Fi networks, their
//!   passwords as stored; corpus from `test/wifi_credentials/make_corpus.py`.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

#[cfg(test)]
mod corpus;

pub mod bookmarks;
#[cfg(test)]
mod bookmarks_tests;
pub mod field;
pub mod headers;
pub mod koreader_credentials;
#[cfg(test)]
mod koreader_credentials_tests;
pub mod opds_servers;
#[cfg(test)]
mod opds_servers_tests;
pub mod recent_books;
#[cfg(test)]
mod recent_books_tests;
pub mod records;
#[cfg(test)]
mod records_tests;
pub mod state;
#[cfg(test)]
mod state_tests;
pub mod wifi_credentials;
#[cfg(test)]
mod wifi_credentials_tests;

pub use bembeyaz_jsondoc::Error;
