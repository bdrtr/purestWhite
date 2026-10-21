//! Glyph records, as `SdCardFont` reads them for a page, its advance table and a glyph loaded on
//! demand, and a page's bitmaps (`test/sd_card_font` pins it).
//!
//! A record is 16 bytes, little-endian: width and height (u8), advance (u16, 12.4 fixed point),
//! left and top (i16), bitmap length (u16), two unused bytes, bitmap offset from the style's
//! bitmaps (u32). The bitmap is `width × height` pixels of 1 or 2 bits, packed without row padding.

use crate::Source;
use crate::cpfont::{GLYPH_SIZE, Style};

const RECORD: usize = GLYPH_SIZE as usize;

/// A glyph record. `EpdGlyph` and `bb_cpfont_glyph` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Glyph {
    pub width: u8,
    pub height: u8,
    pub advance_x: u16,
    pub left: i16,
    pub top: i16,
    pub data_length: u16,
    pub data_offset: u32,
}

/// Decodes a record. A glyph whose bitmap is shorter than its pixels would be read past its end
/// when drawn: it keeps its metrics for layout and gets no pixels.
#[must_use]
pub fn decode(b: &[u8; RECORD], two_bit: bool) -> Glyph {
    let mut glyph = Glyph {
        width: b[0],
        height: b[1],
        advance_x: u16::from_le_bytes([b[2], b[3]]),
        left: i16::from_le_bytes([b[4], b[5]]),
        top: i16::from_le_bytes([b[6], b[7]]),
        data_length: u16::from_le_bytes([b[8], b[9]]),
        data_offset: u32::from_le_bytes([b[12], b[13], b[14], b[15]]),
    };
    let bits = u32::from(glyph.width) * u32::from(glyph.height) * if two_bit { 2 } else { 1 };
    if u32::from(glyph.data_length) < bits.div_ceil(8) {
        glyph.width = 0;
        glyph.height = 0;
        glyph.data_length = 0;
    }
    glyph
}

/// Why a record could not be read; the glyph index.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    Seek(u32),
    Read(u32),
}

/// Reads the records of the glyphs in `indices`, in that order (ascending, so that runs of
/// consecutive glyphs read without seeking), and hands each to `glyph` with its position. The
/// first record always seeks: the file may be anywhere. Returns how many seeks it made.
///
/// # Errors
///
/// The first record that cannot be reached or read; the ones before it were handed over.
pub fn read_glyphs(
    file: &mut impl Source,
    style: &Style,
    two_bit: bool,
    indices: impl IntoIterator<Item = u32>,
    mut glyph: impl FnMut(usize, Glyph),
) -> Result<u32, Error> {
    let mut seeks = 0;
    let mut last: Option<u32> = None;
    for (i, at) in indices.into_iter().enumerate() {
        if last.is_none_or(|last| at != last.wrapping_add(1)) {
            if !file.seek(
                style
                    .glyphs_offset
                    .wrapping_add(at.wrapping_mul(GLYPH_SIZE)),
            ) {
                return Err(Error::Seek(at));
            }
            seeks += 1;
        }
        let mut record = [0u8; RECORD];
        if file.read(&mut record) != RECORD {
            return Err(Error::Read(at));
        }
        last = Some(at);
        glyph(i, decode(&record, two_bit));
    }
    Ok(seeks)
}

/// Why [`read_bitmaps`] failed; the position in `glyphs`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BitmapError {
    /// An `order` entry past `glyphs`, or a bitmap that does not fit `arena`.
    Arena(u32),
    Seek(u32),
    Read(u32),
}

