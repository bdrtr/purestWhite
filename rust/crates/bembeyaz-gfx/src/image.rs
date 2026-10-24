//! Pictures: BMP rows (two bits per pixel, 0 black to 3 white, as lib/GfxRenderer's `Bitmap`
//! hands them out) placed, cropped and shrunk onto the canvas; 1-bit icons; and the inversion
//! that keeps a picture's polarity on a dark-mode screen.

use super::{Canvas, Ink};

/// Which plane a draw writes: black and white, or one bit of the two-bit gray levels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    Bw,
    GrayLsb,
    GrayMsb,
}

/// A picture read row by row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picture {
    pub width: i32,
    pub height: i32,
    /// Rows come top first; otherwise bottom first.
    pub top_down: bool,
    /// Drawn the way 1-bit files are: black where not white in every plane, never cropped, and
    /// rows below the screen read on rather than ending the picture.
    pub one_bit: bool,
}

/// Where and how a picture goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The top left corner.
    pub x: i32,
    pub y: i32,
    /// The box the picture shrinks to fit (never grows); 0 or less for no bound.
    pub max_width: i32,
    pub max_height: i32,
    /// The share of the width and height cut off, half on each side.
    pub crop_x: f32,
    pub crop_y: f32,
    /// Leave white pixels as they are.
    pub white_transparent: bool,
    pub plane: Plane,
    /// Gray planes hold absolute levels (both inks written) rather than differences from black
    /// and white.
    pub absolute_gray: bool,
}

/// What a picture draw did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drawn {
    /// Pixels cropped off each side, horizontally and vertically.
    pub crop: (i32, i32),
    /// The scale, 1 when not shrunk.
    pub scale: f32,
    /// The size drawn.
    pub width: i32,
    pub height: i32,
}

/// `v` rounded down, saturating at the ends of i32 (and 0 for NaN).
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn floor(v: f32) -> i32 {
    let t = v as i32;
    if (t as f32) > v { t - 1 } else { t }
}

/// `v` times `scale`, rounded down, when shrinking.
#[allow(clippy::cast_precision_loss)]
fn scaled(v: i32, scale: Option<f32>) -> i32 {
    scale.map_or(v, |s| floor(v as f32 * s))
}

/// The scale that fits the cropped picture into the placement's box, when it has to shrink.
#[allow(clippy::cast_precision_loss)]
fn fit(picture: &Picture, place: &Placement) -> Option<f32> {
    let cropped_width = (1.0 - place.crop_x) * picture.width as f32;
    let cropped_height = (1.0 - place.crop_y) * picture.height as f32;
    let mut fit = None::<f32>;
    if place.max_width > 0 && cropped_width > 0.0 {
        fit = Some(place.max_width as f32 / cropped_width);
    }
    if place.max_height > 0 && cropped_height > 0.0 {
        let by_height = place.max_height as f32 / cropped_height;
        fit = Some(fit.map_or(by_height, |f| f.min(by_height)));
    }
    fit.filter(|&f| f < 1.0)
}

/// The ink a gray level (0 black to 3 white) gets in `plane`: black, white, or none.
fn ink(level: u8, place: &Placement) -> Option<bool> {
    if place.white_transparent && level == 3 {
        return None;
    }
    let ink = Ink {
        plane: place.plane,
        absolute: place.absolute_gray,
    };
    ink.of(level)
}

