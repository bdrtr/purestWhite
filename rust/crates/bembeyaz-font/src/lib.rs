//! Fonts read from the SD card (`.cpfont`), as lib/EpdFont's `SdCardFont` reads them.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod advance;
pub mod cpfont;
pub mod glyph;
pub mod intervals;
pub mod kern;
pub mod page;

/// The font file, read from the current position.
pub trait Source {
    /// Fills `buf`; how many bytes were read.
    fn read(&mut self, buf: &mut [u8]) -> usize;
    /// Moves to `position`; false when it cannot.
    fn seek(&mut self, position: u32) -> bool;
}

#[cfg(test)]
extern crate std;
