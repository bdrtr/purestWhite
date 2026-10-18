//! Streaming DEFLATE and zlib decompression over caller-owned storage.
//!
//! [`Inflater`] decodes into one buffer that holds the whole output, so it
//! needs no window. With a borrowed 32 KiB [`Window`] it hands out the output
//! in pieces of any size. Nothing allocates: the caller decides where the
//! ~10 KiB decoder state and the window live (heap, or the framebuffer lent
//! out during a chapter build).
//!
//! The compressed data comes from untrusted files. Corrupt or truncated input
//! yields an [`Error`]; it never reads or writes outside the given buffers.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use core::ffi::CStr;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::ptr;
use miniz_oxide::inflate::TINFLStatus;
use miniz_oxide::inflate::core::inflate_flags::{
    TINFL_FLAG_HAS_MORE_INPUT, TINFL_FLAG_PARSE_ZLIB_HEADER,
    TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
};
use miniz_oxide::inflate::core::{DecompressorOxide, decompress};

/// Size of a [`Window`]: the largest distance DEFLATE can refer back.
pub const WINDOW_SIZE: usize = miniz_oxide::inflate::core::TINFL_LZ_DICT_SIZE;

/// Compressed input, read like `std::io::BufRead` without I/O errors.
pub trait Input {
    /// The next unread bytes, refilling when none are buffered. Empty means
    /// the end of the input (a read error looks the same and shows up as
    /// [`Error::Truncated`]).
    fn fill_buf(&mut self) -> &[u8];
    /// Marks `n` bytes returned by [`Input::fill_buf`] as read.
    fn consume(&mut self, n: usize);
}

impl Input for &[u8] {
    fn fill_buf(&mut self) -> &[u8] {
        self
    }

    fn consume(&mut self, n: usize) {
        *self = self.get(n..).unwrap_or_default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Raw DEFLATE, as in ZIP entries.
    Deflate,
    /// DEFLATE inside a zlib header and Adler-32 trailer, as in PNG.
    Zlib,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// The output is full and more data remains.
    More,
    /// The stream ended; the output may not be full.
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The input ended before the stream did.
    Truncated,
    /// The input is not valid DEFLATE or zlib data.
    Corrupt,
    /// The zlib Adler-32 trailer does not match the output.
    Checksum,
    /// The output does not continue this stream's earlier output.
    BadOutput,
}

impl Error {
    /// Every error, in the order of their C status codes.
    pub const ALL: [Error; 4] = [
        Error::Truncated,
        Error::Corrupt,
        Error::Checksum,
        Error::BadOutput,
    ];

    /// A short description, NUL-terminated so C code can log it too.
    #[must_use]
    pub fn message(self) -> &'static CStr {
        match self {
            Error::Truncated => c"compressed data is truncated",
            Error::Corrupt => c"compressed data is corrupt",
            Error::Checksum => c"adler32 checksum mismatch",
            Error::BadOutput => c"output does not continue the previous read",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message().to_str().unwrap_or_default())
    }
}

impl core::error::Error for Error {}

/// The decoder. On its own it writes the whole output into one buffer:
/// back-references resolve inside that buffer, so no window is needed, but
/// every call must pass the same buffer with the earlier output in front.
/// [`Window`] hands the output out in pieces instead.
pub struct Inflater {
    state: DecompressorOxide,
    format: Format,
    done: bool,
}

impl Inflater {
    /// Tests only: the ~10 KiB result may be built on the stack and moved.
    #[cfg(test)]
    #[must_use]
    pub fn new(format: Format) -> Self {
        Self {
            state: DecompressorOxide::new(),
            format,
            done: false,
        }
    }

    /// Sets up an inflater in `slot` without building it on the stack first,
    /// which Rust does not promise for a returned value even when inlined.
    pub fn init(slot: &mut MaybeUninit<Self>, format: Format) -> &mut Self {
        let this = slot.as_mut_ptr();
        // SAFETY: in miniz_oxide 0.9.1 (pinned) `DecompressorOxide::default()`
        // is all zeros: integers, integer arrays and `State::Start = 0`. Miri
        // checks this in `init_matches_new`.
        unsafe {
            (&raw mut (*this).state).write_bytes(0, 1);
            (&raw mut (*this).format).write(format);
            (&raw mut (*this).done).write(false);
            slot.assume_init_mut()
        }
    }

    /// Changes the format; takes effect only before the first read.
    pub fn set_format(&mut self, format: Format) {
        self.format = format;
    }

    #[must_use]
    pub fn is_done(&self) -> bool {
        self.done
    }

    /// Decodes into `out[pos..]` until it is full or the stream ends.
    /// `out[..pos]` must hold everything this stream produced so far.
    ///
    /// Returns how many bytes were added, which counts even on an error: they
    /// are the output decoded before it. The error is [`Error::BadOutput`]
    /// when `pos` is past the end of `out`; otherwise the input is truncated,
    /// corrupt or fails its Adler-32 check.
    pub fn decompress_into(
        &mut self,
        input: &mut impl Input,
        out: &mut [u8],
        pos: usize,
    ) -> (usize, Result<Status, Error>) {
        if pos > out.len() {
            return (0, Err(Error::BadOutput));
        }
        let mut at = pos;
        while !self.done {
            if at == out.len() {
                return (at - pos, Ok(Status::More));
            }
            let (written, result) = self.step(input, out, at, false);
            at += written;
            if let Err(error) = result {
                return (at - pos, Err(error));
            }
        }
        (at - pos, Ok(Status::Done))
    }