impl Canvas<'_> {
    /// Draws `picture`, reading each row into `row` with `read` (false when the row could not be
    /// read), from the first row of the file to the last one needed. Rows and columns that fall
    /// off the screen are skipped. Returns what was drawn, or the row that could not be read.
    ///
    /// # Errors
    /// The index of the row `read` failed on; the rows before it are drawn.
    #[allow(clippy::cast_precision_loss)]
    pub fn picture(
        &mut self,
        picture: &Picture,
        place: &Placement,
        row: &mut [u8],
        mut read: impl FnMut(&mut [u8]) -> bool,
    ) -> Result<Drawn, i32> {
        let one_bit = picture.one_bit;
        // 1-bit pictures are never cropped, and black wherever not white in every plane.
        let place = &if one_bit {
            Placement {
                crop_x: 0.0,
                crop_y: 0.0,
                plane: Plane::Bw,
                ..*place
            }
        } else {
            *place
        };
        let (width, height) = (picture.width, picture.height);
        let crop_x = floor(width as f32 * place.crop_x / 2.0);
        let crop_y = floor(height as f32 * place.crop_y / 2.0);
        let scale = fit(picture, place);
        let screen_h = self.screen_size().1;
        let mut file_row = 0;
        while file_row < height.wrapping_sub(crop_y) {
            let from_top = if picture.top_down {
                file_row
            } else {
                height.wrapping_sub(1).wrapping_sub(file_row)
            };
            let y = scaled(from_top.wrapping_sub(crop_y), scale).wrapping_add(place.y);
            // Below the screen, a top-down picture's later rows only go further down: the gray
            // path stops there (the 1-bit path reads on, as the old renderer did). A bottom-up
            // picture's later rows come back up onto the screen, so it reads on too.
            if y >= screen_h && !one_bit && picture.top_down {
                break;
            }
            if !read(row) {
                return Err(file_row);
            }
            if (0..screen_h).contains(&y) && file_row >= crop_y {
                let end = width.wrapping_sub(crop_x);
                self.picture_row(row, crop_x, end, y, place, scale);
            }
            file_row += 1;
        }
        Ok(Drawn {
            crop: (crop_x, crop_y),
            scale: scale.unwrap_or(1.0),
            width: rendered(width.wrapping_sub(crop_x.wrapping_mul(2)), scale),
            height: rendered(height.wrapping_sub(crop_y.wrapping_mul(2)), scale),
        })
    }

    /// Columns `first..end` of a picture row onto screen row `y`, column `first` at `place.x`.
    fn picture_row(
        &mut self,
        row: &[u8],
        first: i32,
        end: i32,
        y: i32,
        place: &Placement,
        scale: Option<f32>,
    ) {
        let screen_w = self.screen_size().0;
        let mut column = first;
        while column < end {
            let x = scaled(column.wrapping_sub(first), scale).wrapping_add(place.x);
            if x >= screen_w {
                break;
            }
            if x >= 0 {
                // A negative column (a negative crop) reads as white.
                #[allow(clippy::cast_sign_loss)]
                let byte = row.get(column as usize / 4).copied().unwrap_or(0xFF);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let level = (byte >> (6 - (column as u32 % 4) * 2)) & 3;
                if let Some(black) = ink(level, place) {
                    self.pixel(x, y, black);
                }
            }
            column += 1;
        }
    }

    /// A square 1-bit icon of `size` pixels (rows of `(size + 7) / 8` bytes, most significant bit
    /// first, 0 for ink), its rows running right to left and its columns down from (`x`, `y`)
    /// (the portrait layout the UI themes expect); only the ink is drawn. Returns how many pixels
    /// were off the panel.
    pub fn icon(&mut self, bits: &[u8], x: i32, y: i32, size: i32) -> u32 {
        let row_bytes = size.wrapping_add(7) / 8;
        let mut off = 0u32;
        for row in 0..size.max(0) {
            for column in 0..size {
                #[allow(clippy::cast_sign_loss)]
                let at = row.wrapping_mul(row_bytes).wrapping_add(column / 8) as usize;
                let byte = bits.get(at).copied().unwrap_or(0xFF);
                let (px, py) = (x.wrapping_add(size - 1 - row), y.wrapping_add(column));
                if byte & (0x80 >> (column & 7)) == 0 && !self.pixel(px, py, true) {
                    off = off.wrapping_add(1);
                }
            }
        }
        off
    }

    /// Inverts every pixel of a logical rectangle, clipped to the screen, the clip and the strip.
    pub fn invert_rect(&mut self, x: i32, y: i32, w: i32, h: i32) {
        if w <= 0 || h <= 0 {
            return;
        }
        let clip = self.clip;
        let (screen_w, screen_h) = self.screen_size();
        let lx0 = x.max(clip.left).max(0);
        let ly0 = y.max(clip.top).max(0);
        let lx1 = x.wrapping_add(w).min(clip.right).min(screen_w);
        let ly1 = y.wrapping_add(h).min(clip.bottom).min(screen_h);
        if lx0 >= lx1 || ly0 >= ly1 {
            return;
        }
        let (ax, ay) = self.to_panel(lx0, ly0);
        let (bx, by) = self.to_panel(lx1 - 1, ly1 - 1);
        let (left, right) = (ax.min(bx), ax.max(bx));
        let top = ay.min(by).max(self.origin_y);
        let bottom = ay
            .max(by)
            .min(self.origin_y.wrapping_add(self.rows).wrapping_sub(1));
        for py in top..=bottom {
            #[allow(clippy::cast_sign_loss)]
            let start = (py - self.origin_y) as usize * self.stride;
            let mut px = left;
            while px <= right {
                // Whole bytes at once.
                let (mask, step) = if px.trailing_zeros() >= 3 && px + 7 <= right {
                    (0xFF, 8)
                } else {
                    (0x80u8 >> (px & 7), 1)
                };
                #[allow(clippy::cast_sign_loss)]
                if let Some(byte) = self.buf.get_mut(start + (px as usize >> 3)) {
                    *byte ^= mask;
                }
                px += step;
            }
        }
    }
}

/// The size `source` pixels come to when shrunk by `scale`: the last one's position plus one.
fn rendered(source: i32, scale: Option<f32>) -> i32 {
    match scale {
        Some(_) => scaled(source.wrapping_sub(1), scale).wrapping_add(1),
        None => source,
    }
}
