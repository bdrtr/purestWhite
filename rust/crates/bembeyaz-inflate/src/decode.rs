//! The DEFLATE decoder (RFC 1951) and the zlib wrapper (RFC 1950), as a state machine that stops
//! at any byte of input or output and resumes there. Each state takes its bits only once all of
//! them are at hand, so the bit buffer never holds a half-read field.
//!
//! Beyond the plain format errors (block type 3, a stored length that does not match its
//! complement, a bad zlib header, bits that start no code), what counts as corrupt follows
//! `miniz_oxide` 0.9.1, which this replaces: over-subscribed codes, an incomplete code-length
//! code, incomplete literal/length or distance codes longer than one bit, more than 286
//! literal/length or 30 distance codes, a repeat with nothing before it, lengths past the declared
//! count, symbols 286/287 and distance codes 30/31, and a distance before the start of the output
//! buffer. So does when input goes back: whole bytes left in the bit buffer are returned to the
//! input whenever a run stops for anything but more input.
//!
//! Stricter than `miniz_oxide`, as zlib: a literal/length code without the end-of-block symbol is
//! corrupt (such a block could never end).
//!
//! In a ring (`wrapping`) a distance is checked against the output the stream has produced, as
//! zlib does ("invalid distance too far back"); `miniz_oxide` checked it against the ring's size
//! only and copied whatever the ring held before the stream.

use crate::huffman::{Huffman, Lookup};
use crate::{Error, Format};

const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The farthest back a distance reaches.
const MAX_DISTANCE: usize = 32768;
/// The order in which a dynamic block lists the code lengths of the code-length code.
const CODE_LENGTH_ORDER: [u8; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Code lengths of the fixed literal/length code (RFC 1951 §3.2.6).
#[allow(
    clippy::indexing_slicing,
    reason = "evaluated at compile time: an index out of range fails the build"
)]
static FIXED_LITERAL_LENGTHS: [u8; 288] = {
    let mut lengths = [8u8; 288];
    let mut i = 144;
    while i < 288 {
        lengths[i] = if i < 256 {
            9
        } else if i < 280 {
            7
        } else {
            8
        };
        i += 1;
    }
    lengths
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Mode {
    /// Before the zlib header (if any) and the first block. Must stay 0: zeroed is fresh, and
    /// zeroing is the only way it is made.
    #[allow(dead_code, reason = "made by Inflater::init zeroing the state")]
    Start = 0,
    BlockHeader,
    StoredHeader,
    Stored,
    TableSizes,
    CodeLengthLengths,
    CodeLengths,
    Literal,
    /// A literal read while the output was full, in `literal`: written first next time.
    PendingLiteral,
    DistanceCode,
    DistanceExtra,
    Copy,
    Trailer,
    Done,
    Failed,
    /// Failed on the Adler-32 check: later calls report that again.
    BadChecksum,
}

/// Why [`Decoder::run`] stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Done,
    NeedInput,
    OutputFull,
    Failed(Error),
}

/// The decoder's whole state. All-zero bytes are a fresh decoder.
pub(crate) struct Decoder {
    mode: Mode,
    last_block: bool,
    /// The tables hold the fixed codes (a dynamic block clears it).
    fixed_tables: bool,
    bits: u32,
    nbits: u32,
    /// Literal/length and distance code counts of a dynamic block, code-length count.
    hlit: u16,
    hdist: u16,
    hclen: u16,
    /// The code length being read.
    index: u16,
    /// Bytes left in a stored block, or in a match.
    remaining: u16,
    /// The distance of the match (its base while the extra bits are pending).
    distance: u16,
    distance_extra: u8,
    literal: u8,
    adler_a: u32,
    adler_b: u32,
    /// The Adler-32 sum is neither kept nor checked ([`Format::ZlibUnchecked`]).
    unchecked: bool,
    /// The output before this run, up to the farthest a distance reaches.
    history: u16,
    /// Where this run started writing.
    run_start: usize,
    /// Where this run stops writing: the end of `out`, or before it in a ring decoded in steps.
    limit: usize,
    lengths: [u8; 288 + 32],
    code_lengths: Huffman<19, 128>,
    literals: Huffman<288, 1024>,
    distances: Huffman<32, 512>,
}

/// The input of one run and the bit buffer, least significant bit first.
struct BitReader<'a> {
    data: &'a [u8],
    used: usize,
    bits: u32,
    nbits: u32,
}

