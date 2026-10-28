//! Text on the panel in a font from the SD card: one style of a `.cpfont`
//! (`bembeyaz_font::face`), its glyphs drawn with `bembeyaz_gfx`. A [`Writer`] holds only the
//! style; the font's bytes come from a `bembeyaz_font::Source` at each call, so the caller keeps
//! the volume for its other files. [`FontFile`] reads a file on the volume through blocking calls
//! (`crate::ready`), as `sd::BlockingBus` gives them.

use crate::ready;
use bembeyaz_font::Source;
use bembeyaz_font::face::{Error as FaceError, Face};
use bembeyaz_gfx::{Bitmap, Canvas, Frame, Plane};
use purestwhite_fat::{BlockDevice, File, Volume};

/// Bytes a glyph's bitmap may take: a 14 px font's take a few dozen.
pub const SCRATCH: usize = 1024;

/// A file on the volume as the font's reader sees it, for one call.
pub struct FontFile<'v, D> {
    pub volume: &'v mut Volume<D>,
    pub file: &'v mut File,
}

impl<D: BlockDevice> Source for FontFile<'_, D> {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        // A read that fails, or is not done at once (the blocking bus always is), reads nothing:
        // the font's reader takes that as the file's end and fails the text.
        match ready(self.volume.read(self.file, buf)) {
            Some(Ok(n)) => n,
            _ => 0,
        }
    }

    fn seek(&mut self, position: u32) -> bool {
        self.file.seek(position);
        self.file.position() == position
    }
}

/// One style of a font, ready to draw.
pub struct Writer<'t> {
    face: Face<'t>,
    scratch: [u8; SCRATCH],
}

impl<'t> Writer<'t> {
    /// Opens style `style_id` (0 regular) of the font `src` reads, its interval table in `table`
    /// (`bembeyaz_font::face::Face::open`; 2 KiB is ample for a Latin font).
    ///
    /// # Errors
    ///
    /// The font cannot be read or is not sane.
    pub fn open(
        src: &mut impl Source,
        style_id: u8,
        table: &'t mut [u8],
    ) -> Result<Self, FaceError> {
        Ok(Writer {
            face: Face::open(src, style_id, table)?,
            scratch: [0; SCRATCH],
        })
    }

    /// The distance from one line to the next, in pixels.
    #[must_use]
    pub fn line_height(&self) -> i32 {
        self.face.line_height()
    }

    /// How wide `text` is, in pixels; 0 when a glyph cannot be read.
    pub fn width(&mut self, src: &mut impl Source, text: &str) -> i32 {
        self.face
            .line(src, text, &mut self.scratch, |_, _, _| {})
            .unwrap_or(0)
    }

    /// Draws `text` in black with its line's top at logical (`x`, `y`) on `canvas`, in the canvas's
    /// orientation; the glyphs after one that cannot be read are left out. Its width, or 0 when a
    /// glyph could not be read.
    pub fn draw(
        &mut self,
        src: &mut impl Source,
        canvas: &mut Canvas<'_>,
        x: i32,
        y: i32,
        text: &str,
    ) -> i32 {
        let baseline = y.wrapping_add(self.face.ascender());
        let two_bit = self.face.two_bit;
        self.face
            .line(src, text, &mut self.scratch, |pen, glyph, bits| {
                let bitmap = Bitmap {
                    bits,
                    width: i32::from(glyph.width),
                    height: i32::from(glyph.height),
                    two_bit,
                };
                // As `GfxRenderer`'s upright text: pixel (0, 0) at the pen plus the glyph's left,
                // its top above the baseline.
                let frame = Frame {
                    x: x.wrapping_add(pen).wrapping_add(i32::from(glyph.left)),
                    y: baseline.wrapping_sub(i32::from(glyph.top)),
                    dx_x: 1,
                    dx_y: 0,
                    dy_x: 0,
                    dy_y: 1,
                };
                canvas.glyph(&bitmap, &frame, Plane::Bw, true);
            })
            .unwrap_or(0)
    }
}
