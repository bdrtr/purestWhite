//! `StarDict` dictionaries (src/util/Dictionary): the `.ifo` facts the reader needs, the sampled-offset
//! sidecars (`.qidx` over `.idx`, `.sidx` over `.syn`: the byte offset of every 256th entry) and
//! their scan, the lookups that bisect a sidecar and scan at most 256 entries from there, and the
//! cleaning and English stem variants of a looked-up word; and [`dictzip`] files' chunk tables.
//! Files are read through [`Source`]; opening them, memory and the definitions themselves are the
//! caller's.
//!
//! Nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use core::cmp::Ordering;

pub mod dictzip;

/// Entries between two samples of a sidecar.
pub const SAMPLE_INTERVAL: u32 = 256;
/// "QIDX" and "SIDX", little-endian: the `.qidx` and `.sidx` sidecars.
pub const QIDX_MAGIC: u32 = 0x5844_4951;
pub const SIDX_MAGIC: u32 = 0x5844_4953;
/// Bumped from 1 when the entry count was added.
pub const SIDECAR_VERSION: u32 = 2;
/// A sidecar's header: magic, version, sample interval, sample count, the source's size and its
/// entry count, each a little-endian `u32`; the samples follow.
pub const HEADER_LEN: usize = 24;
/// The longest word kept; longer ones are cut there (their rest is skipped).
pub const WORD_MAX: usize = 255;
/// The fixed bytes after an entry's word: in `.idx` the definition's offset and size, in `.syn`
/// the ordinal of its `.idx` entry.
pub const IDX_SUFFIX: u32 = 8;
pub const SYN_SUFFIX: u32 = 4;

/// What the reader needs from a dictionary's `.ifo`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ifo {
    /// `idxoffsetbits=64`, which the reader does not support.
    pub offsets64: bool,
    /// `sametypesequence=h` or `x` (HTML or XDXF), which the viewer lays out as HTML.
    pub html: bool,
}

/// Where `needle` first starts in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// The text after the first `=` from where `key` first appears.
fn value<'a>(text: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let rest = text.get(find(text, key)?..)?;
    rest.get(rest.iter().position(|&b| b == b'=')? + 1..)
}

/// `strtol(text, nullptr, 10)` as far as telling 64 apart: leading white space, a sign, digits.
fn leading_number(text: &[u8]) -> i64 {
    let mut digits = text
        .iter()
        .skip_while(|b| matches!(b, b' ' | b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r'))
        .peekable();
    let negative = match digits.peek() {
        Some(b'-') => {
            digits.next();
            true
        }
        Some(b'+') => {
            digits.next();
            false
        }
        _ => false,
    };
    let n = digits
        .take_while(|d| d.is_ascii_digit())
        .fold(0i64, |n, &d| {
            n.saturating_mul(10).saturating_add(i64::from(d - b'0'))
        });
    if negative { -n } else { n }
}

/// The facts from the start of an `.ifo` (the reader reads its first 2 KB), read as a C string:
/// up to its first NUL.
#[must_use]
pub fn ifo(text: &[u8]) -> Ifo {
    let text = text.split(|&b| b == 0).next().unwrap_or_default();
    let html = value(text, b"sametypesequence").is_some_and(|v| {
        matches!(v.first(), Some(b'h' | b'x')) && matches!(v.get(1), None | Some(b'\r' | b'\n'))
    });
    Ifo {
        offsets64: value(text, b"idxoffsetbits").is_some_and(|v| leading_number(v) == 64),
        html,
    }
}

/// A sidecar header's counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Header {
    pub sample_count: u32,
    pub source_size: u32,
    pub entry_count: u32,
}

/// The counts of a sidecar header, when it has `magic`, this version and the sample interval.
#[must_use]
pub fn header(raw: &[u8; HEADER_LEN], magic: u32) -> Option<Header> {
    let (words, _) = raw.as_chunks::<4>();
    let [m, version, interval, samples, size, entries] = words else {
        return None;
    };
    let [m, version, interval, samples, size, entries] =
        [m, version, interval, samples, size, entries].map(|w| u32::from_le_bytes(*w));
    (m == magic && version == SIDECAR_VERSION && interval == SAMPLE_INTERVAL).then_some(Header {
        sample_count: samples,
        source_size: size,
        entry_count: entries,
    })
}

/// A sidecar header with `magic` and `counts`.
#[must_use]
pub fn header_bytes(magic: u32, counts: &Header) -> [u8; HEADER_LEN] {
    let mut out = [0; HEADER_LEN];
    let words = [
        magic,
        SIDECAR_VERSION,
        SAMPLE_INTERVAL,
        counts.sample_count,
        counts.source_size,
        counts.entry_count,
    ];
    for (chunk, word) in out.as_chunks_mut::<4>().0.iter_mut().zip(words) {
        *chunk = word.to_le_bytes();
    }
    out
}

/// A sidecar's scan of its source: entries are a NUL-terminated word and `suffix` fixed bytes (8
/// in `.idx`: offset and size; 4 in `.syn`: an ordinal), and the byte offset of every 256th one
/// (but the first, which the sidecar always starts with, and any at the source's end) is a sample.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Scan {
    suffix: u32,
    /// Suffix bytes still to come, or 0 in a word.
    left: u32,
    source_size: u32,
    /// Source bytes read.
    pos: u32,
    pub entries: u32,
    /// Samples taken, the first included.
    pub samples: u32,
}