impl BitReader<'_> {
    /// Tops the buffer up to more than 24 bits while input lasts.
    #[inline]
    fn refill(&mut self) {
        if self.nbits > 24 {
            return;
        }
        // Four bytes at once while they are there: the whole bytes that fit are taken; the low
        // bits of the next one may land above them, which is what they will be when it is read.
        if let Some(&word) = self
            .data
            .get(self.used..)
            .and_then(|rest| rest.first_chunk::<4>())
        {
            self.bits |= u32::from_le_bytes(word) << self.nbits;
            let take = (32 - self.nbits) / 8;
            self.used += take as usize;
            self.nbits += take * 8;
            return;
        }
        while self.nbits <= 24 {
            let Some(&byte) = self.data.get(self.used) else {
                return;
            };
            self.bits |= u32::from(byte) << self.nbits;
            self.nbits += 8;
            self.used += 1;
        }
    }

    /// The next `n` bits (at most 32), if the buffer holds them after a refill.
    #[inline]
    fn peek(&mut self, n: u32) -> Option<u32> {
        if self.nbits < n {
            self.refill();
            if self.nbits < n {
                return None;
            }
        }
        Some(self.bits & mask(n))
    }

    /// Drops `n` bits, fewer than 32.
    #[inline]
    fn consume(&mut self, n: u32) {
        self.bits >>= n & 31;
        self.nbits -= n;
    }

    /// Drops all 32 bits of a full buffer.
    fn consume_all(&mut self) {
        self.bits = 0;
        self.nbits = 0;
    }

    fn align(&mut self) {
        self.consume(self.nbits % 8);
    }

    /// Returns the whole bytes still in the buffer to the input, as far as this run took them.
    fn give_back(&mut self) {
        let bytes = (self.nbits / 8).min(u32::try_from(self.used).unwrap_or(u32::MAX));
        self.used -= bytes as usize;
        self.nbits -= bytes * 8;
        self.bits &= mask(self.nbits);
    }
}

#[inline]
fn mask(n: u32) -> u32 {
    1u32.checked_shl(n).map_or(u32::MAX, |bit| bit - 1)
}

/// Adds `data` to an Adler-32 sum (RFC 1950 §8.2).
fn adler(a: &mut u32, b: &mut u32, data: &[u8]) {
    // 5552 bytes is the most that cannot overflow `b` before the modulo.
    for chunk in data.chunks(5552) {
        for &byte in chunk {
            *a += u32::from(byte);
            *b += *a;
        }
        *a %= 65521;
        *b %= 65521;
    }
}

/// The base and extra-bit count of a length (`symbol` 257-285) or distance (0-29) code.
fn length_code(symbol: u16) -> Option<(u16, u32)> {
    let i = usize::from(symbol.checked_sub(257)?);
    Some((*LENGTH_BASE.get(i)?, u32::from(*LENGTH_EXTRA.get(i)?)))
}

fn distance_code(symbol: u16) -> Option<(u16, u32)> {
    let i = usize::from(symbol);
    Some((*DIST_BASE.get(i)?, u32::from(*DIST_EXTRA.get(i)?)))
}

/// Whether a match `distance` back may be copied: within the output so far (`at` in a plain
/// buffer, `history` in a ring) and within the buffer.
fn distance_ok(distance: usize, at: usize, history: usize, out_len: usize, wrapping: bool) -> bool {
    distance <= if wrapping { history } else { at } && distance <= out_len
}

/// Writes the `n` bytes `distance` back from `at` to `out[at..at + n]`, which the caller has
/// checked fits. In a ring the source may wrap around its end.
fn copy_match(out: &mut [u8], at: usize, distance: usize, n: usize) {
    let len = out.len();
    if n == 0 || distance == 0 || distance > len || at + n > len {
        return;
    }
    let Some(src) = at.checked_sub(distance) else {
        // Only in a ring: the source starts before its end. Copy up to the end, then the rest
        // continues from the start of the ring like any match.
        let src = at + len - distance;
        let first = (len - src).min(n);
        if src >= at + first {
            copy_disjoint(out, src, at, first);
        } else {
            // The source meets the copy (a distance within 258 of the ring size).
            for i in 0..first {
                let byte = out.get(src + i).copied().unwrap_or(0);
                if let Some(slot) = out.get_mut(at + i) {
                    *slot = byte;
                }
            }
        }
        copy_match(out, at + first, distance, n - first);
        return;
    };
    if distance >= n {
        copy_disjoint(out, src, at, n);
    } else if distance == 1 {
        let byte = out.get(src).copied().unwrap_or(0);
        out.get_mut(at..at + n).unwrap_or_default().fill(byte);
    } else if n <= 16 {
        // Short overlapping matches byte by byte, without a call per doubling.
        let Some(span) = out.get_mut(src..at + n) else {
            return;
        };
        for i in 0..n {
            if let Some(&byte) = span.get(i)
                && let Some(slot) = span.get_mut(i + distance)
            {
                *slot = byte;
            }
        }
    } else {
        // Overlapping: the written part repeats every `distance` bytes, and doubles each copy.
        let mut copied = 0;
        while copied < n {
            let chunk = (distance + copied).min(n - copied);
            copy_disjoint(out, src, at + copied, chunk);
            copied += chunk;
        }
    }
}