/// Reads the bitmaps of `glyphs` into `arena` one after another, in `order` (ascending by bitmap
/// offset, so that adjacent bitmaps read without seeking), and points each glyph's offset at its
/// place in `arena`; a glyph without a bitmap gets the place the next one would take. Returns how
/// many seeks it made.
///
/// # Errors
///
/// A bitmap that cannot be reached, read or placed.
pub fn read_bitmaps(
    file: &mut impl Source,
    style: &Style,
    glyphs: &mut [Glyph],
    order: &[u32],
    arena: &mut [u8],
) -> Result<u32, BitmapError> {
    let mut seeks = 0;
    let mut at: usize = 0;
    let mut last_end = u32::MAX;
    for &i in order {
        let Some(glyph) = glyphs.get_mut(i as usize) else {
            return Err(BitmapError::Arena(i));
        };
        #[allow(clippy::cast_possible_truncation)]
        let place = at as u32;
        if glyph.data_length == 0 {
            glyph.data_offset = place;
            continue;
        }
        let position = style.bitmap_offset.wrapping_add(glyph.data_offset);
        let length = usize::from(glyph.data_length);
        let Some(bitmap) = arena.get_mut(at..at + length) else {
            return Err(BitmapError::Arena(i));
        };
        if position != last_end {
            if !file.seek(position) {
                return Err(BitmapError::Seek(i));
            }
            seeks += 1;
        }
        if file.read(bitmap) != length {
            return Err(BitmapError::Read(i));
        }
        last_end = position.wrapping_add(u32::from(glyph.data_length));
        glyph.data_offset = place;
        at += length;
    }
    Ok(seeks)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{BitmapError, Error, Glyph, decode, read_bitmaps, read_glyphs};
    use crate::Source;
    use crate::cpfont::Style;
    use std::vec;
    use std::vec::Vec;

    struct File {
        bytes: Vec<u8>,
        at: usize,
        seeks: Vec<usize>,
    }

    impl Source for File {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let n = buf.len().min(self.bytes.len().saturating_sub(self.at));
            buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
            self.at += n;
            n
        }
        fn seek(&mut self, position: u32) -> bool {
            self.seeks.push(position as usize);
            self.at = position as usize;
            self.at <= self.bytes.len()
        }
    }

    fn record(width: u8, height: u8, advance: u16, length: u16, offset: u32) -> [u8; 16] {
        let mut r = [0u8; 16];
        r[0] = width;
        r[1] = height;
        r[2..4].copy_from_slice(&advance.to_le_bytes());
        r[4..6].copy_from_slice(&(-2i16).to_le_bytes());
        r[6..8].copy_from_slice(&7i16.to_le_bytes());
        r[8..10].copy_from_slice(&length.to_le_bytes());
        r[12..16].copy_from_slice(&offset.to_le_bytes());
        r
    }

    #[test]
    fn decodes_records_and_drops_short_bitmaps() {
        let glyph = decode(&record(3, 5, 0x1234, 4, 77), true);
        assert_eq!(
            glyph,
            Glyph {
                width: 3,
                height: 5,
                advance_x: 0x1234,
                left: -2,
                top: 7,
                data_length: 4,
                data_offset: 77
            }
        );
        assert_eq!(decode(&record(3, 5, 9, 2, 0), false).width, 3);
        let short = decode(&record(3, 5, 9, 3, 0), true);
        assert_eq!(
            (
                short.width,
                short.height,
                short.data_length,
                short.advance_x
            ),
            (0, 0, 0, 9)
        );
        assert_eq!(decode(&record(255, 255, 0, u16::MAX, 0), true).width, 255);
    }

    // Four glyphs (8×8 1-bit, 8 bytes each) at 100, bitmaps at 300 in reverse order.
    fn font() -> (File, Style) {
        let mut bytes = vec![0u8; 400];
        for g in 0..4u8 {
            let at = 100 + usize::from(g) * 16;
            bytes[at..at + 16].copy_from_slice(&record(
                8,
                8,
                u16::from(g),
                8,
                u32::from(3 - g) * 8,
            ));
            let bitmap = 300 + usize::from(3 - g) * 8;
            bytes[bitmap..bitmap + 8].fill(g + 1);
        }
        let style = Style {
            glyphs_offset: 100,
            bitmap_offset: 300,
            ..Style::default()
        };
        (
            File {
                bytes,
                at: 0,
                seeks: Vec::new(),
            },
            style,
        )
    }

    #[test]
    fn reads_runs_without_seeking() {
        let (mut file, style) = font();
        let wanted = [0u32, 1, 3];
        let mut glyphs = vec![Glyph::default(); 3];
        let seeks = read_glyphs(&mut file, &style, false, wanted, |i, g| glyphs[i] = g).unwrap();
        assert_eq!((seeks, file.seeks.as_slice()), (2, &[100, 148][..]));
        assert_eq!(
            glyphs.iter().map(|g| g.advance_x).collect::<Vec<_>>(),
            [0, 1, 3]
        );

        // Bitmaps in file order: glyph 3's first.
        file.seeks.clear();
        let mut arena = [0u8; 24];
        let seeks = read_bitmaps(&mut file, &style, &mut glyphs, &[2, 1, 0], &mut arena).unwrap();
        assert_eq!((seeks, file.seeks.as_slice()), (2, &[300, 316][..]));
        assert_eq!(arena, [[4u8; 8], [2; 8], [1; 8]].concat()[..]);
        assert_eq!(
            glyphs.iter().map(|g| g.data_offset).collect::<Vec<_>>(),
            [16, 8, 0]
        );
    }

    #[test]
    fn reports_where_reads_fail() {
        let (mut file, style) = font();
        file.bytes.truncate(140);
        let mut seen = 0;
        let failed = read_glyphs(&mut file, &style, false, 0..4, |_, _| seen += 1);
        assert_eq!((failed, seen), (Err(Error::Read(2)), 2));
        let failed = read_glyphs(&mut file, &style, false, [9], |_, _| {});
        assert_eq!(failed, Err(Error::Seek(9)));

        let (mut file, style) = font();
        let mut glyphs = [decode(&record(8, 8, 0, 8, 0), false); 2];
        let mut arena = [0u8; 15];
        assert_eq!(
            read_bitmaps(&mut file, &style, &mut glyphs, &[0, 1], &mut arena),
            Err(BitmapError::Arena(1))
        );
        assert_eq!(file.seeks, [300]); // the second bitmap is refused before its seek
        assert_eq!(
            read_bitmaps(&mut file, &style, &mut glyphs, &[2], &mut arena),
            Err(BitmapError::Arena(2))
        );
        file.bytes.truncate(304);
        assert_eq!(
            read_bitmaps(&mut file, &style, &mut glyphs, &[0], &mut [0u8; 8]),
            Err(BitmapError::Read(0))
        );
    }

    #[test]
    fn any_records_never_panic() {
        let mut seed = 0x0bad_f00d_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..1000 {
            let bytes: Vec<u8> = (0..next() % 600).map(|_| next().to_le_bytes()[0]).collect();
            let style = Style {
                glyphs_offset: next() % 700,
                bitmap_offset: next(),
                ..Style::default()
            };
            let mut file = File {
                bytes,
                at: 0,
                seeks: Vec::new(),
            };
            let count = (next() % 40) as usize;
            let mut glyphs = vec![Glyph::default(); count];
            let mut index = next() % 50;
            let indices: Vec<u32> = (0..count)
                .map(|_| {
                    index += next() % 3;
                    index
                })
                .collect();
            let _ = read_glyphs(&mut file, &style, next() % 2 == 0, indices, |i, g| {
                glyphs[i] = g;
            });
            let order: Vec<u32> = (0..count as u32 + 2).collect();
            let mut arena = vec![0u8; (next() % 300) as usize];
            let _ = read_bitmaps(&mut file, &style, &mut glyphs, &order, &mut arena);
        }
    }
}
