//! A style's interval table, as `SdCardFont` loads it and looks codepoints up in it
//! (`test/sd_card_font` pins it).
//!
//! The file holds 12-byte records (first codepoint, last codepoint, first glyph; u32,
//! little-endian). A loaded table keeps them as they are, or as 6-byte compact records (u16) when
//! every value fits, and a style whose table equals an earlier style's shares it.

use crate::Source;
use crate::cpfont::{INTERVAL_SIZE, IntervalCheck, IntervalFault, MAX_STYLES, Style};

const RECORD: usize = INTERVAL_SIZE as usize;
const COMPACT: usize = 6;

#[inline]
fn full_record(r: &[u8; RECORD]) -> [u32; 3] {
    [
        u32::from_le_bytes([r[0], r[1], r[2], r[3]]),
        u32::from_le_bytes([r[4], r[5], r[6], r[7]]),
        u32::from_le_bytes([r[8], r[9], r[10], r[11]]),
    ]
}

#[inline]
fn compact_record(r: [u8; COMPACT]) -> [u32; 3] {
    [
        u32::from(u16::from_le_bytes([r[0], r[1]])),
        u32::from(u16::from_le_bytes([r[2], r[3]])),
        u32::from(u16::from_le_bytes([r[4], r[5]])),
    ]
}

/// A loaded table's records.
#[derive(Clone, Copy, Debug)]
pub struct Table<'a> {
    pub entries: &'a [u8],
    pub compact: bool,
}

impl Table<'_> {
    /// Bytes `count` records take.
    #[must_use]
    pub fn bytes(count: u32, compact: bool) -> usize {
        count as usize * if compact { COMPACT } else { RECORD }
    }

    #[inline]
    fn len(&self) -> usize {
        self.entries.len() / if self.compact { COMPACT } else { RECORD }
    }

    #[inline]
    fn record(&self, i: usize) -> Option<[u32; 3]> {
        if self.compact {
            let r = self.entries.get(i * COMPACT..)?.first_chunk::<COMPACT>()?;
            Some(compact_record(*r))
        } else {
            let r = self.entries.get(i * RECORD..)?.first_chunk::<RECORD>()?;
            Some(full_record(r))
        }
    }

    /// The glyph index of `codepoint`, if the table covers it.
    #[must_use]
    pub fn find(&self, codepoint: u32) -> Option<u32> {
        let (mut low, mut high) = (0, self.len());
        while low < high {
            let mid = low + (high - low) / 2;
            let [first, last, offset] = self.record(mid)?;
            if codepoint < first {
                high = mid;
            } else if codepoint > last {
                low = mid + 1;
            } else {
                return Some(offset.wrapping_add(codepoint - first));
            }
        }
        None
    }
}

/// Why [`load`] failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// The table's start could not be reached.
    Seek,
    /// This record could not be read while checking.
    CheckRead(u32),
    /// This record (first, last) breaks the table's layout.
    Invalid(u32, u32, u32, IntervalFault),
    /// The table's start could not be reached again.
    SeekBack,
    /// `reserve` gave no table of this form (compact or not).
    Reserve(bool),
    /// This record could not be read into the compact table.
    CompactRead(u32),
    /// The records could not be read into the full table.
    FullRead,
}

/// What [`load`] did.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Loaded {
    /// The table equals `earlier[k]`'s, in the same form: use that one.
    Shared(usize),
    /// The table was read into the memory `reserve` gave, compact or not.
    Own(bool),
}

