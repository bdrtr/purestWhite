//! Buffered reading and writing over the SD card's files, for the cache-file crates (section,
//! book).
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

/// A file, read at any offset.
pub trait Source {
    fn size(&self) -> u32;
    /// Fills `buf` from `offset`; false when it cannot fill all of it.
    fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool;
}

/// A file being written, at any offset.
pub trait Sink {
    /// Writes `bytes` at `offset`; false when it cannot write all of them.
    fn write_at(&mut self, offset: u32, bytes: &[u8]) -> bool;
}

/// Writes from a position through a small buffer.
pub struct Out<'a, S: Sink> {
    sink: &'a mut S,
    at: u32,
    buf: [u8; 128],
    len: usize,
    ok: bool,
}

impl<'a, S: Sink> Out<'a, S> {
    pub fn new(sink: &'a mut S, at: u32) -> Self {
        Self {
            sink,
            at,
            buf: [0; 128],
            len: 0,
            ok: true,
        }
    }

    /// Where the next byte goes.
    #[must_use]
    pub fn position(&self) -> Option<u32> {
        self.at.checked_add(u32::try_from(self.len).ok()?)
    }

    /// Whether every write so far went through (after a [`flush`](Self::flush)).
    #[must_use]
    pub fn ok(&self) -> bool {
        self.ok
    }

    pub fn flush(&mut self) {
        if self.len > 0 {
            let written = self
                .buf
                .get(..self.len)
                .is_some_and(|b| self.sink.write_at(self.at, b));
            self.ok &= written;
            self.at = self.position().unwrap_or(u32::MAX);
            self.len = 0;
        }
    }

    /// Writes a string: its length (u32), then its bytes.
    pub fn push_string(&mut self, text: &[u8]) {
        self.push(&u32::try_from(text.len()).unwrap_or(u32::MAX).to_le_bytes());
        self.push(text);
    }

    pub fn push(&mut self, bytes: &[u8]) {
        if self.len + bytes.len() > self.buf.len() {
            self.flush();
        }
        if let Some(slot) = self.buf.get_mut(self.len..self.len + bytes.len()) {
            slot.copy_from_slice(bytes);
            self.len += bytes.len();
        } else {
            // Longer than the buffer: straight through.
            self.ok &= self.sink.write_at(self.at, bytes);
            match u32::try_from(bytes.len())
                .ok()
                .and_then(|n| self.at.checked_add(n))
            {
                Some(at) => self.at = at,
                None => self.ok = false,
            }
        }
    }

    /// Writes what is left; where the writing ended, unless a write failed.
    #[must_use]
    pub fn finish(&mut self) -> Option<u32> {
        self.flush();
        self.ok.then_some(self.at)
    }
}

/// Reads on from a position through the caller's buffer, as a file handle does: a read the file
/// ends in moves to its end and fails, and so does every read after it until the next seek.
pub struct Cursor<'a, S: Source> {
    src: &'a mut S,
    pos: u32,
    size: u32,
    buf: &'a mut [u8],
    buf_at: u32,
    buf_len: usize,
}

impl<'a, S: Source> Cursor<'a, S> {
    pub fn new(src: &'a mut S, at: u32, buf: &'a mut [u8]) -> Self {
        let size = src.size();
        Self {
            src,
            pos: at.min(size),
            size,
            buf,
            buf_at: 0,
            buf_len: 0,
        }
    }

    /// Moves to `at`; past the end, to the end.
    pub fn seek(&mut self, at: u32) {
        self.pos = at.min(self.size);
    }

    /// Moves past `n` bytes; false (at the end) when the file ends first.
    pub fn skip(&mut self, n: u32) -> bool {
        if n > self.remaining() {
            return self.give_up();
        }
        self.pos += n;
        true
    }

    #[must_use]
    pub fn position(&self) -> u32 {
        self.pos
    }

    /// The bytes left in the file.
    #[must_use]
    pub fn remaining(&self) -> u32 {
        self.size - self.pos
    }

    fn give_up(&mut self) -> bool {
        self.pos = self.size;
        false
    }

    /// Fills `out`; false (at the end) when the file ends first or cannot be read.
    pub fn read(&mut self, out: &mut [u8]) -> bool {
        if self.pos < self.buf_at {
            self.buf_len = 0;
        }
        let Ok(n) = u32::try_from(out.len()) else {
            return self.give_up();
        };
        if n > self.remaining() {
            return self.give_up();
        }
        let start = self.pos.wrapping_sub(self.buf_at) as usize;
        let buffered = self.pos >= self.buf_at
            && start + out.len() <= self.buf_len
            && self.buf.get(start..start + out.len()).is_some();
        if !buffered {
            if out.len() > self.buf.len() {
                // Larger than the buffer: straight in.
                if !self.src.read_at(self.pos, out) {
                    return self.give_up();
                }
                self.pos += n;
                return true;
            }
            let fill = self
                .buf
                .len()
                .min(usize::try_from(self.remaining()).unwrap_or(usize::MAX));
            let Some(window) = self.buf.get_mut(..fill) else {
                return self.give_up();
            };
            if !self.src.read_at(self.pos, window) {
                self.buf_len = 0;
                return self.give_up();
            }
            self.buf_at = self.pos;
            self.buf_len = fill;
        }
        let start = (self.pos - self.buf_at) as usize;
        match self.buf.get(start..start + out.len()) {
            Some(src) => {
                out.copy_from_slice(src);
                self.pos += n;
                true
            }
            None => self.give_up(),
        }
    }