/// Copies `out[from..from + n]` to `out[to..to + n]`, which do not overlap. Uses memcpy
/// (`copy_from_slice`): the ESP32-S3 ROM memcpy copies words even when unaligned, while its memmove
/// (`copy_within`) copies one byte at a time, at 7 instructions a byte.
fn copy_disjoint(out: &mut [u8], from: usize, to: usize, n: usize) {
    if from < to {
        let (head, tail) = out.split_at_mut(to);
        if let (Some(src), Some(dst)) = (head.get(from..from + n), tail.get_mut(..n)) {
            dst.copy_from_slice(src);
        }
    } else {
        let (head, tail) = out.split_at_mut(from);
        if let (Some(dst), Some(src)) = (head.get_mut(to..to + n), tail.get(..n)) {
            dst.copy_from_slice(src);
        }
    }
}

impl Decoder {
    /// Decodes `input` into `out[pos..]` until the output is full, the input runs out or the
    /// stream ends. `out[..pos]` holds the earlier output: before `pos` in a plain buffer, or
    /// around the ring when `wrapping` (`out.len()` a power of two). `pos <= out.len()`. Returns
    /// the input bytes taken, the output bytes written and why it stopped.
    pub(crate) fn run(
        &mut self,
        input: &[u8],
        out: &mut [u8],
        pos: usize,
        wrapping: bool,
        format: Format,
        limit: usize,
    ) -> (usize, usize, Outcome) {
        let zlib = format != Format::Deflate;
        self.limit = limit.min(out.len());
        self.unchecked = format == Format::ZlibUnchecked;
        let mut bits = BitReader {
            data: input,
            used: 0,
            bits: self.bits,
            nbits: self.nbits,
        };
        self.run_start = pos;
        let mut at = pos;
        // The output from `hashed` on is not in the Adler-32 sum yet.
        let mut hashed = pos;
        let outcome = self.decode(&mut bits, out, &mut at, &mut hashed, wrapping, zlib);
        // As `miniz_oxide`: every stop but for input returns the unread whole bytes.
        if outcome != Outcome::NeedInput {
            bits.give_back();
        }
        self.bits = bits.bits;
        self.nbits = bits.nbits;
        if zlib && !self.unchecked {
            self.hash(out, hashed, at);
        }
        self.history = u16::try_from(self.history(at).min(MAX_DISTANCE)).unwrap_or(u16::MAX);
        match outcome {
            Outcome::Failed(Error::Checksum) => self.mode = Mode::BadChecksum,
            Outcome::Failed(_) => self.mode = Mode::Failed,
            _ => {}
        }
        (bits.used, at - pos, outcome)
    }

    /// The output of the stream up to `at` in this run, counted up to past what a distance reaches.
    fn history(&self, at: usize) -> usize {
        usize::from(self.history) + at.saturating_sub(self.run_start)
    }

    fn hash(&mut self, out: &[u8], from: usize, to: usize) {
        adler(
            &mut self.adler_a,
            &mut self.adler_b,
            out.get(from..to).unwrap_or_default(),
        );
    }

    fn end_block(&mut self, zlib: bool) {
        self.mode = match (self.last_block, zlib) {
            (false, _) => Mode::BlockHeader,
            (true, true) => Mode::Trailer,
            (true, false) => Mode::Done,
        };
    }

