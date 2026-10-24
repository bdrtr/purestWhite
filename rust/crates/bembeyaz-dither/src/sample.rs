//! Images drawn as they decode (lib/Epub's framebuffer converters): JPEG blocks of gray resampled to
//! the output size in 16.16 fixed point, or PNG lines stepped through column by column, each pixel
//! turned into one of the panel's four levels by its screen position (ordered dithering) or by plain
//! thresholds, and written as packed 2-bit rows (four levels to a byte, most significant first: the
//! chapter image cache's format).

/// 1.0 in 16.16 fixed point.
const ONE: i32 = 1 << 16;
const FRACTION: i32 = ONE - 1;

/// How a gray (0 black to 255 white) becomes a level (0 black to 3 white).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantize {
    /// Ordered dithering with a 4×4 Bayer matrix at the pixel's screen position.
    Bayer,
    /// A third of the range each: gray / 85.
    Thirds,
    /// The top two bits.
    Quarters,
}

const BAYER: [[i32; 4]; 4] = [[0, 8, 2, 10], [12, 4, 14, 6], [3, 11, 1, 9], [15, 7, 13, 5]];

/// Gray pixels from the decoder at (`x`, `y`) of the source: `height` rows `stride` bytes apart, the
/// first `width` of each valid.
#[derive(Clone, Copy, Debug)]
pub struct GrayBlock<'a> {
    pub pixels: &'a [u8],
    pub stride: usize,
    pub width: i32,
    pub height: i32,
    pub x: i32,
    pub y: i32,
}

/// Output size over source size on each axis, and source over output, 16.16 fixed point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scale {
    pub x: i32,
    pub inv_x: i32,
    pub y: i32,
    pub inv_y: i32,
}

impl GrayBlock<'_> {
    /// Row `y` of the block (from its top), or nothing.
    fn row(&self, y: i32) -> &[u8] {
        usize::try_from(y)
            .ok()
            .and_then(|y| self.pixels.get(y.wrapping_mul(self.stride)..))
            .unwrap_or(&[])
    }

    /// Source row or column `at` (16.16) as an index into the block, kept in `0..len`.
    fn index(at: i32, origin: i32, len: i32) -> i32 {
        ((at >> 16) - origin).max(0).min(len - 1)
    }
}

/// Pixel `x` of `row`, or 0.
#[allow(clippy::inline_always)]
#[inline(always)]
fn pixel(row: &[u8], x: i32) -> i32 {
    usize::try_from(x)
        .ok()
        .and_then(|x| row.get(x))
        .map_or(0, |&g| i32::from(g))
}

/// Writes the levels of output row `dst_y`'s columns `start..end` from `block` into `out`, packed
/// from its first bit (as many as fit; the last byte's spare bits are 0). The output's top left is
/// at screen (`ox`, `oy`), which places the dither pattern. Growing on both axes interpolates
/// bilinearly, 1:1 copies, and anything else takes the nearest pixel; source positions are kept in
/// the block (but for 1:1, which reads the row as it is, up to its stride).
#[allow(clippy::too_many_arguments)]
pub fn block_row(
    block: &GrayBlock,
    scale: &Scale,
    dst_y: i32,
    (start, end): (i32, i32),
    (ox, oy): (i32, i32),
    quantize: Quantize,
    out: &mut [u8],
) {
    if block.width <= 0 || block.height <= 0 || start >= end {
        return;
    }
    let (w, h) = (block.width, block.height);
    if scale.x == ONE && scale.y == ONE {
        let row = block.row(dst_y - block.y);
        let x0 = start - block.x;
        emit(out, start, end, (ox, oy + dst_y), quantize, |i| {
            pixel(row, x0.wrapping_add(i))
        });
    } else if scale.x > ONE && scale.y > ONE {
        let sy = dst_y.wrapping_mul(scale.inv_y);
        let fy = sy & FRACTION;
        let raw = (sy >> 16) - block.y;
        let row0 = block.row(raw.max(0).min(h - 1));
        let row1 = block.row((raw + 1).max(0).min(h - 1));
        emit(out, start, end, (ox, oy + dst_y), quantize, |i| {
            let sx = start.wrapping_add(i).wrapping_mul(scale.inv_x);
            let fx = sx & FRACTION;
            let raw = (sx >> 16) - block.x;
            let (x0, x1) = (raw.max(0).min(w - 1), (raw + 1).max(0).min(w - 1));
            let top = (pixel(row0, x0) * (ONE - fx) + pixel(row0, x1) * fx) >> 16;
            let bottom = (pixel(row1, x0) * (ONE - fx) + pixel(row1, x1) * fx) >> 16;
            ((top * (ONE - fy) + bottom * fy) >> 16) & 0xFF
        });
    } else {
        let row = block.row(GrayBlock::index(
            dst_y.wrapping_mul(scale.inv_y),
            block.y,
            h,
        ));
        emit(out, start, end, (ox, oy + dst_y), quantize, |i| {
            let sx = start.wrapping_add(i).wrapping_mul(scale.inv_x);
            pixel(row, GrayBlock::index(sx, block.x, w))
        });
    }
}

/// A source line of gray values (and alpha, for images drawn over what is on the screen) stepped
/// through for an output `dst_width` columns wide: column `c` takes pixel `crop + c * visible /
/// dst_width`, rounded down.
#[derive(Clone, Copy, Debug)]
pub struct Line<'a> {
    pub gray: &'a [u8],
    pub alpha: Option<&'a [u8]>,
    pub crop: i32,
    pub visible: i32,
    pub dst_width: i32,
}

/// Where each output column of a [`Line`] reads, from column `start` on.
struct Steps {
    src: i32,
    error: i32,
    whole: i32,
    rest: i32,
    dst: i32,
}

