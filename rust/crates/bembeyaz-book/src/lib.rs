//! A book's metadata cache (`book.bin`), as lib/Epub's `BookMetadataCache` builds and reads it.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod cache;

#[cfg(test)]
extern crate std;
