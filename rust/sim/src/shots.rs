//! Shots as PNG files: upright as the device is held (480 by 800), black and white, named by
//! their order and refresh.

use crate::Shot;
use purestwhite_app::Refresh;
use purestwhite_render::{HEIGHT, WIDTH, is_black};
use std::io::Write;
use std::path::Path;

/// The file name of the shot at `index` (from 0): `0001-full.png`, `0002-fast.png`, ...
#[must_use]
pub fn shot_name(index: usize, refresh: Refresh) -> String {
    let refresh = match refresh {
        Refresh::Full => "full",
        Refresh::Half => "half",
        Refresh::Fast => "fast",
    };
    format!("{:04}-{refresh}.png", index + 1)
}

/// Removes the files in `dir` named as shots (an earlier run's), and nothing else; how many.
///
/// # Errors
/// `dir` cannot be read, or a shot cannot be removed.
pub fn clear_shots(dir: &Path) -> std::io::Result<usize> {
    let mut removed = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_str().is_some_and(is_shot_name) {
            std::fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn is_shot_name(name: &str) -> bool {
    let Some((number, rest)) = name.split_once('-') else {
        return false;
    };
    number.len() == 4
        && number.bytes().all(|b| b.is_ascii_digit())
        && ["full.png", "half.png", "fast.png"].contains(&rest)
}

/// Writes `shot` as an 8-bit grayscale PNG to `out`.
///
/// # Errors
/// The PNG cannot be written.
#[allow(
    clippy::cast_sign_loss,
    reason = "the screen's size is positive and fits a u32"
)]
pub fn write_png(shot: &Shot, out: impl Write) -> Result<(), png::EncodingError> {
    let mut encoder = png::Encoder::new(out, WIDTH as u32, HEIGHT as u32);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&upright(&shot.frame))?;
    writer.finish()
}

/// The frame's pixels upright, a byte each (0 black, 255 white), row by row.
fn upright(frame: &[u8]) -> Vec<u8> {
    (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| if is_black(frame, x, y) { 0 } else { 255 }))
        .collect()
}
