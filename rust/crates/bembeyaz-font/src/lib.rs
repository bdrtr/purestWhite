//! Fonts read from the SD card (`.cpfont`), as lib/EpdFont's `SdCardFont` reads them.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod advance;
pub mod compressed;
pub mod cpfont;
pub mod face;
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

/// A font held whole in memory, read as from its file.
pub struct Memory<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Memory<'a> {
    /// Reads `bytes` from their start.
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Memory { bytes, at: 0 }
    }
}

impl Source for Memory<'_> {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        let rest = self.bytes.get(self.at..).unwrap_or_default();
        let n = buf.len().min(rest.len());
        if let (Some(to), Some(from)) = (buf.get_mut(..n), rest.get(..n)) {
            to.copy_from_slice(from);
        }
        self.at += n;
        n
    }

    /// As a file: a position past the end cannot be reached; the end itself can.
    fn seek(&mut self, position: u32) -> bool {
        let at = position as usize;
        if at > self.bytes.len() {
            return false;
        }
        self.at = at;
        true
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod memory_tests {
    use super::{Memory, Source};

    #[test]
    fn memory_reads_and_seeks_as_a_file() {
        let bytes = [1u8, 2, 3, 4, 5];
        let mut m = Memory::new(&bytes);
        let mut buf = [0u8; 3];
        assert_eq!(m.read(&mut buf), 3);
        assert_eq!(buf, [1, 2, 3]);
        assert_eq!(m.read(&mut buf), 2);
        assert_eq!(buf[..2], [4, 5]);
        assert_eq!(m.read(&mut buf), 0);
        assert!(m.seek(1));
        assert_eq!(m.read(&mut buf), 3);
        assert_eq!(buf, [2, 3, 4]);
        assert!(m.seek(5));
        assert_eq!(m.read(&mut buf), 0);
        assert!(!m.seek(6));
    }
}
