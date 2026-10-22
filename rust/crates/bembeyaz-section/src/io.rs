//! Buffered reading and writing over the section file.

use crate::{Sink, Source};

/// Writes from a position through a small buffer.
pub(crate) struct Out<'a, S: Sink> {
    sink: &'a mut S,
    at: u32,
    buf: [u8; 128],
    len: usize,
    ok: bool,
}

impl<'a, S: Sink> Out<'a, S> {
    pub(crate) fn new(sink: &'a mut S, at: u32) -> Self {
        Self {
            sink,
            at,
            buf: [0; 128],
            len: 0,
            ok: true,
        }
    }

    /// Where the next byte goes.
    pub(crate) fn position(&self) -> Option<u32> {
        self.at.checked_add(u32::try_from(self.len).ok()?)
    }

    /// Whether every write so far went through (after a [`flush`](Self::flush)).
    pub(crate) fn ok(&self) -> bool {
        self.ok
    }

    pub(crate) fn flush(&mut self) {
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

    pub(crate) fn push(&mut self, bytes: &[u8]) {
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
    pub(crate) fn finish(mut self) -> Option<u32> {
        self.flush();
        self.ok.then_some(self.at)
    }
}

/// Reads on from a position through a small buffer, as a file handle does: a read the file ends
/// in moves to its end and fails, and so does every read after it.
pub(crate) struct Cursor<'a, S: Source> {
    src: &'a mut S,
    pos: u32,
    size: u32,
    buf: [u8; 256],
    buf_at: u32,
    buf_len: usize,
}

impl<'a, S: Source> Cursor<'a, S> {
    pub(crate) fn new(src: &'a mut S, at: u32) -> Self {
        let size = src.size();
        Self {
            src,
            pos: at.min(size),
            size,
            buf: [0; 256],
            buf_at: 0,
            buf_len: 0,
        }
    }

    pub(crate) fn position(&self) -> u32 {
        self.pos
    }

    /// The bytes left in the file.
    pub(crate) fn remaining(&self) -> u32 {
        self.size - self.pos
    }

    fn give_up(&mut self) -> bool {
        self.pos = self.size;
        false
    }

    /// Fills `out`; false (at the end) when the file ends first or cannot be read.
    pub(crate) fn read(&mut self, out: &mut [u8]) -> bool {
        let Ok(n) = u32::try_from(out.len()) else {
            return self.give_up();
        };
        if n > self.remaining() {
            return self.give_up();
        }
        let start = (self.pos - self.buf_at) as usize;
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

    pub(crate) fn u8(&mut self) -> u8 {
        self.field(1).to_le_bytes()[0]
    }

    pub(crate) fn i8(&mut self) -> i8 {
        i8::from_le_bytes([self.u8()])
    }

    pub(crate) fn u16(&mut self) -> u16 {
        let [low, high, _, _] = self.field(2).to_le_bytes();
        u16::from_le_bytes([low, high])
    }

    pub(crate) fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.u16().to_le_bytes())
    }

    pub(crate) fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.field(4).to_le_bytes())
    }

    /// The length of a string (u32, then its bytes), when the file holds that many bytes after
    /// it; else the read moves to the end.
    pub(crate) fn string_len(&mut self) -> Option<usize> {
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
        let mut file = Bytes::new((0..=255).chain(0..=255).map(|b| b as u8).collect());
        let mut c = Cursor::new(&mut file, 1);
        assert_eq!((c.u8(), c.u16(), c.i32()), (1, 0x0302, 0x0706_0504));
        let mut big = [0u8; 300];
        assert!(c.read(&mut big));
        assert_eq!((big[0], c.position()), (8, 308));
        assert!(c.read(&mut [0u8; 203]));
        assert_eq!(c.u16(), 0); // only one byte left
        assert_eq!((c.position(), c.u8()), (512, 0));
        assert_eq!(file.reads, 3);

        let mut file = Bytes::new(Vec::from([5, 0, 0, 0, 1, 2, 3, 4, 5, 9]));
        let mut c = Cursor::new(&mut file, 0);
        assert_eq!(c.string_len(), Some(5));
        assert!(c.read(&mut [0u8; 5]));
        assert_eq!(c.string_len(), None);
        assert_eq!(c.remaining(), 0);
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
