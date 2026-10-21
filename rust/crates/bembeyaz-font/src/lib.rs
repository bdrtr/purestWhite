//! Fonts read from the SD card (`.cpfont`), as lib/EpdFont's `SdCardFont` reads them.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod cpfont;

#[cfg(test)]
extern crate std;