    /// Runs the decoder once on the buffered input and returns the bytes
    /// written at `out[pos..]`, even on an error. A `wrapping` `out` is a
    /// power-of-two ring.
    fn step(
        &mut self,
        input: &mut impl Input,
        out: &mut [u8],
        pos: usize,
        wrapping: bool,
    ) -> (usize, Result<(), Error>) {
        let data = input.fill_buf();
        let mut flags = 0;
        if self.format == Format::Zlib {
            flags |= TINFL_FLAG_PARSE_ZLIB_HEADER;
        }
        // An empty refill is the end of the input; until then more may follow.
        if !data.is_empty() {
            flags |= TINFL_FLAG_HAS_MORE_INPUT;
        }
        if !wrapping {
            flags |= TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF;
        }
        let (status, consumed, written) = decompress(&mut self.state, data, out, pos, flags);
        input.consume(consumed);
        let result = match status {
            TINFLStatus::Done => {
                self.done = true;
                Ok(())
            }
            // Both report progress; a step that made none would loop forever.
            TINFLStatus::NeedsMoreInput | TINFLStatus::HasMoreOutput
                if consumed > 0 || written > 0 =>
            {
                Ok(())
            }
            TINFLStatus::NeedsMoreInput | TINFLStatus::FailedCannotMakeProgress => {
                Err(Error::Truncated)
            }
            TINFLStatus::Adler32Mismatch => Err(Error::Checksum),
            _ => Err(Error::Corrupt),
        };
        (written, result)
    }
}

/// Hands an [`Inflater`]'s output out in pieces of any size, through a
/// 32 KiB window that holds the history back-references need.
///
/// The decoder fills the window as far as it can in one go (several times
/// faster than decoding piecemeal); what the caller has not taken yet stays
/// pending in the window until the next read.
pub struct Window<'w> {
    buf: &'w mut [u8; WINDOW_SIZE],
    /// Where the decoder writes next.
    pos: usize,
    /// Decoded bytes not yet handed out.
    pending: Range<usize>,
}

impl<'w> Window<'w> {
    /// The buffer's previous contents are irrelevant.
    #[must_use]
    pub fn new(buf: &'w mut [u8; WINDOW_SIZE]) -> Self {
        Self {
            buf,
            pos: 0,
            pending: 0..0,
        }
    }

    /// Fills `out` from the stream until it is full or the stream ends. Pass
    /// the same `inflater` (fresh at the first read) for the whole stream.
    ///
    /// Returns how many bytes were written, which counts even on an error:
    /// they are the output decoded before it, as far as `out` holds it. The error means the input is
    /// truncated, corrupt or fails its Adler-32 check.
    pub fn read(
        &mut self,
        inflater: &mut Inflater,
        input: &mut impl Input,
        out: &mut [u8],
    ) -> (usize, Result<Status, Error>) {
        // SAFETY: `read_uninit` only ever writes initialised bytes.
        let out = unsafe { &mut *(ptr::from_mut(out) as *mut [MaybeUninit<u8>]) };
        self.read_uninit(inflater, input, out)
    }

    /// [`Window::read`] into a buffer that may be uninitialised; the first
    /// `n` bytes of `out` are initialised afterwards.
    pub fn read_uninit(
        &mut self,
        inflater: &mut Inflater,
        input: &mut impl Input,
        out: &mut [MaybeUninit<u8>],
    ) -> (usize, Result<Status, Error>) {
        let mut filled = 0;
        loop {
            filled += self.take_pending(out.get_mut(filled..).unwrap_or_default());
            if self.pending.is_empty() && inflater.is_done() {
                return (filled, Ok(Status::Done));
            }
            if filled == out.len() {
                return (filled, Ok(Status::More));
            }
            // The decoder derives the ring size from the slice, so it always
            // gets the whole window and writes from `pos` up to its end.
            let (written, result) = inflater.step(input, self.buf, self.pos, true);
            self.pending = self.pos..self.pos + written;
            self.pos = self.pending.end % WINDOW_SIZE;
            if let Err(error) = result {
                filled += self.take_pending(out.get_mut(filled..).unwrap_or_default());
                return (filled, Err(error));
            }
        }
    }

    fn take_pending(&mut self, out: &mut [MaybeUninit<u8>]) -> usize {
        let pending = self.buf.get(self.pending.clone()).unwrap_or_default();
        let n = pending.len().min(out.len());
        if let (Some(dst), Some(src)) = (out.get_mut(..n), pending.get(..n)) {
            dst.write_copy_of_slice(src);
        }
        self.pending.start += n;
        n
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests"
)]
mod tests;
