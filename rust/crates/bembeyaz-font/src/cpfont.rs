//! The `.cpfont` header, style table of contents and interval tables, as `SdCardFont::load` checks
//! them before anything reads glyphs (`test/sd_card_font` pins it).
//!
//! Layout, little-endian: a 32-byte header (magic `CPFONT\0\0`, version at 8, flags at 10 with bit
//! 0 for 2-bit glyphs, style count at 12), then one 32-byte entry per style (style id, interval
//! and glyph counts, metrics, kern and ligature counts, data offset at 24). A style's data is its
//! 12-byte intervals, 16-byte glyphs, 3-byte left then right kern class entries, the class matrix,
//! 8-byte ligature pairs and the bitmaps, in that order. Offsets are 32-bit and wrap.

use crate::Source;

pub const HEADER_SIZE: usize = 32;
pub const TOC_ENTRY_SIZE: usize = 32;
pub const INTERVAL_SIZE: u32 = 12;
pub const VERSION: u16 = 4;
pub const MAX_STYLES: u8 = 4;
const MAGIC: &[u8; 8] = b"CPFONT\0\0";
const MAX_INTERVALS: u32 = 4096;
const MAX_GLYPHS: u32 = 65536;
const MAX_KERN_ENTRIES: u16 = 4096;
pub(crate) const GLYPH_SIZE: u32 = 16;
pub(crate) const KERN_ENTRY_SIZE: u32 = 3;
const LIGATURE_SIZE: u32 = 8;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([
        b.get(at).copied().unwrap_or(0),
        b.get(at + 1).copied().unwrap_or(0),
    ])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(core::array::from_fn(|i| {
        b.get(at + i).copied().unwrap_or(0)
    }))
}

/// FNV-1a 32 over `data`, continuing from `hash`.
fn fnv1a(data: &[u8], hash: u32) -> u32 {
    data.iter()
        .fold(hash, |h, &c| (h ^ u32::from(c)).wrapping_mul(16_777_619))
}

const FNV_OFFSET: u32 = 2_166_136_261;

/// Why [`read_header`] refuses a font. `BB_CPFONT_*` in C.
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    HeaderEnded = 1,
    NotCpfont = 2,
    /// The version is in [`Header::version`].
    WrongVersion = 3,
    /// The count is in [`Header::style_count`].
    BadStyleCount = 4,
    /// The entry's index is in [`Header::failed_entry`].
    TocEnded = 5,
    /// The id is in [`Header::failed_entry`].
    BadStyleId = 6,
    /// The style is [`Header::failed_entry`], its counts are in its entry.
    BadCounts = 7,
}

/// One style's table of contents entry and where its sections start. `bb_cpfont_style` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Style {
    pub interval_count: u32,
    pub glyph_count: u32,
    pub intervals_offset: u32,
    pub glyphs_offset: u32,
    pub kern_left_offset: u32,
    pub kern_right_offset: u32,
    pub kern_matrix_offset: u32,
    pub ligatures_offset: u32,
    pub bitmap_offset: u32,
    pub ascender: i16,
    pub descender: i16,
    pub kern_left_entries: u16,
    pub kern_right_entries: u16,
    pub advance_y: u8,
    pub kern_left_classes: u8,
    pub kern_right_classes: u8,
    pub ligature_pairs: u8,
    pub present: bool,
}

/// The header and table of contents. `bb_cpfont_header` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Header {
    /// By style id; a later entry with the same id replaces an earlier one.
    pub styles: [Style; MAX_STYLES as usize],
    /// FNV-1a over the header and the table of contents.
    pub content_hash: u32,
    pub version: u16,
    pub two_bit: bool,
    pub style_count: u8,
    pub failed_entry: u8,
}

/// Reads the header and the table of contents into `header` from `file`'s current position, one
/// read per 32-byte block, and lays out each style's sections.
///
/// # Errors
///
/// A short read, a wrong magic or version, a style count of 0 or more than [`MAX_STYLES`], a style
/// id of [`MAX_STYLES`] or more, or counts no sane font has. `header` keeps what was read so far.
pub fn read_header(header: &mut Header, file: &mut impl Source) -> Result<(), Error> {
    *header = Header::default();
    let mut b = [0u8; HEADER_SIZE];
    if file.read(&mut b) != HEADER_SIZE {
        return Err(Error::HeaderEnded);
    }
    if b.get(..8) != Some(&MAGIC[..]) {
        return Err(Error::NotCpfont);
    }
    header.version = u16_at(&b, 8);
    if header.version != VERSION {
        return Err(Error::WrongVersion);
    }
    header.content_hash = fnv1a(&b, FNV_OFFSET);
    header.two_bit = u16_at(&b, 10) & 1 != 0;
    header.style_count = b.get(12).copied().unwrap_or(0);
    if header.style_count == 0 || header.style_count > MAX_STYLES {
        return Err(Error::BadStyleCount);
    }
    for i in 0..header.style_count {
        let mut t = [0u8; TOC_ENTRY_SIZE];
        if file.read(&mut t) != TOC_ENTRY_SIZE {
            header.failed_entry = i;
            return Err(Error::TocEnded);
        }
        header.content_hash = fnv1a(&t, header.content_hash);
        let id = t.first().copied().unwrap_or(0);
        header.failed_entry = id;
        let Some(style) = header.styles.get_mut(usize::from(id)) else {
            return Err(Error::BadStyleId);
        };
        *style = toc_entry(&t);
        if style.interval_count > MAX_INTERVALS
            || style.glyph_count > MAX_GLYPHS
            || style.kern_left_entries > MAX_KERN_ENTRIES
            || style.kern_right_entries > MAX_KERN_ENTRIES
        {
            return Err(Error::BadCounts);
        }
    }
    header.failed_entry = 0;
    Ok(())
}

