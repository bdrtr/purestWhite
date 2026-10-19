//! Streaming DEFLATE and zlib decompression over caller-owned storage: our own decoder
//! (RFC 1951, RFC 1950).
//!
//! [`Inflater`] decodes into one buffer that holds the whole output, so it
//! needs no window. With a borrowed 32 KiB [`Window`] it hands out the output
//! in pieces of any size. Nothing allocates: the caller decides where the
//! ~4.5 KiB decoder state and the window live (heap, or the framebuffer lent
//! out during a chapter build).
//!
//! The compressed data comes from untrusted files. Corrupt or truncated input
//! yields an [`Error`]; it never reads or writes outside the given buffers.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

mod decode;
mod huffman;

use core::ffi::CStr;
use core::fmt;
use core::mem::MaybeUninit;
use core::ops::Range;
use core::ptr;
use decode::{Decoder, Outcome};

/// Size of a [`Window`]: the largest distance DEFLATE can refer back.
pub const WINDOW_SIZE: usize = 32 * 1024;

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
    /// DEFLATE inside a zlib header and Adler-32 trailer.
    Zlib,
    /// [`Format::Zlib`] with the trailer read but not checked, as PNGdec does: the sum costs a
    /// pass over all the output.
    ZlibUnchecked,
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

/// How [`Inflater::step`] went, when it did not fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    /// It took input or wrote output, or the stream ended.
    Progress,
    /// The input ran out and the decoder needs more.
    InputEnded,
}

/// Where [`Window::feed_with`] stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    /// `want` bytes were handed out.
    OutputFull,
    /// The input given so far is used up.
    NeedInput,
    /// The stream ended; fewer than `want` bytes may have been handed out.
    Done,
}

/// The decoder. On its own it writes the whole output into one buffer:
/// back-references resolve inside that buffer, so no window is needed, but
/// every call must pass the same buffer with the earlier output in front.
/// [`Window`] hands the output out in pieces instead.
pub struct Inflater {
    decoder: Decoder,
    format: Format,
    done: bool,
}

impl Inflater {
    /// Tests only: the ~4.5 KiB result may be built on the stack and moved.
    #[cfg(test)]
    #[must_use]
    pub fn new(format: Format) -> Self {
        let mut slot = MaybeUninit::uninit();
        Self::init(&mut slot, format);
        // SAFETY: init() initialised it.
        unsafe { slot.assume_init() }
    }

    /// Sets up an inflater in `slot` without building it on the stack first,
    /// which Rust does not promise for a returned value even when inlined.
    pub fn init(slot: &mut MaybeUninit<Self>, format: Format) -> &mut Self {
        let this = slot.as_mut_ptr();
        // SAFETY: all-zero bytes are a fresh `Decoder` (integers, integer arrays, `false` and
        // enums whose first variant is 0); Miri checks this in `init_over_garbage_decodes`.
        unsafe {
            (&raw mut (*this).decoder).write_bytes(0, 1);
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
            let (written, result) = self.step(input, out, at, out.len(), false);
            at += written;
            match result {
                Ok(Step::Progress) => {}
                // All the input is here: running out of it is a truncation.
                Ok(Step::InputEnded) => return (at - pos, Err(Error::Truncated)),
                Err(error) => return (at - pos, Err(error)),
            }
        }
        (at - pos, Ok(Status::Done))
    }

    /// Runs the decoder once on the buffered input and returns the bytes
    /// written at `out[pos..out.len().min(limit)]`, even on an error. A
    /// `wrapping` `out` is a power-of-two ring.
    fn step(
        &mut self,
        input: &mut impl Input,
        out: &mut [u8],
        pos: usize,
        limit: usize,
        wrapping: bool,
    ) -> (usize, Result<Step, Error>) {
        if pos > out.len() {
            return (0, Err(Error::BadOutput));
        }
        let data = input.fill_buf();
        let at_end = data.is_empty();
        let (consumed, written, outcome) =
            self.decoder
                .run(data, out, pos, wrapping, self.format, limit);
        input.consume(consumed);
        let result = match outcome {
            Outcome::Done => {
                self.done = true;
                Ok(Step::Progress)
            }
            // The input has ended (an empty refill is the end) and more was needed, even when
            // this step still wrote what the bit buffer held.
            Outcome::NeedInput if at_end => Ok(Step::InputEnded),
            // Both report progress; a step that made none would loop forever.
            Outcome::NeedInput | Outcome::OutputFull if consumed > 0 || written > 0 => {
                Ok(Step::Progress)
            }
            // Input left over that the decoder could not take (no field needs that many bits).
            Outcome::NeedInput => Err(Error::Truncated),
            Outcome::OutputFull => Err(Error::Corrupt),
            Outcome::Failed(error) => Err(error),
        };
        (written, result)
    }
}

