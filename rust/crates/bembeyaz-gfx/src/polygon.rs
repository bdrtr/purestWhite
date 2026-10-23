//! Filled polygons, one scanline at a time: with no buffer for the edge crossings, each row finds
//! them in order by going over the edges again for every crossing, so the work per row grows with
//! the square of the point count (callers draw triangles and ribbons of a few points).

use super::{Canvas, Fill};

/// Where the edges of the polygon with corners (`xs[i]`, `ys[i]`) cross row `y`, smallest first,
/// equal crossings repeated.
struct Crossings<'a> {
    xs: &'a [i32],
    ys: &'a [i32],
    y: i32,
    /// The last crossing given, and how many times.
    last: Option<(i32, usize)>,
}

impl Crossings<'_> {
    /// Every crossing, in edge order: an edge crosses the row when one end is above it and the
    /// other on it or below.
    fn unordered(&self) -> impl Iterator<Item = i32> + '_ {
        let y = i64::from(self.y);
        let n = self.xs.len().min(self.ys.len());
        (0..n).filter_map(move |i| {
            let j = if i == 0 { n - 1 } else { i - 1 };
            let (xi, yi) = (i64::from(*self.xs.get(i)?), i64::from(*self.ys.get(i)?));
            let (xj, yj) = (i64::from(*self.xs.get(j)?), i64::from(*self.ys.get(j)?));
            if (yi < y && yj >= y) || (yj < y && yi >= y) {
                // Rounded toward zero; the ends differ in y, so this never divides by zero. Only
                // coordinates near the ends of i32 can wrap.
                let dx = (y - yi).wrapping_mul(xj - xi).wrapping_div(yj - yi);
                #[allow(clippy::cast_possible_truncation)]
                Some(xi.wrapping_add(dx) as i32)
            } else {
                None
            }
        })
    }
}

impl Iterator for Crossings<'_> {
    type Item = i32;

    /// The last crossing again while more crossings equal it than it was given, else the smallest
    /// crossing above it.
    fn next(&mut self) -> Option<i32> {
        let mut same = 0;
        let mut above = None::<i32>;
        for x in self.unordered() {
            match self.last {
                Some((last, _)) if x == last => same += 1,
                Some((last, _)) if x < last => {}
                _ => above = Some(above.map_or(x, |a| a.min(x))),
            }
        }
        if let Some((last, given)) = self.last.as_mut()
            && same > *given
        {
            *given += 1;
            return Some(*last);
        }
        let next = above?;
        self.last = Some((next, 1));
        Some(next)
    }
}

impl Canvas<'_> {
    /// Fills the polygon with corners (`xs[i]`, `ys[i]`) (at least three): on each screen row,
    /// between the first and second edge crossing, the third and fourth, and so on.
    pub fn fill_polygon(&mut self, xs: &[i32], ys: &[i32], fill: Fill) {
        let n = xs.len().min(ys.len());
        let (Some(xs), Some(ys)) = (xs.get(..n), ys.get(..n)) else {
            return;
        };
        if n < 3 {
            return;
        }
        let (screen_w, screen_h) = self.screen_size();
        let top = ys.iter().copied().min().unwrap_or(0).max(0);
        let bottom = ys
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .min(screen_h.wrapping_sub(1));
        for y in top..=bottom {
            let mut crossings = Crossings {
                xs,
                ys,
                y,
                last: None,
            };
            while let Some(start) = crossings.next() {
                let Some(end) = crossings.next() else { break };
                let (start, end) = (start.max(0), end.min(screen_w.wrapping_sub(1)));
                if start <= end {
                    self.fill_rect(start, y, end - start + 1, 1, fill);
                }
            }
        }
    }
}