/// Loads `style`'s interval table from `file`. Every record is checked first (see
/// [`IntervalCheck`]) and compared with the earlier styles' tables of the same length (`earlier`,
/// `None` for a style that is absent; at most [`MAX_STYLES`] are looked at); one that matched every
/// record in the same form is shared. Otherwise `reserve` is asked for [`Table::bytes`] bytes,
/// compact when every value and the glyph count fit 16 bits, and the records are read again into
/// them: one read per record for a compact table, one read for all of them otherwise.
///
/// # Errors
///
/// The table cannot be reached, a record cannot be read or breaks the layout, or `reserve` gives
/// no memory.
pub fn load<'t>(
    file: &mut impl Source,
    style: &Style,
    earlier: &[Option<Table<'_>>],
    reserve: impl FnOnce(bool, usize) -> Option<&'t mut [u8]>,
) -> Result<Loaded, Error> {
    if !file.seek(style.intervals_offset) {
        return Err(Error::Seek);
    }
    let count = style.interval_count;
    let mut check = IntervalCheck::new(style.glyph_count);
    // A style stays a candidate only while its table has matched every record so far.
    let mut candidates: u8 = 0;
    for (k, table) in earlier.iter().enumerate().take(usize::from(MAX_STYLES)) {
        if table.is_some_and(|t| t.len() == count as usize) {
            candidates |= 1 << k;
        }
    }
    let mut record = [0u8; RECORD];
    for j in 0..count {
        if file.read(&mut record) != RECORD {
            return Err(Error::CheckRead(j));
        }
        let [first, last, offset] = full_record(&record);
        if let Err(fault) = check.check(first, last, offset) {
            return Err(Error::Invalid(j, first, last, fault));
        }
        for (k, table) in earlier.iter().enumerate().take(usize::from(MAX_STYLES)) {
            if candidates & (1 << k) != 0
                && table.and_then(|t| t.record(j as usize)) != Some([first, last, offset])
            {
                candidates &= !(1 << k);
            }
        }
    }
    let compact = check.bmp16();
    // Equal content can still be held in the other form when the styles' glyph counts differ.
    for (k, table) in earlier.iter().enumerate().take(usize::from(MAX_STYLES)) {
        if candidates & (1 << k) != 0 && table.is_some_and(|t| t.compact == compact) {
            return Ok(Loaded::Shared(k));
        }
    }

    if !file.seek(style.intervals_offset) {
        return Err(Error::SeekBack);
    }
    let Some(entries) = reserve(compact, Table::bytes(count, compact)) else {
        return Err(Error::Reserve(compact));
    };
    if compact {
        for (j, out) in (0..count).zip(entries.as_chunks_mut::<COMPACT>().0) {
            if file.read(&mut record) != RECORD {
                return Err(Error::CompactRead(j));
            }
            // Every value fits 16 bits: the low halves of the three u32s.
            *out = [
                record[0], record[1], record[4], record[5], record[8], record[9],
            ];
        }
    } else if file.read(entries) != entries.len() {
        return Err(Error::FullRead);
    }
    Ok(Loaded::Own(compact))
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{Error, Loaded, Table, load};
    use crate::Source;
    use crate::cpfont::Style;
    use std::vec;
    use std::vec::Vec;

    struct File {
        bytes: Vec<u8>,
        at: usize,
        reads: usize,
    }

    impl Source for File {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            self.reads += 1;
            let n = buf.len().min(self.bytes.len().saturating_sub(self.at));
            buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
            self.at += n;
            n
        }
        fn seek(&mut self, position: u32) -> bool {
            self.at = position as usize;
            self.at <= self.bytes.len()
        }
    }

    fn file(records: &[[u32; 3]]) -> (File, Style) {
        let bytes = records
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let glyphs = records.iter().map(|r| r[1] - r[0] + 1).sum();
        let style = Style {
            interval_count: records.len() as u32,
            glyph_count: glyphs,
            ..Style::default()
        };
        (
            File {
                bytes,
                at: 0,
                reads: 0,
            },
            style,
        )
    }

    fn own(
        file: &mut File,
        style: &Style,
        earlier: &[Option<Table<'_>>],
        memory: &mut Vec<u8>,
    ) -> Result<Loaded, Error> {
        load(file, style, earlier, |_, bytes| {
            memory.resize(bytes, 0);
            Some(&mut memory[..])
        })
    }

    #[test]
    fn loads_compact_and_full_tables_and_finds_glyphs() {
        let (mut f, style) = file(&[[0x41, 0x5A, 0], [0x61, 0x7A, 26]]);
        let mut compact = Vec::new();
        assert_eq!(
            own(&mut f, &style, &[], &mut compact),
            Ok(Loaded::Own(true))
        );
        assert_eq!(f.reads, 2 + 2);
        let table = Table {
            entries: &compact,
            compact: true,
        };
        assert_eq!(
            (
                table.find(0x41),
                table.find(0x63),
                table.find(0x60),
                table.find(0x10061)
            ),
            (Some(0), Some(28), None, None)
        );

        let (mut f, style) = file(&[[0x41, 0x42, 0], [0x1F600, 0x1F601, 2]]);
        let mut full = Vec::new();
        assert_eq!(own(&mut f, &style, &[], &mut full), Ok(Loaded::Own(false)));
        assert_eq!(f.reads, 2 + 1);
        let table = Table {
            entries: &full,
            compact: false,
        };
        assert_eq!(table.find(0x1F601), Some(3));
    }

    #[test]
    fn shares_an_earlier_equal_table_of_the_same_form() {
        let records = [[0x41, 0x5A, 0]];
        let (mut f, style) = file(&records);
        let mut first = Vec::new();
        own(&mut f, &style, &[], &mut first).unwrap();
        let earlier = [
            None,
            Some(Table {
                entries: &first,
                compact: true,
            }),
        ];
        let (mut f, style) = file(&records);
        assert_eq!(
            load(&mut f, &style, &earlier, |_, _| None),
            Ok(Loaded::Shared(1))
        );
        // Too many glyphs for the compact form: the same records, but a table of its own.
        let wide = Style {
            glyph_count: 65536,
            ..style
        };
        let mut second = Vec::new();
        assert_eq!(
            own(&mut f, &wide, &earlier, &mut second),
            Ok(Loaded::Own(false))
        );
        let (mut f, style) = file(&[[0x41, 0x59, 0]]);
        assert_eq!(
            own(&mut f, &style, &earlier, &mut second),
            Ok(Loaded::Own(true))
        );
    }

    #[test]
    fn reports_bad_tables() {
        let (mut f, style) = file(&[[0x41, 0x5A, 0], [0x50, 0x60, 26]]);
        assert!(matches!(
            own(&mut f, &style, &[], &mut Vec::new()),
            Err(Error::Invalid(1, 0x50, 0x60, _))
        ));
        let (mut f, style) = file(&[[0x41, 0x5A, 0]]);
        f.bytes.truncate(11);
        assert_eq!(
            own(&mut f, &style, &[], &mut Vec::new()),
            Err(Error::CheckRead(0))
        );
        let (mut f, style) = file(&[[0x41, 0x5A, 0]]);
        assert_eq!(
            load(&mut f, &style, &[], |_, _| None),
            Err(Error::Reserve(true))
        );
        let far = Style {
            intervals_offset: 99,
            ..style
        };
        assert_eq!(own(&mut f, &far, &[], &mut Vec::new()), Err(Error::Seek));
    }

    #[test]
    fn any_tables_never_panic() {
        let mut seed = 0x5eed_1234_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..1000 {
            let bytes: Vec<u8> = (0..next() % 400).map(|_| next().to_le_bytes()[0]).collect();
            let style = Style {
                interval_count: next() % 40,
                glyph_count: next() % 70000,
                intervals_offset: next() % 420,
                ..Style::default()
            };
            let mut f = File {
                bytes,
                at: 0,
                reads: 0,
            };
            let junk = vec![next().to_le_bytes()[0]; (next() % 300) as usize];
            let earlier = [
                Some(Table {
                    entries: &junk,
                    compact: next() % 2 == 0,
                }),
                None,
            ];
            let _ = own(&mut f, &style, &earlier, &mut Vec::new());
            let table = Table {
                entries: &junk,
                compact: next() % 2 == 0,
            };
            let _ = table.find(next());
        }
    }
}
