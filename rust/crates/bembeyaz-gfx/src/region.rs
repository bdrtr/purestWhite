//! Copies of logical screen rectangles out of and back into the panel's rows, whole bytes at a
//! time, for redrawing part of the screen without rendering it again (a moved highlight, a
//! cached cover). Clipping does not apply: the rectangle is only kept on the panel.

use super::Canvas;

/// A block of whole bytes in the panel's rows: `bytes` bytes from byte `byte_x` of each of `rows`
/// rows from panel row `y`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub byte_x: usize,
    pub y: usize,
    pub bytes: usize,
    pub rows: usize,
}

impl Region {
    /// The bytes a copy of the region takes.
    #[must_use]
    pub fn size(&self) -> usize {
        self.bytes.saturating_mul(self.rows)
    }
}

/// How a rectangle widens to whole bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Widen {
    /// Its panel x range snaps out to multiples of 8 pixels before it is kept on the panel; a
    /// last partial byte the panel's width cuts off is left out (`GfxRenderer`'s
    /// `readFramebufferRegion`, for the panel's 8-pixel window alignment).
    Aligned,
    /// It takes every byte its kept panel x range touches (`copyRegionToBuffer`).
    Touched,
}

impl Canvas<'_> {
    /// The bytes of the panel that logical rectangle (`x`, `y`, `w`, `h`) covers, or none when it
    /// is empty or off the panel.
    #[must_use]
    pub fn region(&self, x: i32, y: i32, w: i32, h: i32, widen: Widen) -> Option<Region> {
        if w <= 0 || h <= 0 {
            return None;
        }
        // Rotation is rigid: two opposite corners bound the panel rectangle.
        let (ax, ay) = self.to_panel(x, y);
        let (bx, by) = self.to_panel(
            x.wrapping_add(w).wrapping_sub(1),
            y.wrapping_add(h).wrapping_sub(1),
        );
        let (pw, ph) = (self.panel_width, self.panel_height);
        let (x0, x1) = (ax.min(bx), ax.max(bx));
        let y0 = ay.min(by).max(0);
        let y1 = ay.max(by).min(ph.saturating_sub(1));
        if y1 < y0 {
            return None;
        }
        let (byte_x, bytes) = match widen {
            Widen::Aligned => {
                // Exclusive bounds, snapped out to whole bytes, then kept on the panel.
                let lo = (x0 & !7).max(0);
                let hi = (x1.saturating_add(8) & !7).min(pw);
                if hi <= lo {
                    return None;
                }
                (lo / 8, (hi - lo) / 8)
            }
            Widen::Touched => {
                let (lo, hi) = (x0.max(0), x1.min(pw.saturating_sub(1)));
                if lo > hi {
                    return None;
                }
                (lo / 8, hi / 8 - lo / 8 + 1)
            }
        };
        // All are at least 0 here.
        #[allow(clippy::cast_sign_loss)]
        Some(Region {
            byte_x: byte_x as usize,
            y: y0 as usize,
            bytes: bytes as usize,
            rows: (y1 - y0 + 1) as usize,
        })
    }

    /// Copies `region`'s rows into `out`, one after another; false, copying nothing, when `out` is
    /// smaller than the region or the region is not all in the canvas's rows.
    #[must_use]
    pub fn read_region(&self, region: &Region, out: &mut [u8]) -> bool {
        let (Some(start), Some(out)) = (self.region_start(region), out.get_mut(..region.size()))
        else {
            return false;
        };
        for (row, dst) in out.chunks_exact_mut(region.bytes.max(1)).enumerate() {
            let at = start + row * self.stride;
            if let Some(src) = self.buf.get(at..at + region.bytes) {
                dst.copy_from_slice(src);
            }
        }
        true
    }

    /// Copies `src` (`region`'s rows one after another) into the region; false, copying nothing,
    /// when `src` is smaller than the region or the region is not all in the canvas's rows.
    #[must_use]
    pub fn write_region(&mut self, region: &Region, src: &[u8]) -> bool {
        let (Some(start), Some(src)) = (self.region_start(region), src.get(..region.size())) else {
            return false;
        };
        for (row, from) in src.chunks_exact(region.bytes.max(1)).enumerate() {
            let at = start + row * self.stride;
            if let Some(dst) = self.buf.get_mut(at..at + region.bytes) {
                dst.copy_from_slice(from);
            }
        }
        true
    }

    /// Where `region`'s first row starts in `buf`, when all of its rows are there.
    fn region_start(&self, region: &Region) -> Option<usize> {
        let row = region.y.checked_sub(usize::try_from(self.origin_y).ok()?)?;
        let start = row.checked_mul(self.stride)?.checked_add(region.byte_x)?;
        let last = region
            .rows
            .checked_sub(1)
            .map_or(Some(0), |r| r.checked_mul(self.stride))?;
        let end = start.checked_add(last)?.checked_add(region.bytes)?;
        (end <= self.buf.len()).then_some(start)
    }
}