/// Hands an [`Inflater`]'s output out in pieces of any size, through a
/// 32 KiB window that holds the history back-references need.
///
/// For [`Window::read`] the decoder fills the window as far as it can in one
/// go (several times faster than decoding piecemeal); what the caller has not
/// taken yet stays pending in the window until the next read.
/// [`Window::feed_with`] decodes only as far as asked, so the bytes are still in
/// the CPU data cache when taken.
pub struct Window<'w> {
    buf: &'w mut [u8; WINDOW_SIZE],
    /// Where the decoder writes next.
    pos: usize,
    /// Decoded bytes not yet handed out.
    pending: Range<usize>,
    /// The error that stopped the decoder, reported once `pending` is handed out.
    failed: Option<Error>,
}

impl<'w> Window<'w> {
    /// The buffer's previous contents are irrelevant to a valid stream (see the ring note in
    /// `decode.rs` for one that refers back past its start).
    #[must_use]
    pub fn new(buf: &'w mut [u8; WINDOW_SIZE]) -> Self {
        Self {
            buf,
            pos: 0,
            pending: 0..0,
            failed: None,
        }
    }

    /// Fills `out` from the stream until it is full or the stream ends. Pass
    /// the same `inflater` (fresh at the first read) for the whole stream.
    ///
    /// Returns how many bytes were written, which counts even on an error:
    /// they are the output decoded before it, as far as `out` holds it. The error means the input is
    /// truncated, corrupt or fails its Adler-32 check. It comes only once every byte decoded before
    /// it has been handed out: a read that ends before the damage succeeds.
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
        let (n, result) = self.fill(inflater, input, out.len(), false, |piece| {
            if let Some(dst) = out.get_mut(filled..filled + piece.len()) {
                dst.write_copy_of_slice(piece);
            }
            filled += piece.len();
        });
        let result = match result {
            Ok(Feed::OutputFull) => Ok(Status::More),
            Ok(Feed::Done) => Ok(Status::Done),
            // Never: not resumable, `fill` reports running out of input as Truncated itself.
            Ok(Feed::NeedInput) => Err(Error::Truncated),
            Err(error) => Err(error),
        };
        (n, result)
    }

    /// [`Window::feed_with`] into `out`, for the tests.
    #[cfg(test)]
    pub fn feed(
        &mut self,
        inflater: &mut Inflater,
        input: &mut &[u8],
        out: &mut [u8],
    ) -> (usize, Result<Feed, Error>) {
        let mut filled = 0;
        self.feed_with(inflater, input, out.len(), |piece| {
            if let Some(dst) = out.get_mut(filled..filled + piece.len()) {
                dst.copy_from_slice(piece);
            }
            filled += piece.len();
        })
    }

    /// Decodes input that arrives in pieces (PNG's IDAT chunks) and hands the next `want` bytes
    /// (at most) to `take` where they lie in the window, in one or more pieces, instead of
    /// copying them out. Running out of `input` is [`Feed::NeedInput`], not an error, and the
    /// next call goes on with more; the bytes taken are removed from the front of `input`. The
    /// decoder runs only as far ahead as asked, so the bytes are still in the CPU data cache when
    /// `take` reads them.
    pub fn feed_with(
        &mut self,
        inflater: &mut Inflater,
        input: &mut &[u8],
        want: usize,
        take: impl FnMut(&[u8]),
    ) -> (usize, Result<Feed, Error>) {
        self.fill(inflater, input, want, true, take)
    }

    fn fill(
        &mut self,
        inflater: &mut Inflater,
        input: &mut impl Input,
        want: usize,
        resumable: bool,
        mut take: impl FnMut(&[u8]),
    ) -> (usize, Result<Feed, Error>) {
        let mut given = 0;
        loop {
            let n = self.pending.len().min(want - given);
            if n > 0 {
                let start = self.pending.start;
                take(self.buf.get(start..start + n).unwrap_or_default());
                self.pending.start += n;
                given += n;
            }
            if self.pending.is_empty() {
                if inflater.is_done() {
                    return (given, Ok(Feed::Done));
                }
                if let Some(error) = self.failed {
                    return (given, Err(error));
                }
            }
            if given == want {
                return (given, Ok(Feed::OutputFull));
            }
            // The decoder derives the ring size from the slice, so it always gets the whole
            // window. It writes from `pos` up to its end, or in steps only as far as asked.
            let limit = if resumable {
                (self.pos + (want - given)).min(WINDOW_SIZE)
            } else {
                WINDOW_SIZE
            };
            let (written, result) = inflater.step(input, self.buf, self.pos, limit, true);
            self.pending = self.pos..self.pos + written;
            self.pos = self.pending.end % WINDOW_SIZE;
            match result {
                Ok(Step::Progress) => {}
                // Out of input for now. What the bit buffer still held is pending; once that is
                // handed out, a step that writes nothing ends the call.
                Ok(Step::InputEnded) if resumable => {
                    if written == 0 {
                        return (given, Ok(Feed::NeedInput));
                    }
                }
                // All the input is here: running out of it is a truncation.
                Ok(Step::InputEnded) => self.failed = Some(Error::Truncated),
                Err(error) => self.failed = Some(error),
            }
        }
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

#[cfg(test)]
mod reference;

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "tests"
)]
mod differential;
