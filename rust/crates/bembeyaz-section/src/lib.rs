//! The section cache (`sections/<spine>.bin`), as lib/Epub's `Section` writes and reads it.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod file;
pub mod page;

pub use bembeyaz_io::{Sink, Source};

#[cfg(test)]
extern crate std;