    // A field of up to 4 bytes, little-endian; zero when it cannot be read. One out-of-line
    // reader for every width keeps the many field reads small.
    #[inline(never)]
    fn field(&mut self, n: usize) -> u32 {
        let mut b = [0u8; 4];
        let read = match b.get_mut(..n) {
            Some(bytes) => self.read(bytes),
            None => false,
        };
        if read { u32::from_le_bytes(b) } else { 0 }
    }

    pub fn u8(&mut self) -> u8 {
        self.field(1).to_le_bytes()[0]
    }

    pub fn i8(&mut self) -> i8 {
        i8::from_le_bytes([self.u8()])
    }

    pub fn u16(&mut self) -> u16 {
        let [low, high, _, _] = self.field(2).to_le_bytes();
        u16::from_le_bytes([low, high])
    }

    pub fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.u16().to_le_bytes())
    }

    pub fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.field(4).to_le_bytes())
    }

    pub fn u32(&mut self) -> u32 {
        self.field(4)
    }

    /// A u32 that must be there: None when the file ends first.
    pub fn u32_there(&mut self) -> Option<u32> {
        let mut b = [0u8; 4];
        self.read(&mut b).then(|| u32::from_le_bytes(b))
    }

    /// Moves past a string (u32 length, then its bytes); false when the file cannot hold it.
    pub fn skip_string(&mut self) -> bool {
        match self.string_len() {
            Some(len) => self.skip(u32::try_from(len).unwrap_or(u32::MAX)),
            None => false,
        }
    }

    /// The length of a string (u32, then its bytes), when the file holds that many bytes after
    /// it; else the read moves to the end.
    pub fn string_len(&mut self) -> Option<usize> {
        let mut b = [0u8; 4];
        let len = self.read(&mut b).then(|| u32::from_le_bytes(b));
        match len {
            Some(len) if len <= self.remaining() => Some(len as usize),
            _ => {
                self.give_up();
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]

    use super::{Cursor, Out};
    use std::vec::Vec;

    use crate::testing::Bytes;

    #[test]
    fn reads_fields_through_the_buffer_and_fails_at_the_end() {
        let mut buf = [0u8; 256];
        let mut file = Bytes::new((0..=255).chain(0..=255).map(|b| b as u8).collect());
        let mut c = Cursor::new(&mut file, 1, &mut buf);
        assert_eq!((c.u8(), c.u16(), c.i32()), (1, 0x0302, 0x0706_0504));
        let mut big = [0u8; 300];
        assert!(c.read(&mut big));
        assert_eq!((big[0], c.position()), (8, 308));
        assert!(c.read(&mut [0u8; 203]));
        assert_eq!(c.u16(), 0); // only one byte left
        assert_eq!((c.position(), c.u8()), (512, 0));
        assert_eq!(file.reads, 3);
        // Back to a place the buffer no longer holds.
        let mut c = Cursor::new(&mut file, 0, &mut buf);
        c.seek(300);
        assert_eq!(c.u8(), 44);
        c.seek(2);
        assert_eq!(c.u8(), 2);
        assert!(c.skip(10) && c.u8() == 13);
        c.seek(9999);
        assert_eq!((c.position(), c.u32()), (512, 0));

        let mut file = Bytes::new(Vec::from([5, 0, 0, 0, 1, 2, 3, 4, 5, 9]));
        let mut c = Cursor::new(&mut file, 0, &mut buf);
        assert_eq!(c.string_len(), Some(5));
        assert!(c.read(&mut [0u8; 5]));
        assert_eq!(c.string_len(), None);
        assert_eq!(c.remaining(), 0);
    }

    #[test]
    fn any_reads_never_panic() {
        let mut seed = 0x10c0_ffee_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let mut buf = [0u8; 48];
        for _ in 0..2000 {
            let data: Vec<u8> = (0..next() % 300).map(|_| next() as u8).collect();
            let mut file = Bytes::new(data);
            let size = next() % 7 + 1;
            let mut c = Cursor::new(&mut file, next() % 400, &mut buf[..size as usize * 6]);
            for _ in 0..40 {
                match next() % 8 {
                    0 => c.seek(next() % 400),
                    1 => {
                        let _ = c.skip(next() % 200);
                    }
                    2 => {
                        let _ = c.string_len();
                    }
                    3 => {
                        let _ = c.skip_string();
                    }
                    4 => {
                        let _ = c.read(&mut [0u8; 70][..(next() % 70) as usize]);
                    }
                    5 => {
                        let _ = c.u32_there();
                    }
                    _ => {
                        let _ = (c.u8(), c.i16(), c.i32());
                    }
                }
                assert!(c.position() <= 300);
            }
        }
    }

    #[test]
    fn writes_through_the_buffer_from_a_position() {
        let mut file = Bytes::new(Vec::from([7u8; 3]));
        let mut out = Out::new(&mut file, 3);
        out.push(&[1, 2]);
        out.push(&[9; 200]);
        out.push(&[3]);
        assert_eq!(out.finish(), Some(206));
        assert_eq!(&file.data[..6], &[7, 7, 7, 1, 2, 9]);
        assert_eq!(file.data[205], 3);
    }
}

#[cfg(any(test, feature = "testing"))]
extern crate std;

/// A file held in memory, for the tests (here and, with the `testing` feature, in other crates).
#[cfg(any(test, feature = "testing"))]
pub mod testing {
    #![allow(clippy::indexing_slicing, clippy::cast_possible_truncation)]

    use std::vec::Vec;

    pub struct Bytes {
        pub data: Vec<u8>,
        /// How many reads it served.
        pub reads: usize,
    }

    impl Bytes {
        #[must_use]
        pub fn new(data: Vec<u8>) -> Self {
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