fn toc_entry(t: &[u8; TOC_ENTRY_SIZE]) -> Style {
    let mut s = Style {
        interval_count: u32_at(t, 4),
        glyph_count: u32_at(t, 8),
        advance_y: t.get(12).copied().unwrap_or(0),
        ascender: i16::from_le_bytes(u16_at(t, 13).to_le_bytes()),
        descender: i16::from_le_bytes(u16_at(t, 15).to_le_bytes()),
        kern_left_entries: u16_at(t, 17),
        kern_right_entries: u16_at(t, 19),
        kern_left_classes: t.get(21).copied().unwrap_or(0),
        kern_right_classes: t.get(22).copied().unwrap_or(0),
        ligature_pairs: t.get(23).copied().unwrap_or(0),
        present: true,
        ..Style::default()
    };
    s.intervals_offset = u32_at(t, 24);
    s.glyphs_offset = s
        .intervals_offset
        .wrapping_add(s.interval_count.wrapping_mul(INTERVAL_SIZE));
    s.kern_left_offset = s
        .glyphs_offset
        .wrapping_add(s.glyph_count.wrapping_mul(GLYPH_SIZE));
    s.kern_right_offset = s
        .kern_left_offset
        .wrapping_add(u32::from(s.kern_left_entries) * KERN_ENTRY_SIZE);
    s.kern_matrix_offset = s
        .kern_right_offset
        .wrapping_add(u32::from(s.kern_right_entries) * KERN_ENTRY_SIZE);
    s.ligatures_offset = s
        .kern_matrix_offset
        .wrapping_add(u32::from(s.kern_left_classes) * u32::from(s.kern_right_classes));
    s.bitmap_offset = s
        .ligatures_offset
        .wrapping_add(u32::from(s.ligature_pairs) * LIGATURE_SIZE);
    s
}

/// Why an interval is refused; the flags say how its layout is wrong.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct IntervalFault {
    pub reversed: bool,
    pub overlaps: bool,
    pub span_too_big: bool,
    pub offset_mismatch: bool,
    pub offset_overruns: bool,
    /// Codepoints in the interval, as computed (wrapping).
    pub span: u32,
}

/// Checks a style's intervals one by one: sorted, not overlapping, each starting where the glyphs
/// of the last ended, none past the glyphs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct IntervalCheck {
    glyph_count: u32,
    expected_offset: u32,
    prev_last: u32,
    bmp16: bool,
    started: bool,
}

impl IntervalCheck {
    #[must_use]
    pub fn new(glyph_count: u32) -> Self {
        IntervalCheck {
            glyph_count,
            expected_offset: 0,
            prev_last: 0,
            bmp16: u16::try_from(glyph_count).is_ok(),
            started: false,
        }
    }

    /// Every interval so far fits the compact 16-bit table.
    #[must_use]
    pub fn bmp16(&self) -> bool {
        self.bmp16
    }

