//! Cover rows scaled to the output width before dithering (lib's JPEG and PNG to BMP converters):
//! area averages summed over the source rows each output row covers, and linear interpolation for
//! JPEGs grown from their 1/8 progressive scan. Positions are 16.16 fixed point, as the converters'.

use super::Ditherer;

const ONE: u32 = 1 << 16;

/// Adds source `row` to the area sums of an output row `sums.len()` wide: output column `x` covers
/// source columns `x * scale_x >> 16` up to `(x + 1) * scale_x >> 16` (products wrapping at 32
/// bits), or at least the first of them, kept inside the row.
pub fn area_add(row: &[u8], scale_x: u32, sums: &mut [u32], counts: &mut [u32]) {
    let width = row.len();
    for (x, (sum, count)) in (0u32..).zip(sums.iter_mut().zip(counts.iter_mut())) {
        let start = (x.wrapping_mul(scale_x) >> 16) as usize;
        let end = (x.wrapping_add(1).wrapping_mul(scale_x) >> 16) as usize;
        let (mut s, mut n) = (0u32, 0u32);
        if let Some(covered) = row.get(start..end.min(width)) {
            for &v in covered {
                s += u32::from(v);
            }
            n = u32::try_from(covered.len()).unwrap_or(u32::MAX);
        }
        if n == 0
            && let Some(&v) = row.get(start)
        {
            (s, n) = (u32::from(v), 1);
        }
        *sum = sum.wrapping_add(s);
        *count = count.wrapping_add(n);
    }
}

/// Dithers the area averages (`sums` over `counts`, 0 where nothing was counted) as output row `y`
/// into `out`, 64 pixels at a time.
pub fn area_pixels(d: &mut Ditherer, sums: &[u32], counts: &[u32], y: i32, out: &mut [u8]) {
    let mut gray = [0u8; 64];
    for (piece, (sums, counts)) in sums.chunks(64).zip(counts.chunks(64)).enumerate() {
        for (g, (&s, &n)) in gray.iter_mut().zip(sums.iter().zip(counts)) {
            #[allow(clippy::cast_possible_truncation, reason = "the old byte store")]
            let average = s.checked_div(n).unwrap_or(0) as u8;
            *g = average;
        }
        let len = sums.len();
        d.pixels(gray.get(..len).unwrap_or_default(), piece * 64, y, out);
    }
}

/// Where output position `index` of `out_size` samples source positions of `src_size`, 16.16:
/// `index * step`, the last one exactly at the source's last. `JpegToBmpConverter`'s
/// `interpolatedSourceFp` samples the rows the same way: change both.
fn source(index: usize, out_size: usize, src_size: usize, step: u32) -> u32 {
    if src_size <= 1 || out_size <= 1 {
        0
    } else if index >= out_size - 1 {
        #[allow(clippy::cast_possible_truncation, reason = "as the converter's")]
        let last = ((src_size - 1) as u32) << 16;
        last
    } else {
        #[allow(clippy::cast_possible_truncation, reason = "as the converter's")]
        let at = (index as u32).wrapping_mul(step);
        at
    }
}

/// Writes `row` linearly resampled to `out.len()` pixels into `out`, a step of `step` (16.16)
/// apart and the last pixel on the row's last.
pub fn linear_row(row: &[u8], step: u32, out: &mut [u8]) {
    let (width, out_width) = (row.len(), out.len());
    for (x, o) in out.iter_mut().enumerate() {
        let at = source(x, out_width, width, step);
        let x0 = (at >> 16) as usize;
        let x1 = if x0 + 1 < width { x0 + 1 } else { x0 };
        let fx = at & (ONE - 1);
        let (a, b) = (
            u32::from(row.get(x0).copied().unwrap_or(0)),
            u32::from(row.get(x1).copied().unwrap_or(0)),
        );
        #[allow(clippy::cast_possible_truncation, reason = "a mix of two bytes")]
        let mixed = ((a * (ONE - fx) + b * fx) >> 16) as u8;
        *o = mixed;
    }
}

/// Writes rows `a` and `b` mixed `fy` (16.16) of the way to `b` into `out`.
pub fn blend_rows(a: &[u8], b: &[u8], fy: u32, out: &mut [u8]) {
    let fy = fy & (ONE - 1);
    for (o, (&a, &b)) in out.iter_mut().zip(a.iter().zip(b)) {
        #[allow(clippy::cast_possible_truncation, reason = "a mix of two bytes")]
        let mixed = ((u32::from(a) * (ONE - fy) + u32::from(b) * fy) >> 16) as u8;
        *o = mixed;
    }
}
