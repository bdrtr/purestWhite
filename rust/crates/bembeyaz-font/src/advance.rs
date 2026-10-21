//! The per-style advance table `SdCardFont` keeps for layout: codepoints and their advances,
//! sorted by codepoint and capped, filled from the text being laid out (`test/sd_card_font` pins
//! it).

use crate::intervals::Table;
use crate::page::{Mapping, REPLACEMENT, seen_before};

/// A codepoint and its advance (12.4 fixed point). `bb_cpfont_advance` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Advance {
    pub codepoint: u32,
    pub advance: u16,
}

/// The advance of `codepoint` in the sorted `table`.
#[must_use]
#[inline]
pub fn find(table: &[Advance], codepoint: u32) -> Option<u16> {
    let (mut low, mut high) = (0, table.len());
    while low < high {
        let mid = low + (high - low) / 2;
        if table.get(mid).is_some_and(|e| e.codepoint < codepoint) {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    table
        .get(low)
        .filter(|e| e.codepoint == codepoint)
        .map(|e| e.advance)
}

/// Merges the sorted `new` entries into the first `size` entries of `table`, keeping it sorted
/// and at most `limit` long: the largest codepoints past the limit are dropped. `table` holds
/// room for the result. Returns the new size.
pub fn merge(table: &mut [Advance], size: usize, new: &[Advance], limit: usize) -> usize {
    if new.is_empty() || size >= limit {
        return size;
    }
    let total = size + new.len();
    let merged = total.min(limit);
    let mut drop = total - merged;
    // From the back, so existing entries move at most once and are never overwritten before
    // they are read.
    let (mut i, mut j, mut k) = (size, new.len(), total);
    while j > 0 {
        let newest = new.get(j - 1).copied().unwrap_or_default();
        let take_new = i == 0
            || table
                .get(i - 1)
                .is_none_or(|old| newest.codepoint >= old.codepoint);
        let entry = if take_new {
            j -= 1;
            newest
        } else {
            i -= 1;
            table.get(i).copied().unwrap_or_default()
        };
        k -= 1;
        if drop > 0 {
            drop -= 1;
        } else if let Some(slot) = table.get_mut(k) {
            *slot = entry;
        }
    }
    merged
}

/// Maps each of `codepoints` that `cached` lacks to its glyph index in `intervals`, or to U+FFFD's
/// when the font lacks it, into `out` (one entry per codepoint at most). Returns how many it
/// mapped, and how many it could not (the font has neither the codepoint nor U+FFFD).
pub fn plan(
    cached: &[Advance],
    intervals: &Table,
    codepoints: &[u32],
    out: &mut [Mapping],
) -> (usize, u32) {
    let replacement = intervals.find(REPLACEMENT);
    let mut count = 0;
    let mut missed: u32 = 0;
    for &codepoint in codepoints {
        if find(cached, codepoint).is_some() {
            continue;
        }
        let Some(index) = intervals
            .find(codepoint)
            .or(replacement)
            .and_then(|i| i32::try_from(i).ok())
        else {
            missed += 1;
            continue;
        };
        if let Some(slot) = out.get_mut(count) {
            *slot = Mapping { codepoint, index };
            count += 1;
        }
    }
    (count, missed)
}

/// Gathers the unique codepoints of the words in `segments` (each a run of NUL-terminated words;
/// a word ends at the segment's end at the latest) and of `extra` into `out` in the order they
/// first appear, each followed by its small-caps uppercase when `small_caps`, up to `max`; then
/// the `append` codepoints that are missing, past `max` if need be (`out` holds `max +
/// append.len()`). Returns how many there are and whether `max` cut the words short.
pub fn gather_words<'a>(
    segments: impl IntoIterator<Item = &'a [u8]>,
    extra: Option<&[u8]>,
    max: usize,
    small_caps: bool,
    append: &[u32],
    out: &mut [u32],
) -> (usize, bool) {
    let mut count = 0;
    // Adds a codepoint not seen yet; true when there is no room for it.
    let add = |out: &mut [u32], count: &mut usize, cp: u32| {
        if out.get(..*count).is_some_and(|seen| seen_before(seen, cp)) {
            return false;
        }
        if *count >= max {
            return true;
        }
        match out.get_mut(*count) {
            Some(slot) => {
                *slot = cp;
                *count += 1;
                false
            }
            None => true,
        }
    };
    let collect = |out: &mut [u32], count: &mut usize, text: &[u8]| {
        for cp in bembeyaz_utf8::codepoints(text) {
            if add(out, count, cp)
                || (small_caps && add(out, count, bembeyaz_utf8::small_caps_upper(cp)))
            {
                return true;
            }
        }
        false
    };
    let mut cut = false;
    'segments: for segment in segments {
        for word in segment.split(|&b| b == 0) {
            if collect(out, &mut count, word) {
                cut = true;
                break 'segments;
            }
        }
    }
    if !cut && let Some(text) = extra {
        cut = collect(out, &mut count, text);
    }
    for &cp in append {
        if !out.get(..count).is_some_and(|seen| seen_before(seen, cp))
            && let Some(slot) = out.get_mut(count)
        {
            *slot = cp;
            count += 1;
        }
    }
    (count, cut)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{Advance, find, gather_words, merge, plan};
    use crate::intervals::Table;
    use crate::page::Mapping;
    use std::vec;
    use std::vec::Vec;

    fn entries(cps: &[u32]) -> Vec<Advance> {
        cps.iter()
            .map(|&codepoint| Advance {
                codepoint,
                advance: (codepoint as u16).wrapping_mul(2),
            })
            .collect()
    }

    #[test]
    fn finds_and_merges_with_a_cap() {
        let mut table = entries(&[10, 30, 50]);
        table.resize(5, Advance::default());
        assert_eq!(
            (find(&table[..3], 30), find(&table[..3], 31)),
            (Some(60), None)
        );
        let size = merge(&mut table, 3, &entries(&[20, 40, 60]), 5);
        assert_eq!(size, 5);
        assert_eq!(
            table.iter().map(|e| e.codepoint).collect::<Vec<_>>(),
            [10, 20, 30, 40, 50]
        );
        assert_eq!(merge(&mut table, 5, &entries(&[1]), 5), 5);
        assert_eq!(merge(&mut table, 2, &[], 5), 2);
    }

    #[test]
    fn plans_uncached_codepoints_with_the_replacement() {
        // 'A'..'C' at glyph 0, U+FFFD at glyph 3.
        let bytes: Vec<u8> = [[0x41u16, 0x43, 0], [0xFFFD, 0xFFFD, 3]]
            .iter()
            .flatten()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let table = Table {
            entries: &bytes,
            compact: true,
        };
        let mut out = [Mapping::default(); 4];
        let (n, missed) = plan(&entries(&[0x42]), &table, &[0x41, 0x42, 0x61], &mut out);
        assert_eq!((n, missed), (2, 0));
        assert_eq!(
            &out[..2],
            &[
                Mapping {
                    codepoint: 0x41,
                    index: 0
                },
                Mapping {
                    codepoint: 0x61,
                    index: 3
                }
            ]
        );
        let no_replacement = Table {
            entries: &bytes[..6],
            compact: true,
        };
        assert_eq!(
            plan(&[], &no_replacement, &[0x41, 0x61, 0x62], &mut out),
            (1, 2)
        );
    }

    #[test]
    fn gathers_words_with_small_caps_space_and_hyphen() {
        let mut out = [0u32; 8];
        let (n, cut) = gather_words(
            [&b"ab\0a\0"[..]],
            Some(&b"z"[..]),
            6,
            true,
            &[0x20, 0x2D],
            &mut out,
        );
        assert!(!cut);
        assert_eq!(&out[..n], &[0x61, 0x41, 0x62, 0x42, 0x7A, 0x5A, 0x20, 0x2D]);
        let (n, cut) = gather_words(
            [&b"abc\0"[..]],
            Some(&b"d"[..]),
            2,
            false,
            &[0x2D],
            &mut out,
        );
        assert!(cut);
        assert_eq!(&out[..n], &[0x61, 0x62, 0x2D]);
    }

    #[test]
    fn any_input_never_panics() {
        let mut seed = 0xdead_beef_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..500 {
            let segment: Vec<u8> = (0..next() % 300).map(|_| next() as u8).collect();
            let max = (next() % 50) as usize;
            let mut out = vec![0u32; max + 2];
            let (n, _) = gather_words(
                [&segment[..]],
                Some(&segment[..next() as usize % (segment.len() + 1)]),
                max,
                next() % 2 == 0,
                &[0x20, 0x2D],
                &mut out,
            );
            let mut sorted: Vec<Advance> = entries(&out[..n]);
            sorted.sort_unstable_by_key(|e| e.codepoint);
            sorted.dedup_by_key(|e| e.codepoint);
            let mut table = entries(&[]);
            table.resize((next() % 40) as usize, Advance::default());
            let size = (next() as usize) % (table.len() + 1);
            let room = table.len();
            let _ = merge(&mut table, size.min(room), &sorted, room);
            let _ = find(&table, next());
        }
    }
}