impl Steps {
    fn new(line: &Line, start: i32) -> Self {
        let dst = line.dst_width.max(1);
        let at = i64::from(start) * i64::from(line.visible);
        // Both fit: the quotient is below `visible` times `start` over `dst`, the remainder below `dst`.
        #[allow(clippy::cast_possible_truncation)]
        let (q, r) = (
            (at.div_euclid(i64::from(dst))) as i32,
            (at.rem_euclid(i64::from(dst))) as i32,
        );
        Steps {
            src: line.crop.wrapping_add(q),
            error: r,
            whole: line.visible.div_euclid(dst),
            rest: line.visible.rem_euclid(dst),
            dst,
        }
    }

    /// The source pixel of this column; moves on to the next.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn next(&mut self) -> i32 {
        let src = self.src;
        self.src = self.src.wrapping_add(self.whole);
        self.error += self.rest;
        if self.error >= self.dst {
            self.error -= self.dst;
            self.src = self.src.wrapping_add(1);
        }
        src
    }
}

/// Writes the levels of output row `out_y`'s columns `start..end` from `line` into `out`, packed from
/// its first bit as [`block_row`] does; the output's left edge is at screen column `ox`. With `mask`,
/// each column also gets a bit there (most significant first): set where its pixel is opaque enough
/// to draw against a 4×4 ordered threshold at its screen position, all set without alpha.
pub fn line_row(
    line: &Line,
    (start, end): (i32, i32),
    (ox, out_y): (i32, i32),
    quantize: Quantize,
    out: &mut [u8],
    mask: Option<&mut [u8]>,
) {
    if start >= end || line.dst_width <= 0 {
        return;
    }
    if let Some(mask) = mask {
        let mut steps = Steps::new(line, start);
        #[allow(clippy::cast_sign_loss)]
        let mut thresholds = BAYER
            .get((out_y & 3) as usize)
            .copied()
            .unwrap_or_default()
            .map(|b| b * 16);
        #[allow(clippy::cast_sign_loss)]
        thresholds.rotate_left((ox.wrapping_add(start) & 3) as usize);
        let count = end - start;
        let mut i = 0;
        for byte in mask {
            if i >= count {
                break;
            }
            let mut bits = 0u8;
            for _ in 0..8 {
                let opaque = i < count && {
                    let src = steps.next();
                    let alpha = line.alpha.map_or(255, |a| pixel(a, src));
                    #[allow(clippy::cast_sign_loss)]
                    let threshold = thresholds.get((i & 3) as usize).copied().unwrap_or(0);
                    alpha >= 8 && alpha > threshold
                };
                bits = (bits << 1) | u8::from(opaque);
                i += 1;
            }
            *byte = bits;
        }
    }
    // `emit` asks for the columns in order, once each.
    let mut steps = Steps::new(line, start);
    emit(out, start, end, (ox, out_y), quantize, |_| {
        pixel(line.gray, steps.next())
    });
}

/// Packs the levels of columns `start..end` into `out`, column `start` + `i`'s gray being `gray(i)`
/// (asked for in order, once each).
fn emit(
    out: &mut [u8],
    start: i32,
    end: i32,
    (ox, out_y): (i32, i32),
    quantize: Quantize,
    mut gray: impl FnMut(i32) -> i32,
) {
    // The dither offsets of the row, from column `start` on.
    #[allow(clippy::cast_sign_loss)]
    let mut offsets = BAYER
        .get((out_y & 3) as usize)
        .copied()
        .unwrap_or_default()
        .map(|b| (b - 8) * 5);
    #[allow(clippy::cast_sign_loss)]
    offsets.rotate_left((ox.wrapping_add(start) & 3) as usize);
    match quantize {
        Quantize::Bayer => pack(out, start, end, |i| {
            #[allow(clippy::cast_sign_loss)]
            let d = offsets.get((i & 3) as usize).copied().unwrap_or(0);
            (gray(i) + d).clamp(0, 255) >> 6
        }),
        Quantize::Thirds => pack(out, start, end, |i| (gray(i) / 85).min(3)),
        Quantize::Quarters => pack(out, start, end, |i| gray(i) >> 6),
    }
}

/// Packs levels `level(0..end - start)` into `out`, asking for each in order.
#[allow(clippy::inline_always)]
#[inline(always)]
fn pack(out: &mut [u8], start: i32, end: i32, mut level: impl FnMut(i32) -> i32) {
    let count = end.saturating_sub(start);
    let mut i = 0;
    for byte in out {
        if i >= count {
            break;
        }
        let mut packed = 0;
        for _ in 0..4 {
            packed = (packed << 2) | if i < count { level(i) & 3 } else { 0 };
            i += 1;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let packed = packed as u8;
        *byte = packed;
    }
}

/// Copies `count` packed levels from `src` into the packed row `dst` from its column `at`, leaving
/// out the columns before its start or past its last byte.
pub fn put_levels(dst: &mut [u8], at: i32, src: &[u8], count: usize) {
    for (i, level) in src
        .iter()
        .flat_map(|&b| [b >> 6, b >> 4, b >> 2, b])
        .take(count)
        .enumerate()
    {
        let i = i64::try_from(i).unwrap_or(i64::MAX);
        let Ok(col) = usize::try_from(i64::from(at).saturating_add(i)) else {
            continue;
        };
        #[allow(clippy::cast_possible_truncation)]
        let shift = (6 - (col % 4) * 2) as u32;
        if let Some(byte) = dst.get_mut(col / 4) {
            *byte = (*byte & !(3 << shift)) | ((level & 3) << shift);
        }
    }
}