impl Scan {
    #[must_use]
    pub fn new(suffix: u32, source_size: u32) -> Self {
        Scan {
            suffix,
            source_size,
            samples: 1,
            ..Scan::default()
        }
    }

    /// Reads the next bytes of the source, handing each sample to `sample`; false as soon as
    /// `sample` is.
    pub fn feed(&mut self, bytes: &[u8], mut sample: impl FnMut(u32) -> bool) -> bool {
        for (i, &b) in (0u32..).zip(bytes) {
            if self.left == 0 {
                if b == 0 {
                    self.left = self.suffix;
                }
                continue;
            }
            self.left -= 1;
            if self.left != 0 {
                continue;
            }
            self.entries = self.entries.wrapping_add(1);
            let next = self.pos.wrapping_add(i).wrapping_add(1);
            if self.entries.is_multiple_of(SAMPLE_INTERVAL) && next < self.source_size {
                if !sample(next) {
                    return false;
                }
                self.samples = self.samples.wrapping_add(1);
            }
        }
        #[allow(
            clippy::cast_possible_truncation,
            reason = "the source's offsets are 32-bit"
        )]
        let read = bytes.len() as u32;
        self.pos = self.pos.wrapping_add(read);
        true
    }
}

/// Why a read failed: its seek (a position past the end, or a broken cluster chain), or the read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fail {
    Seek,
    Read,
}

/// A file the lookups read at positions.
pub trait Source {
    /// Reads bytes from `pos` into `buf`: how many (0 at the end).
    ///
    /// # Errors
    /// When seeking to `pos` or the read fails.
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> Result<usize, Fail>;
}

/// A word read from a source into the caller's buffer, cut at [`WORD_MAX`] bytes and followed by a
/// NUL.
#[derive(Debug)]
pub struct Word<'a> {
    bytes: &'a mut [u8; WORD_MAX + 1],
    len: usize,
}

impl<'a> Word<'a> {
    /// An empty word in `bytes`.
    pub fn new(bytes: &'a mut [u8; WORD_MAX + 1]) -> Self {
        bytes[0] = 0;
        Word { bytes, len: 0 }
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or_default()
    }

    /// Reads the NUL-terminated word at `pos` and returns where it ended (past its NUL); none
    /// when the source ended or failed first. A word cut at [`WORD_MAX`] bytes has its rest
    /// skipped, up to its NUL, the end or a failure.
    fn read(&mut self, src: &mut impl Source, pos: u32) -> Option<u32> {
        self.len = 0;
        let end = self.read_from(src, pos);
        if let Some(nul) = self.bytes.get_mut(self.len) {
            *nul = 0;
        }
        end
    }

    fn read_from(&mut self, src: &mut impl Source, mut pos: u32) -> Option<u32> {
        let mut chunk = [0u8; 32];
        loop {
            // A failed read ends a cut word's rest as the end does.
            let Ok(n) = src.read_at(pos, &mut chunk) else {
                return (self.len == WORD_MAX).then_some(pos);
            };
            let got = chunk.get(..n).unwrap_or_default();
            if got.is_empty() {
                // The end, or a cut word's rest running into it.
                return (self.len == WORD_MAX).then_some(pos);
            }
            let nul = got.iter().position(|&b| b == 0);
            let text = got.get(..nul.unwrap_or(n)).unwrap_or_default();
            let room = WORD_MAX - self.len;
            if let Some(dst) = self
                .bytes
                .get_mut(self.len..self.len + text.len().min(room))
            {
                dst.copy_from_slice(text.get(..dst.len()).unwrap_or_default());
                self.len += dst.len();
            }
            #[allow(clippy::cast_possible_truncation, reason = "a chunk")]
            match nul {
                Some(at) => return Some(pos.wrapping_add(at as u32 + 1)),
                None => pos = pos.wrapping_add(n as u32),
            }
        }
    }
}

/// `StringUtils::asciiCaseCmp`: ASCII letters compared without case, byte by byte, a prefix first.
fn ascii_case_cmp(a: &[u8], b: &[u8]) -> Ordering {
    a.iter()
        .map(u8::to_ascii_lowercase)
        .cmp(b.iter().map(u8::to_ascii_lowercase))
}

