//! One style of a `.cpfont`, read straight from the file for short texts such as a screen's
//! labels: the header and the style's interval table once, then each glyph's record and bitmap as
//! a line is laid out. It leaves out what `SdCardFont` keeps for a reader's pages (the resident
//! codepoints, kerning, ligatures, combining marks), so a book's text must be laid out with
//! `page`, `advance` and `kern` as `SdCardFont` does. A codepoint the style lacks is left out of
//! the line (`GfxRenderer` draws a solid box for it). Pen positions
//! advance as `GfxRenderer::drawText` advances them: each glyph's 12.4 fixed-point advance,
//! rounded to the nearest pixel.

use crate::Source;
use crate::cpfont::{self, Header, Style};
use crate::glyph::{self, Glyph};
use crate::intervals::{self, Loaded, Table};

/// Why a face could not be opened or a line laid out.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// The file's start could not be reached.
    Seek,
    Header(cpfont::Error),
    /// The font has no style with that id.
    NoStyle,
    Intervals(intervals::Error),
    Glyph(glyph::Error),
    Bitmap(glyph::BitmapError),
}

/// One style of a font, ready to lay out lines.
pub struct Face<'t> {
    pub style: Style,
    pub two_bit: bool,
    table: Table<'t>,
}

impl<'t> Face<'t> {
    /// Opens style `style_id` (0 is the regular one) of the font in `file`, its interval table
    /// in `table`, which must hold [`Table::bytes`] for the style.
    ///
    /// # Errors
    ///
    /// The header or the interval table cannot be read or is not sane, the style is absent, or
    /// `table` is too small for it (`Intervals(Reserve)`).
    pub fn open(file: &mut impl Source, style_id: u8, table: &'t mut [u8]) -> Result<Self, Error> {
        if !file.seek(0) {
            return Err(Error::Seek);
        }
        let mut header = Header::default();
        cpfont::read_header(&mut header, file).map_err(Error::Header)?;
        let style = header
            .styles
            .get(usize::from(style_id))
            .copied()
            .filter(|s| s.present)
            .ok_or(Error::NoStyle)?;
        let room = &mut *table;
        let loaded = intervals::load(file, &style, &[], move |_, bytes| room.get_mut(..bytes))
            .map_err(Error::Intervals)?;
        // With no earlier tables to share, the table is always the face's own.
        let Loaded::Own(compact) = loaded else {
            return Err(Error::NoStyle);
        };
        let bytes = Table::bytes(style.interval_count, compact);
        let entries = table
            .get(..bytes)
            .ok_or(Error::Intervals(intervals::Error::Reserve(compact)))?;
        Ok(Face {
            style,
            two_bit: header.two_bit,
            table: Table { entries, compact },
        })
    }

    /// How far the baseline is below the line's top, in pixels.
    #[must_use]
    pub fn ascender(&self) -> i32 {
        i32::from(self.style.ascender)
    }

    /// The distance from one line to the next, in pixels.
    #[must_use]
    pub fn line_height(&self) -> i32 {
        i32::from(self.style.advance_y)
    }

    /// Whether the style has a glyph for `c`.
    #[must_use]
    pub fn has(&self, c: char) -> bool {
        self.table.find(u32::from(c)).is_some()
    }