    /// Checks the next interval (first, last, glyph offset).
    ///
    /// # Errors
    ///
    /// How the interval breaks the table's layout.
    pub fn check(&mut self, first: u32, last: u32, offset: u32) -> Result<(), IntervalFault> {
        if first > last {
            return Err(IntervalFault {
                reversed: true,
                ..IntervalFault::default()
            });
        }
        let span = last.wrapping_sub(first).wrapping_add(1);
        let fault = IntervalFault {
            reversed: false,
            overlaps: self.started && first <= self.prev_last,
            span_too_big: span > self.glyph_count,
            offset_mismatch: offset != self.expected_offset,
            offset_overruns: offset > self.glyph_count.wrapping_sub(span),
            span,
        };
        if fault.overlaps || fault.span_too_big || fault.offset_mismatch || fault.offset_overruns {
            return Err(fault);
        }
        let limit = u32::from(u16::MAX);
        if first > limit || last > limit || offset > limit {
            self.bmp16 = false;
        }
        self.expected_offset = self.expected_offset.wrapping_add(span);
        self.prev_last = last;
        self.started = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used)]

    use super::{Error, Header, IntervalCheck, MAX_STYLES, VERSION};
    use crate::Source;
    use std::vec::Vec;

    struct Reader<'a>(&'a [u8]);

    impl Source for Reader<'_> {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let n = buf.len().min(self.0.len());
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            n
        }
        fn seek(&mut self, _: u32) -> bool {
            false
        }
    }

    #[allow(clippy::result_large_err)]
    fn read(bytes: &[u8]) -> Result<Header, (Error, Header)> {
        let mut font = Header::default();
        match super::read_header(&mut font, &mut Reader(bytes)) {
            Ok(()) => Ok(font),
            Err(e) => Err((e, font)),
        }
    }

    fn font(styles: &[[u8; 32]]) -> Vec<u8> {
        let mut v = Vec::from(*b"CPFONT\0\0");
        v.resize(32, 0);
        v[8..10].copy_from_slice(&VERSION.to_le_bytes());
        v[10] = 1;
        v[12] = u8::try_from(styles.len()).unwrap();
        for s in styles {
            v.extend_from_slice(s);
        }
        v
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn header_and_toc() {
        let mut t = [0u8; 32];
        t[0] = 1;
        t[4] = 2;
        t[8] = 26;
        t[24] = 64;
        let f = read(&font(&[t])).unwrap();
        assert!(f.two_bit && f.style_count == 1 && !f.styles[0].present);
        let s = f.styles[1];
        assert!(s.present);
        assert_eq!(
            (s.intervals_offset, s.glyphs_offset, s.kern_left_offset),
            (64, 88, 88 + 26 * 16)
        );
        t[0] = MAX_STYLES;
        let (e, f) = read(&font(&[t])).unwrap_err();
        assert_eq!((e, f.failed_entry), (Error::BadStyleId, MAX_STYLES));
        let bytes = font(&[t, t]);
        assert_eq!(read(&bytes[..40]).unwrap_err().0, Error::TocEnded);
        assert_eq!(read(&bytes[..31]).unwrap_err().0, Error::HeaderEnded);
        let mut v = font(&[]);
        assert_eq!(read(&v).unwrap_err().0, Error::BadStyleCount);
        v[0] = b'X';
        assert_eq!(read(&v).unwrap_err().0, Error::NotCpfont);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn counts_duplicates_and_wrapping() {
        let mut t = [0u8; 32];
        t[4..8].copy_from_slice(&4097u32.to_le_bytes());
        let (e, f) = read(&font(&[t])).unwrap_err();
        assert_eq!(
            (e, f.failed_entry, f.styles[0].interval_count),
            (Error::BadCounts, 0, 4097)
        );
        t[4..8].copy_from_slice(&4096u32.to_le_bytes());
        t[8..12].copy_from_slice(&65537u32.to_le_bytes());
        assert_eq!(read(&font(&[t])).unwrap_err().0, Error::BadCounts);
        t[8..12].copy_from_slice(&65536u32.to_le_bytes());
        t[17..19].copy_from_slice(&4097u16.to_le_bytes());
        assert_eq!(read(&font(&[t])).unwrap_err().0, Error::BadCounts);
        t[17..19].copy_from_slice(&4096u16.to_le_bytes());
        t[24..28].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        let f = read(&font(&[t])).unwrap();
        assert_eq!(
            f.styles[0].glyphs_offset,
            0xFFFF_FFF0u32.wrapping_add(4096 * 12)
        );

        // A later entry with the same id replaces the earlier one; the count stays the header's.
        let mut a = [0u8; 32];
        a[8] = 5;
        let mut b = a;
        b[8] = 9;
        let f = read(&font(&[a, b])).unwrap();
        assert_eq!(
            (f.style_count, f.styles[0].glyph_count, f.styles[1].present),
            (2, 9, false)
        );
    }

    #[test]
    fn intervals() {
        let mut c = IntervalCheck::new(30);
        assert!(c.check('A' as u32, 'M' as u32, 0).is_ok());
        assert!(c.check('O' as u32, 'Z' as u32, 13).is_ok());
        assert!(c.check('Y' as u32, 'Z' as u32, 25).unwrap_err().overlaps);
        assert!(IntervalCheck::new(5).check(1, 0, 0).unwrap_err().reversed);
        let mut big = IntervalCheck::new(10);
        assert!(big.check(0x1F600, 0x1F601, 0).is_ok());
        assert!(!big.bmp16());
    }

    #[test]
    fn any_bytes_never_panic() {
        let mut seed = 0x1357_2468_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..2000 {
            let mut v = font(&[[0; 32]; 4]);
            v[12] = u8::try_from(next() % 6).unwrap();
            for _ in 0..next() % 12 {
                let at = usize::try_from(next()).unwrap() % v.len();
                v[at] = next().to_le_bytes()[0];
            }
            let cut = usize::try_from(next()).unwrap() % (v.len() + 1);
            let _ = read(&v[..cut]);
            let mut c = IntervalCheck::new(next());
            for _ in 0..4 {
                let first = next();
                let _ = c.check(first, first.wrapping_add(next() % 300), next() % 300);
            }
        }
    }
}