/// Reads sample `index` of a sidecar: its seek fails, or its 4 bytes as far as they were read.
fn sample(sidecar: &mut impl Source, index: u32) -> Option<(u32, bool)> {
    let pos = u32::try_from(HEADER_LEN)
        .ok()?
        .wrapping_add(index.wrapping_mul(4));
    let mut raw = [0u8; 4];
    let n = sidecar.read_at(pos, &mut raw).ok()?;
    Some((u32::from_le_bytes(raw), n == 4))
}

/// Where a definition is in the `.dict` data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Location {
    pub offset: u32,
    pub size: u32,
    pub found: bool,
    /// The search was cut short by a failed seek to its start rather than reaching a verdict.
    pub read_error: bool,
}

/// An index and its sidecar, as one lookup holds them open.
pub struct Index<'a, S: Source, T: Source> {
    pub source: &'a mut S,
    pub sidecar: &'a mut T,
    pub source_size: u32,
    /// 0 without a usable sidecar: lookups scan the source from its start.
    pub sample_count: u32,
    /// Entries the sidecar was built over; 0 when unknown.
    pub entry_count: u32,
}

impl<S: Source, T: Source> Index<'_, S, T> {
    /// Whether the source seeks to `pos`, as a lookup does before scanning from it.
    fn seeks(&mut self, pos: u32) -> bool {
        self.source.read_at(pos, &mut []) != Err(Fail::Seek)
    }

    /// Where the last sampled entry whose word is at most `target` starts: 0 without samples, or
    /// when a sample cannot be read.
    fn bisect(&mut self, target: &[u8], word: &mut Word<'_>) -> u32 {
        if self.sample_count == 0 {
            return 0;
        }
        let (mut lo, mut hi) = (0, self.sample_count - 1);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            let read = sample(self.sidecar, mid)
                .filter(|&(_, whole)| whole)
                .and_then(|(offset, _)| word.read(self.source, offset));
            if read.is_none() {
                lo = 0;
                break;
            }
            if ascii_case_cmp(word.as_bytes(), target) == Ordering::Greater {
                hi = mid - 1;
            } else {
                lo = mid;
            }
        }
        sample(self.sidecar, lo).map_or(0, |(offset, _)| offset)
    }

    /// Scans the entries (a word and `suffix` bytes) from the last sample at most `target` for
    /// the one whose word is `target` (ASCII case aside), up to the first word past it.
    fn scan<const N: usize>(
        &mut self,
        target: &[u8],
        word: &mut Word<'_>,
    ) -> Result<Option<[u8; N]>, ()> {
        let mut pos = self.bisect(target, word);
        if !self.seeks(pos) {
            return Err(());
        }
        while pos < self.source_size {
            let Some(end) = word.read(self.source, pos) else {
                break;
            };
            let mut suffix = [0u8; N];
            if self.source.read_at(end, &mut suffix) != Ok(N) {
                break;
            }
            #[allow(clippy::cast_possible_truncation, reason = "8 or 4")]
            let next = end.wrapping_add(N as u32);
            pos = next;
            match ascii_case_cmp(word.as_bytes(), target) {
                Ordering::Equal => return Ok(Some(suffix)),
                Ordering::Greater => break,
                Ordering::Less => {}
            }
        }
        Ok(None)
    }

    /// Looks `target` up in an `.idx` (entries: a word, then the definition's offset and size,
    /// big-endian); `word` gets the headword as stored.
    pub fn locate(&mut self, target: &[u8], word: &mut Word<'_>) -> Location {
        match self.scan::<{ IDX_SUFFIX as usize }>(target, word) {
            Ok(Some(suffix)) => found(suffix),
            Ok(None) => Location::default(),
            Err(()) => Location {
                read_error: true,
                ..Location::default()
            },
        }
    }

    /// The `ordinal`-th entry (from 0) of an `.idx`: from the sample before it when there are
    /// samples, else from the start; nothing past the last entry.
    pub fn locate_ordinal(&mut self, ordinal: u32, word: &mut Word<'_>) -> Location {
        let (mut pos, mut at) = (0, 0);
        if self.sample_count > 0 {
            if self.entry_count != 0 && ordinal >= self.entry_count {
                return Location::default();
            }
            let index = (ordinal / SAMPLE_INTERVAL).min(self.sample_count - 1);
            if let Some((offset, true)) = sample(self.sidecar, index) {
                (pos, at) = (offset, index * SAMPLE_INTERVAL);
            }
        }
        if !self.seeks(pos) {
            return Location {
                read_error: true,
                ..Location::default()
            };
        }
        let mut suffix = [0u8; 8];
        for _ in at..ordinal {
            let Some(end) = word.read(self.source, pos) else {
                return Location::default();
            };
            if self.source.read_at(end, &mut suffix) != Ok(8) {
                return Location::default();
            }
            pos = end.wrapping_add(8);
            if pos >= self.source_size {
                return Location::default();
            }
        }
        if pos >= self.source_size {
            return Location::default();
        }
        match word.read(self.source, pos) {
            Some(end) if self.source.read_at(end, &mut suffix) == Ok(8) => found(suffix),
            _ => Location::default(),
        }
    }
}

