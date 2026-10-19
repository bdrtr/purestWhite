//! The decoder this crate replaced (miniz_oxide 0.9.1 behind the same API, as of 9e985f3), kept
//! for the tests to compare against. `differential.rs` allows the one deliberate change from its
//! behaviour (the ring note in `decode.rs`).
#![allow(dead_code, clippy::pedantic, reason = "test reference, kept as it was")]

use crate::{Error, Format, Input, Status, WINDOW_SIZE};
use core::mem::MaybeUninit;
use core::ops::Range;
use core::ptr;
use miniz_oxide::inflate::TINFLStatus;
use miniz_oxide::inflate::core::inflate_flags::{
    TINFL_FLAG_HAS_MORE_INPUT, TINFL_FLAG_PARSE_ZLIB_HEADER,
    TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
};
use miniz_oxide::inflate::core::{DecompressorOxide, decompress};

pub struct RefInflater {
    state: DecompressorOxide,
    format: Format,
    done: bool,
}

impl RefInflater {
    /// Tests only: the ~10 KiB result may be built on the stack and moved.
    #[must_use]
    pub fn new(format: Format) -> Self {
        Self {
            state: DecompressorOxide::new(),
            format,
            done: false,
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

/// Hands an [`RefInflater`]'s output out in pieces of any size, through a
/// 32 KiB window that holds the history back-references need.
///
/// The decoder fills the window as far as it can in one go (several times
/// faster than decoding piecemeal); what the caller has not taken yet stays
/// pending in the window until the next read.
pub struct RefWindow<'w> {
    buf: &'w mut [u8; WINDOW_SIZE],
    /// Where the decoder writes next.
    pos: usize,
    /// Decoded bytes not yet handed out.
    pending: Range<usize>,
    /// The error that stopped the decoder, reported once `pending` is handed out.
    failed: Option<Error>,
}

impl<'w> RefWindow<'w> {
    /// The buffer's previous contents are irrelevant.
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
        inflater: &mut RefInflater,
        input: &mut impl Input,
        out: &mut [u8],
    ) -> (usize, Result<Status, Error>) {
        // SAFETY: `read_uninit` only ever writes initialised bytes.
        let out = unsafe { &mut *(ptr::from_mut(out) as *mut [MaybeUninit<u8>]) };
        self.read_uninit(inflater, input, out)
    }

    /// [`RefWindow::read`] into a buffer that may be uninitialised; the first
    /// `n` bytes of `out` are initialised afterwards.
    pub fn read_uninit(
        &mut self,
        inflater: &mut RefInflater,
        input: &mut impl Input,
        out: &mut [MaybeUninit<u8>],
    ) -> (usize, Result<Status, Error>) {
        let mut filled = 0;
        loop {
            filled += self.take_pending(out.get_mut(filled..).unwrap_or_default());
            if self.pending.is_empty() {
                if inflater.is_done() {
                    return (filled, Ok(Status::Done));
                }
                if let Some(error) = self.failed {
                    return (filled, Err(error));
                }
            }
            if filled == out.len() {
                return (filled, Ok(Status::More));
            }
            // The decoder derives the ring size from the slice, so it always
            // gets the whole window and writes from `pos` up to its end.
            let (written, result) = inflater.step(input, self.buf, self.pos, true);
            self.pending = self.pos..self.pos + written;
            self.pos = self.pending.end % WINDOW_SIZE;
            self.failed = result.err();
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
