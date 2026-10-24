//! The section cache (`sections/<spine>.bin`), as lib/Epub's `Section` writes and reads it, and
//! ([`notes`]) the links and footnotes a chapter's page break moves with its lines.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod file;
pub mod notes;
pub mod page;

pub use bembeyaz_io::{Sink, Source};

#[cfg(test)]
extern crate std;