    /// Lays `text` out as one line: for each glyph that has ink, `each(pen, glyph, bits)`, `pen`
    /// being the glyph origin's distance from the line's start in pixels and `bits` its bitmap,
    /// read into `scratch`. The line's width.
    ///
    /// # Errors
    ///
    /// A glyph's record or bitmap cannot be read, or the bitmap does not fit `scratch`; the glyphs
    /// before it were handed over.
    pub fn line(
        &self,
        file: &mut impl Source,
        text: &str,
        scratch: &mut [u8],
        mut each: impl FnMut(i32, &Glyph, &[u8]),
    ) -> Result<i32, Error> {
        let mut pen = 0i32;
        for c in text.chars() {
            let Some(index) = self.table.find(u32::from(c)) else {
                continue;
            };
            let mut found = [Glyph::default()];
            glyph::read_glyphs(file, &self.style, self.two_bit, [index], |_, g| {
                if let Some(slot) = found.first_mut() {
                    *slot = g;
                }
            })
            .map_err(Error::Glyph)?;
            let [record] = found;
            // `read_bitmaps` points the copy's offset into `scratch`.
            let mut placed = [record];
            glyph::read_bitmaps(file, &self.style, &mut placed, &[0], scratch)
                .map_err(Error::Bitmap)?;
            let [placed] = placed;
            let start = placed.data_offset as usize;
            if let Some(bits) = scratch.get(start..start + usize::from(placed.data_length))
                && !bits.is_empty()
            {
                each(pen, &record, bits);
            }
            pen = pen.wrapping_add((i32::from(record.advance_x) + 8) >> 4);
        }
        Ok(pen)
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::vec::Vec;

    /// The emulator's SD card font: Liberation Serif at 14 px, Latin with its extensions.
    fn font() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../emu/books/LiberationSerif_14.cpfont"
        ))
        .unwrap()
    }

    struct File {
        bytes: Vec<u8>,
        at: usize,
        seeks: u32,
    }

    impl Source for File {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let n = buf.len().min(self.bytes.len().saturating_sub(self.at));
            buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
            self.at += n;
            n
        }
        fn seek(&mut self, position: u32) -> bool {
            self.seeks += 1;
            if position as usize > self.bytes.len() {
                return false;
            }
            self.at = position as usize;
            true
        }
    }

    fn file() -> File {
        File {
            bytes: font(),
            at: 77,
            seeks: 0,
        }
    }

    #[test]
    fn it_opens_the_regular_style_and_finds_turkish_letters() {
        let mut f = file();
        let mut table = [0u8; 4096];
        let face = Face::open(&mut f, 0, &mut table).unwrap();
        assert!(face.ascender() > 0 && face.line_height() > face.ascender());
        for c in "AaĞğÜüŞşİıÖöÇç0123456789".chars() {
            assert!(face.has(c), "{c}");
        }
        assert!(!face.has('中'));
    }

    #[test]
    fn a_line_advances_by_each_rounded_advance() {
        let mut f = file();
        let mut table = [0u8; 4096];
        let face = Face::open(&mut f, 0, &mut table).unwrap();
        let mut scratch = [0u8; 2048];
        let mut seen = Vec::new();
        let width = face
            .line(&mut f, "Sağ Güç", &mut scratch, |pen, g, bits| {
                seen.push((pen, g.width, g.height, bits.len()));
            })
            .unwrap();
        // Seven characters, the space without ink.
        assert_eq!(seen.len(), 6);
        // Each pen is the sum of the rounded advances before it; the width the sum of all.
        let mut pen = 0;
        let mut pens = Vec::new();
        for c in "Sağ Güç".chars() {
            let mut g = [Glyph::default()];
            let index = face.table.find(u32::from(c)).unwrap();
            glyph::read_glyphs(&mut f, &face.style, face.two_bit, [index], |_, r| g[0] = r)
                .unwrap();
            if g[0].data_length > 0 {
                pens.push(pen);
            }
            pen += (i32::from(g[0].advance_x) + 8) >> 4;
        }
        assert_eq!(seen.iter().map(|s| s.0).collect::<Vec<_>>(), pens);
        assert_eq!(width, pen);
        // A bitmap is as long as its pixels need.
        let bits = if face.two_bit { 2 } else { 1 };
        for &(_, w, h, len) in &seen {
            assert_eq!(len, (usize::from(w) * usize::from(h) * bits).div_ceil(8));
        }
    }

    #[test]
    fn a_missing_letter_is_left_out() {
        let mut f = file();
        let mut table = [0u8; 4096];
        let face = Face::open(&mut f, 0, &mut table).unwrap();
        let mut scratch = [0u8; 2048];
        let with = face
            .line(&mut f, "a中b", &mut scratch, |_, _, _| {})
            .unwrap();
        let without = face.line(&mut f, "ab", &mut scratch, |_, _, _| {}).unwrap();
        assert_eq!(with, without);
    }

    #[test]
    fn a_small_table_or_scratch_fails_without_panicking() {
        let mut f = file();
        let mut tiny = [0u8; 4];
        assert!(matches!(
            Face::open(&mut f, 0, &mut tiny),
            Err(Error::Intervals(intervals::Error::Reserve(_)))
        ));
        let mut table = [0u8; 4096];
        let face = Face::open(&mut f, 0, &mut table).unwrap();
        let mut scratch = [0u8; 4];
        assert!(matches!(
            face.line(&mut f, "W", &mut scratch, |_, _, _| {}),
            Err(Error::Bitmap(_))
        ));
        // Past the four styles a font can have.
        assert!(matches!(
            Face::open(&mut f, 4, &mut table),
            Err(Error::NoStyle)
        ));
    }
}
