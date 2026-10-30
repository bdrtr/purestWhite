//! Frames as BMP files, as the battery measurement's pages are kept on the card: upright 480 by
//! 800, a bit a pixel (black and white palette), rows bottom-up as most writers put them. The C++
//! firmware's image viewer and the page loop (`purestwhite_services::pages`) read them.

use purestwhite_render::{HEIGHT, WIDTH, is_black};
use std::io::Write;

/// Bytes in a row: 480 bits, a multiple of 4 bytes already.
#[allow(clippy::cast_sign_loss, reason = "the screen's width")]
const ROW_BYTES: usize = WIDTH as usize / 8;
#[allow(clippy::cast_sign_loss, reason = "the screen's height")]
const ROW_COUNT: usize = HEIGHT as usize;
/// The file header, the 40-byte DIB header and the two palette entries.
const PIXELS_AT: u32 = 14 + 40 + 8;

/// Writes `frame` (a panel frame, `purestwhite_render::FRAME_BYTES` long) as a BMP.
///
/// # Errors
/// `out` fails.
pub fn write_bmp(frame: &[u8], mut out: impl Write) -> std::io::Result<()> {
    out.write_all(&header())?;
    for y in (0..HEIGHT).rev() {
        let mut row = [0u8; ROW_BYTES];
        for (x, byte) in (0..WIDTH).zip((0..).map(|i: usize| i / 8)) {
            if !is_black(frame, x, y)
                && let Some(byte) = row.get_mut(byte)
            {
                // Palette entry 1, white.
                *byte |= 0x80 >> (x % 8);
            }
        }
        out.write_all(&row)?;
    }
    Ok(())
}

/// The headers and the palette: black, then white.
fn header() -> Vec<u8> {
    #[allow(clippy::cast_possible_truncation, reason = "48,062 bytes")]
    let size = PIXELS_AT + (ROW_BYTES * ROW_COUNT) as u32;
    let mut h = Vec::with_capacity(PIXELS_AT as usize);
    h.extend_from_slice(b"BM");
    h.extend_from_slice(&size.to_le_bytes());
    h.extend_from_slice(&[0; 4]);
    h.extend_from_slice(&PIXELS_AT.to_le_bytes());
    h.extend_from_slice(&40u32.to_le_bytes());
    h.extend_from_slice(&WIDTH.to_le_bytes());
    h.extend_from_slice(&HEIGHT.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    // No compression, the pixels' size, 72 dpi both ways, two colors, both important.
    h.extend_from_slice(&0u32.to_le_bytes());
    h.extend_from_slice(&(size - PIXELS_AT).to_le_bytes());
    h.extend_from_slice(&2835u32.to_le_bytes());
    h.extend_from_slice(&2835u32.to_le_bytes());
    h.extend_from_slice(&2u32.to_le_bytes());
    h.extend_from_slice(&2u32.to_le_bytes());
    h.extend_from_slice(&[0, 0, 0, 0, 255, 255, 255, 0]);
    h
}