    #[allow(clippy::too_many_lines, reason = "one arm per state of the format")]
    fn decode(
        &mut self,
        bits: &mut BitReader,
        out: &mut [u8],
        at: &mut usize,
        hashed: &mut usize,
        wrapping: bool,
        zlib: bool,
    ) -> Outcome {
        loop {
            match self.mode {
                Mode::Start => {
                    if zlib {
                        let Some(header) = bits.peek(16) else {
                            return Outcome::NeedInput;
                        };
                        let (cmf, flg) = (header & 0xFF, header >> 8);
                        // CM 8 (DEFLATE), a window of at most 32 KiB, no preset dictionary.
                        if (cmf * 256 + flg) % 31 != 0
                            || flg & 0x20 != 0
                            || cmf & 15 != 8
                            || cmf >> 4 > 7
                        {
                            return Outcome::Failed(Error::Corrupt);
                        }
                        bits.consume(16);
                        self.adler_a = 1;
                        self.adler_b = 0;
                    }
                    self.mode = Mode::BlockHeader;
                }
                Mode::BlockHeader => {
                    let Some(header) = bits.peek(3) else {
                        return Outcome::NeedInput;
                    };
                    bits.consume(3);
                    self.last_block = header & 1 == 1;
                    match header >> 1 {
                        0 => self.mode = Mode::StoredHeader,
                        1 => {
                            if !self.fixed_tables {
                                if self.literals.build(&FIXED_LITERAL_LENGTHS, false).is_err()
                                    || self.distances.build(&[5; 32], false).is_err()
                                {
                                    return Outcome::Failed(Error::Corrupt);
                                }
                                self.fixed_tables = true;
                            }
                            self.mode = Mode::Literal;
                        }
                        2 => self.mode = Mode::TableSizes,
                        _ => return Outcome::Failed(Error::Corrupt),
                    }
                }
                Mode::StoredHeader => {
                    bits.align();
                    let Some(header) = bits.peek(32) else {
                        return Outcome::NeedInput;
                    };
                    #[allow(clippy::cast_possible_truncation, reason = "the two halves")]
                    let (len, nlen) = (header as u16, (header >> 16) as u16);
                    if len != !nlen {
                        return Outcome::Failed(Error::Corrupt);
                    }
                    bits.consume_all();
                    self.remaining = len;
                    if len == 0 {
                        self.end_block(zlib);
                    } else {
                        self.mode = Mode::Stored;
                    }
                }
                Mode::Stored => {
                    while self.remaining > 0 {
                        let space = self.limit.saturating_sub(*at);
                        if space == 0 {
                            return Outcome::OutputFull;
                        }
                        // Whole bytes already in the bit buffer come first.
                        if bits.nbits >= 8 {
                            #[allow(clippy::cast_possible_truncation, reason = "one byte")]
                            let byte = bits.bits as u8;
                            bits.consume(8);
                            if let Some(slot) = out.get_mut(*at) {
                                *slot = byte;
                            }
                            *at += 1;
                            self.remaining -= 1;
                            continue;
                        }
                        let available = bits.data.get(bits.used..).unwrap_or_default();
                        if available.is_empty() {
                            return Outcome::NeedInput;
                        }
                        let n = usize::from(self.remaining).min(space).min(available.len());
                        if let (Some(dst), Some(src)) =
                            (out.get_mut(*at..*at + n), available.get(..n))
                        {
                            dst.copy_from_slice(src);
                        }
                        bits.used += n;
                        *at += n;
                        #[allow(clippy::cast_possible_truncation, reason = "n <= remaining")]
                        let n = n as u16;
                        self.remaining -= n;
                    }
                    self.end_block(zlib);
                }
                Mode::TableSizes => {
                    let Some(sizes) = bits.peek(14) else {
                        return Outcome::NeedInput;
                    };
                    bits.consume(14);
                    #[allow(clippy::cast_possible_truncation, reason = "masked fields")]
                    {
                        self.hlit = (sizes & 31) as u16 + 257;
                        self.hdist = ((sizes >> 5) & 31) as u16 + 1;
                        self.hclen = ((sizes >> 10) & 15) as u16 + 4;
                    }
                    if self.hlit > 286 || self.hdist > 30 {
                        return Outcome::Failed(Error::Corrupt);
                    }
                    self.lengths.get_mut(..19).unwrap_or_default().fill(0);
                    self.index = 0;
                    self.mode = Mode::CodeLengthLengths;
                }
                Mode::CodeLengthLengths => {
                    while self.index < self.hclen {
                        let Some(len) = bits.peek(3) else {
                            return Outcome::NeedInput;
                        };
                        bits.consume(3);
                        let symbol = CODE_LENGTH_ORDER
                            .get(usize::from(self.index))
                            .copied()
                            .unwrap_or(0);
                        #[allow(clippy::cast_possible_truncation, reason = "3 bits")]
                        if let Some(slot) = self.lengths.get_mut(usize::from(symbol)) {
                            *slot = len as u8;
                        }
                        self.index += 1;
                    }
                    if self
                        .code_lengths
                        .build(self.lengths.get(..19).unwrap_or_default(), false)
                        .is_err()
                    {
                        return Outcome::Failed(Error::Corrupt);
                    }
                    self.index = 0;
                    self.mode = Mode::CodeLengths;
                }
                Mode::CodeLengths => {
                    let total = self.hlit + self.hdist;
                    while self.index < total {
                        bits.refill();
                        let (symbol, len) = match self.code_lengths.lookup(bits.bits, bits.nbits) {
                            Lookup::Found(symbol, len) => (symbol, len),
                            Lookup::NeedBits => return Outcome::NeedInput,
                            Lookup::Invalid => return Outcome::Failed(Error::Corrupt),
                        };
                        let index = usize::from(self.index);
                        if symbol < 16 {
                            bits.consume(len);
                            #[allow(clippy::cast_possible_truncation, reason = "below 16")]
                            if let Some(slot) = self.lengths.get_mut(index) {
                                *slot = symbol as u8;
                            }
                            self.index += 1;
                            continue;
                        }
                        // 16: repeat the previous length 3-6 times; 17, 18: zeros, 3-10 and 11-138.
                        let (extra, base, value) = match symbol {
                            16 => {
                                let Some(&previous) =
                                    index.checked_sub(1).and_then(|i| self.lengths.get(i))
                                else {
                                    return Outcome::Failed(Error::Corrupt);
                                };
                                (2, 3, previous)
                            }
                            17 => (3, 3, 0),
                            _ => (7, 11, 0),
                        };
                        if bits.nbits < len + extra {
                            return Outcome::NeedInput;
                        }
                        bits.consume(len);
                        let count = base + (bits.bits & mask(extra)) as usize;
                        bits.consume(extra);
                        if index + count > usize::from(total) {
                            return Outcome::Failed(Error::Corrupt);
                        }
                        self.lengths
                            .get_mut(index..index + count)
                            .unwrap_or_default()
                            .fill(value);
                        #[allow(clippy::cast_possible_truncation, reason = "at most 138")]
                        let count = count as u16;
                        self.index += count;
                    }
                    let (hlit, hdist) = (usize::from(self.hlit), usize::from(self.hdist));
                    let (literals, distances) =
                        self.lengths.split_at_checked(hlit).unwrap_or_default();
                    self.fixed_tables = false;
                    // A block without an end-of-block code can never end; zlib refuses it here.
                    if literals.get(256).is_none_or(|&len| len == 0)
                        || self.literals.build(literals, true).is_err()
                        || self
                            .distances
                            .build(distances.get(..hdist).unwrap_or_default(), true)
                            .is_err()
                    {
                        return Outcome::Failed(Error::Corrupt);
                    }
                    self.mode = Mode::Literal;
                }
                Mode::Literal => loop {
                    bits.refill();
                    let (symbol, len) = match self.literals.lookup(bits.bits, bits.nbits) {
                        Lookup::Found(symbol, len) => (symbol, len),
                        Lookup::NeedBits => return Outcome::NeedInput,
                        Lookup::Invalid => return Outcome::Failed(Error::Corrupt),
                    };
                    if symbol < 256 {
                        bits.consume(len);
                        #[allow(clippy::cast_possible_truncation, reason = "below 256")]
                        let byte = symbol as u8;
                        let Some(slot) = out.get_mut(*at).filter(|_| *at < self.limit) else {
                            // Read now, written next time, as `miniz_oxide` does: it decides how
                            // much input is left over.
                            self.literal = byte;
                            self.mode = Mode::PendingLiteral;
                            return Outcome::OutputFull;
                        };
                        *slot = byte;
                        *at += 1;
                        continue;
                    }
                    if symbol == 256 {
                        bits.consume(len);
                        self.end_block(zlib);
                        break;
                    }
                    let Some((base, extra)) = length_code(symbol) else {
                        return Outcome::Failed(Error::Corrupt);
                    };
                    if bits.nbits < len + extra {
                        return Outcome::NeedInput;
                    }
                    bits.consume(len);
                    #[allow(clippy::cast_possible_truncation, reason = "at most 5 bits")]
                    let length = base + (bits.bits & mask(extra)) as u16;
                    bits.consume(extra);
                    // The distance right away when its bits are here, and the copy when it fits;
                    // otherwise the states below take it from where it stands.
                    bits.refill();
                    match self.distances.lookup(bits.bits, bits.nbits) {
                        Lookup::Found(symbol, len) => {
                            let Some((base, extra)) = distance_code(symbol) else {
                                return Outcome::Failed(Error::Corrupt);
                            };
                            if bits.nbits >= len + extra {
                                bits.consume(len);
                                let distance =
                                    usize::from(base) + (bits.bits & mask(extra)) as usize;
                                bits.consume(extra);
                                if !distance_ok(
                                    distance,
                                    *at,
                                    self.history(*at),
                                    out.len(),
                                    wrapping,
                                ) {
                                    return Outcome::Failed(Error::Corrupt);
                                }
                                let length = usize::from(length);
                                if length <= self.limit.saturating_sub(*at) {
                                    copy_match(out, *at, distance, length);
                                    *at += length;
                                    continue;
                                }
                                #[allow(
                                    clippy::cast_possible_truncation,
                                    reason = "at most 258 and 32768"
                                )]
                                {
                                    self.remaining = length as u16;
                                    self.distance = distance as u16;
                                }
                                self.mode = Mode::Copy;
                                break;
                            }
                        }
                        Lookup::Invalid => return Outcome::Failed(Error::Corrupt),
                        Lookup::NeedBits => {}
                    }
                    self.remaining = length;
                    self.mode = Mode::DistanceCode;
                    break;
                },
                Mode::PendingLiteral => {
                    let Some(slot) = out.get_mut(*at).filter(|_| *at < self.limit) else {
                        return Outcome::OutputFull;
                    };
                    *slot = self.literal;
                    *at += 1;
                    self.mode = Mode::Literal;
                }
                Mode::DistanceCode => {
                    bits.refill();
                    let (symbol, len) = match self.distances.lookup(bits.bits, bits.nbits) {
                        Lookup::Found(symbol, len) => (symbol, len),
                        Lookup::NeedBits => return Outcome::NeedInput,
                        Lookup::Invalid => return Outcome::Failed(Error::Corrupt),
                    };
                    let Some((base, extra)) = distance_code(symbol) else {
                        return Outcome::Failed(Error::Corrupt);
                    };
                    bits.consume(len);
                    self.distance = base;
                    #[allow(clippy::cast_possible_truncation, reason = "at most 13")]
                    {
                        self.distance_extra = extra as u8;
                    }
                    self.mode = Mode::DistanceExtra;
                }
                Mode::DistanceExtra => {
                    let extra = u32::from(self.distance_extra);
                    let Some(more) = bits.peek(extra) else {
                        return Outcome::NeedInput;
                    };
                    bits.consume(extra);
                    #[allow(clippy::cast_possible_truncation, reason = "at most 13 bits")]
                    let more = more as u16;
                    self.distance += more;
                    if !distance_ok(
                        usize::from(self.distance),
                        *at,
                        self.history(*at),
                        out.len(),
                        wrapping,
                    ) {
                        return Outcome::Failed(Error::Corrupt);
                    }
                    self.mode = Mode::Copy;
                }
                Mode::Copy => {
                    let space = self.limit.saturating_sub(*at);
                    let n = usize::from(self.remaining).min(space);
                    copy_match(out, *at, usize::from(self.distance), n);
                    *at += n;
                    #[allow(clippy::cast_possible_truncation, reason = "n <= remaining")]
                    let n = n as u16;
                    self.remaining -= n;
                    if self.remaining > 0 {
                        return Outcome::OutputFull;
                    }
                    self.mode = Mode::Literal;
                }
                Mode::Trailer => {
                    bits.align();
                    let Some(trailer) = bits.peek(32) else {
                        return Outcome::NeedInput;
                    };
                    bits.consume_all();
                    self.mode = Mode::Done;
                    if self.unchecked {
                        continue;
                    }
                    self.hash(out, *hashed, *at);
                    *hashed = *at;
                    let sum = (self.adler_b << 16) | self.adler_a;
                    if trailer.swap_bytes() != sum {
                        return Outcome::Failed(Error::Checksum);
                    }
                }
                Mode::Done => return Outcome::Done,
                Mode::Failed => return Outcome::Failed(Error::Corrupt),
                Mode::BadChecksum => return Outcome::Failed(Error::Checksum),
            }
        }
    }
}