/// A found entry's location from its 8 suffix bytes.
fn found(suffix: [u8; 8]) -> Location {
    let [o0, o1, o2, o3, s0, s1, s2, s3] = suffix;
    Location {
        offset: u32::from_be_bytes([o0, o1, o2, o3]),
        size: u32::from_be_bytes([s0, s1, s2, s3]),
        found: true,
        read_error: false,
    }
}

/// Looks `target` up in a `.syn` (entries: a synonym, then the ordinal of its `.idx` entry,
/// big-endian) and follows a hit to `idx`.
pub fn locate_synonym<A: Source, B: Source, C: Source, D: Source>(
    syn: &mut Index<A, B>,
    idx: &mut Index<C, D>,
    target: &[u8],
    word: &mut Word<'_>,
) -> Location {
    match syn.scan::<{ SYN_SUFFIX as usize }>(target, word) {
        Ok(Some(ordinal)) => idx.locate_ordinal(u32::from_be_bytes(ordinal), word),
        Ok(None) => Location::default(),
        Err(()) => Location {
            read_error: true,
            ..Location::default()
        },
    }
}

/// Bytes that belong to a word: ASCII letters and digits, and any byte of a multi-byte character.
fn word_byte(b: u8) -> bool {
    b >= 0x80 || b.is_ascii_alphanumeric()
}

/// A General Punctuation character (U+2000 to U+207F: curly quotes, dashes) starting `bytes`.
fn punctuation(bytes: &[u8]) -> bool {
    matches!(bytes, [0xE2, 0x80 | 0x81, _, ..])
}

/// `word` without the punctuation around it (ASCII, and General Punctuation such as curly quotes
/// and dashes), ASCII lower-cased into `out`; how many bytes that took (as many as fit).
pub fn clean_word(word: &[u8], out: &mut [u8]) -> usize {
    let (mut start, mut end) = (0, word.len());
    while start < end {
        let rest = word.get(start..end).unwrap_or_default();
        if rest.first().is_some_and(|&b| !word_byte(b)) {
            start += 1;
        } else if punctuation(rest) {
            start += 3;
        } else {
            break;
        }
    }
    while end > start {
        let rest = word.get(start..end).unwrap_or_default();
        if rest.last().is_some_and(|&b| !word_byte(b)) {
            end -= 1;
        } else if rest.len() >= 3 && punctuation(rest.get(rest.len() - 3..).unwrap_or_default()) {
            end -= 3;
        } else {
            break;
        }
    }
    let kept = word.get(start..end).unwrap_or_default();
    for (o, &b) in out.iter_mut().zip(kept) {
        *o = b.to_ascii_lowercase();
    }
    kept.len().min(out.len())
}

/// A stem variant: the first `stem` bytes of the word, then `ending`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Variant {
    pub stem: usize,
    pub ending: &'static [u8],
}

/// The English stem variants of a cleaned `word` (-'s, -’s, -ies, -es, -s, -ed, -ing), in the order
/// to try them, each to `variant`. They never repeat: two of the same length would need the word
/// to end in two endings that exclude each other (the old reader's check for repeats never fired).
pub fn stem_variants(word: &[u8], mut variant: impl FnMut(Variant)) {
    let n = word.len();
    let mut add = |stem: usize, ending: &'static [u8]| variant(Variant { stem, ending });
    // A variant is never empty.
    let ends = |suffix: &[u8]| n > suffix.len() && word.ends_with(suffix);
    let at = |i: usize| word.get(i).copied();
    if ends(b"'s") {
        add(n - 2, b"");
    }
    if ends("\u{2019}s".as_bytes()) {
        add(n - 4, b"");
    }
    if ends(b"ies") {
        add(n - 3, b"y");
    }
    if ends(b"es") {
        add(n - 2, b"");
    }
    if ends(b"s") {
        add(n - 1, b"");
    }
    if ends(b"ed") {
        add(n - 2, b"");
        add(n - 1, b"");
        if n >= 4 && at(n - 3) == at(n - 4) {
            add(n - 3, b"");
        }
    }
    if ends(b"ing") {
        add(n - 3, b"");
        add(n - 3, b"e");
        if n >= 5 && at(n - 4) == at(n - 5) {
            add(n - 4, b"");
        }
    }
}

#[cfg(test)]
mod tests;
