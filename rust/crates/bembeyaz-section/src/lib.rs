//! The section cache (`sections/<spine>.bin`), as lib/Epub's `Section` writes and reads it.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

pub mod file;
mod io;
pub mod page;

/// The section file, read at any offset.
pub trait Source {
    fn size(&self) -> u32;
    /// Fills `buf` from `offset`; false when it cannot fill all of it.
    fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool;
}

/// The section file being built, written at any offset.
pub trait Sink {
    /// Writes `bytes` at `offset`; false when it cannot write all of them.
    fn write_at(&mut self, offset: u32, bytes: &[u8]) -> bool;
}

#[cfg(test)]
extern crate std;

/// A file held in memory, for the tests.
#[cfg(test)]
pub(crate) mod testing {
    #![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

    use std::vec::Vec;

    pub(crate) struct Bytes {
        pub(crate) data: Vec<u8>,
        /// How many reads it served.
        pub(crate) reads: usize,
    }

    impl Bytes {
        pub(crate) fn new(data: Vec<u8>) -> Self {
            Self { data, reads: 0 }
        }
    }

    impl crate::Source for Bytes {
        fn size(&self) -> u32 {
            self.data.len() as u32
        }
        fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
            self.reads += 1;
            let start = offset as usize;
            match self.data.get(start..start + buf.len()) {
                Some(src) => {
                    buf.copy_from_slice(src);
                    true
                }
                None => false,
            }
        }
    }

    impl crate::Sink for Bytes {
        fn write_at(&mut self, offset: u32, bytes: &[u8]) -> bool {
            let start = offset as usize;
            if self.data.len() < start + bytes.len() {
                self.data.resize(start + bytes.len(), 0);
            }
            self.data[start..start + bytes.len()].copy_from_slice(bytes);
            true
        }
    }
}
