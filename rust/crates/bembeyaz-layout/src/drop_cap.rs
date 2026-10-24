//! The chapter parser's drop cap: how much its letter is enlarged, how wide it is, and where it
//! sits beside the lines it spans. Its top lines up with the first line's capitals and its
//! baseline with the last line's.

/// Glyph metrics from the parser's fonts.
pub trait Fonts {
    /// The advance (12.4 fixed point) and top of `cp` in the body text's font, or in the cap's
    /// font when `cap`; `None` when the font has no such glyph.
    fn metrics(&mut self, cap: bool, cp: u32) -> Option<(i32, i32)>;
}

/// The body text the letter stands beside, in pixels.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub line_height: i32,
    pub ascender: i32,
    /// A space in the body font: the gap between the letter and the text.
    pub space_width: i32,
    /// The room for the letter and the text beside it; the letter takes at most half.
    pub width: u16,
}

/// Where a drop cap goes. `bb_drop_cap` in C.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropCap {
    /// The letter's enlargement, in 256ths (1 to 16 times).
    pub scale: u16,
    /// The letter and the gap after it; the lines beside it move right by this much.
    pub width: i32,
    /// From the top of its first line to the letter's baseline.
    pub baseline: i32,
    /// The height of the lines it spans.
    pub height: i32,
}

const FRAC_BITS: u32 = 4;

/// A 12.4 fixed-point length in whole pixels, rounded to the nearest, as `fp4::toPixel`.
fn to_pixel(fp: i32) -> i32 {
    fp.wrapping_add(1 << (FRAC_BITS - 1)) >> FRAC_BITS
}

/// Places the letters of `text` (UTF-8, up to a NUL) as a drop cap `lines` lines tall.
#[must_use]
pub fn place(lines: u8, text: &[u8], body: &Body, fonts: &mut impl Fonts) -> DropCap {
    // Without an 'H', a capital is about 70% of the ascender.
    let body_cap_top = fonts
        .metrics(false, u32::from('H'))
        .map_or(body.ascender * 7 / 10, |(_, top)| top);
    let source_cap_top = match fonts.metrics(true, u32::from('H')) {
        Some((_, top)) if top > 0 => top,
        _ if body_cap_top > 0 => body_cap_top,
        _ => 1,
    };
    let spanned = i32::from(lines) - 1;
    let cap_height = spanned * body.line_height + body_cap_top;
    let scale = (cap_height * 256 / source_cap_top).clamp(256, 4096);
    let mut width = body.space_width;
    for cp in bembeyaz_utf8::codepoints(text) {
        if let Some((advance, _)) = fonts.metrics(true, cp) {
            width += to_pixel(advance * scale / 256);
        }
    }
    // The clamp keeps it to 16 bits.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    DropCap {
        scale: scale as u16,
        width: width.min(i32::from(body.width / 2)),
        baseline: spanned * body.line_height + body.ascender,
        height: i32::from(lines) * body.line_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Body capitals 14 tall; the cap's font has 'H' 20 tall and every letter 10.5 wide.
    struct Glyphs {
        body_h: Option<i32>,
        cap_h: Option<i32>,
    }

    impl Fonts for Glyphs {
        fn metrics(&mut self, cap: bool, cp: u32) -> Option<(i32, i32)> {
            match (cap, cp) {
                (false, 0x48) => self.body_h.map(|top| (0, top)),
                (true, 0x48) => self.cap_h.map(|top| (168, top)),
                (true, 0x41..=0x5A | 0x201C) => Some((168, 0)),
                _ => None,
            }
        }
    }

    const BODY: Body = Body {
        line_height: 30,
        ascender: 22,
        space_width: 6,
        width: 400,
    };

    #[test]
    fn spans_the_lines_from_cap_top_to_last_baseline() {
        let mut glyphs = Glyphs {
            body_h: Some(14),
            cap_h: Some(20),
        };
        let cap = place(3, "\u{201C}A".as_bytes(), &BODY, &mut glyphs);
        // (2 * 30 + 14) / 20 = 3.7 times: 947/256.
        assert_eq!(cap.scale, 947);
        // Each letter 168 * 947 / 256 / 16 = 38.8 px, plus the space.
        assert_eq!(cap.width, 6 + 39 + 39);
        assert_eq!((cap.baseline, cap.height), (82, 90));
    }

    #[test]
    fn falls_back_without_metrics() {
        // No body 'H': 70% of the ascender (15); no cap 'H': the body's capitals.
        let mut glyphs = Glyphs {
            body_h: None,
            cap_h: None,
        };
        let cap = place(2, b"Z\0ignored", &BODY, &mut glyphs);
        assert_eq!(cap.scale, 768);
        assert_eq!(cap.width, 6 + 32);
        // A capital of no height in either font still divides by 1, and the scale stays 16 times.
        let mut flat = Glyphs {
            body_h: Some(0),
            cap_h: Some(0),
        };
        assert_eq!(place(4, b"", &BODY, &mut flat).scale, 4096);
        let short = Body {
            line_height: 10,
            ..BODY
        };
        assert_eq!(place(2, b"", &short, &mut flat).scale, 2560);
        // A letter is never shrunk: capitals 14 tall from a font whose 'H' is 20 stay 1 times.
        assert_eq!(place(1, b"", &BODY, &mut glyphs).scale, 256);
        let mut tall = Glyphs {
            body_h: Some(14),
            cap_h: Some(20),
        };
        assert_eq!(place(1, b"", &BODY, &mut tall).scale, 256);
    }

    #[test]
    fn takes_at_most_half_the_width() {
        let mut glyphs = Glyphs {
            body_h: Some(14),
            cap_h: Some(20),
        };
        let narrow = Body { width: 61, ..BODY };
        assert_eq!(place(3, b"WWW", &narrow, &mut glyphs).width, 30);
    }
}
