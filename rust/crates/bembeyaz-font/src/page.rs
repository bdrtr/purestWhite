//! The codepoint set of a page that `SdCardFont` keeps resident: gathered from the page's text,
//! checked against and merged with what is resident, and laid out as mini intervals
//! (`test/sd_card_font` pins it).

use crate::intervals::Table;

pub(crate) const REPLACEMENT: u32 = 0xFFFD;
const LIGATURE: usize = 8;

/// A run of codepoints and the position of its first glyph. `EpdUnicodeInterval` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Interval {
    pub first: u32,
    pub last: u32,
    pub offset: u32,
}

/// A codepoint and its glyph index in the font. `bb_cpfont_mapping` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Mapping {
    pub codepoint: u32,
    pub index: i32,
}

// A plain scan that stops at the first match: `<[u32]>::contains` compares 16 at a time before
// it can stop, which costs more on the ESP32-S3, and it would be inlined at every call.
#[inline(never)]
pub(crate) fn seen_before(seen: &[u32], cp: u32) -> bool {
    for &x in seen {
        if x == cp {
            return true;
        }
    }
    false
}

fn push(out: &mut [u32], count: &mut usize, cp: u32) {
    if let Some(slot) = out.get_mut(*count) {
        *slot = cp;
        *count += 1;
    }
}

/// Gathers a page's codepoints into `out`: those of `texts` (C strings without their terminator)
/// in the order they first appear, up to `budget`, then U+FFFD and the outputs of the `ligatures`
/// (8-byte records, left << 16 | right then the output, little-endian; one table per style) whose
/// two parts are both on the page, while `out` has room. Returns how many there are; the caller
/// sorts them.
pub fn gather<'a>(
    texts: impl IntoIterator<Item = &'a [u8]>,
    budget: usize,
    ligatures: &[&[u8]],
    out: &mut [u32],
) -> usize {
    let room = out.len();
    let budget = budget.min(room);
    let mut count = 0;
    let has = |out: &[u32], count: usize, cp: u32| {
        out.get(..count).is_some_and(|seen| seen_before(seen, cp))
    };
    'texts: for text in texts {
        for cp in bembeyaz_utf8::codepoints(text) {
            if count >= budget {
                break 'texts;
            }
            if !has(out, count, cp) {
                push(out, &mut count, cp);
            }
        }
        if count >= budget {
            break;
        }
    }
    if !has(out, count, REPLACEMENT) && count < room {
        push(out, &mut count, REPLACEMENT);
    }
    for table in ligatures {
        for record in table.as_chunks::<LIGATURE>().0 {
            if count >= room {
                break;
            }
            let pair = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
            let output = u32::from_le_bytes([record[4], record[5], record[6], record[7]]);
            if has(out, count, pair >> 16)
                && has(out, count, pair & 0xFFFF)
                && !has(out, count, output)
            {
                push(out, &mut count, output);
            }
        }
    }
    count
}

fn contains(mini: &[Interval], cp: u32) -> bool {
    for iv in mini {
        if cp < iv.first {
            break; // intervals sorted ascending
        }
        if cp <= iv.last {
            return true;
        }
    }
    false
}

/// Whether the resident `mini` intervals hold every codepoint of `request` that the font covers
/// (`table`); if so, how many the font lacks.
#[must_use]
pub fn mini_covers(request: &[u32], mini: &[Interval], table: &Table) -> Option<u32> {
    let mut missed: u32 = 0;
    for &cp in request {
        if contains(mini, cp) {
            continue;
        }
        if table.find(cp).is_some() {
            return None;
        }
        missed += 1;
    }
    Some(missed)
}

/// Merges the resident `mini` codepoints with the sorted `request` into `out`, dropping
/// duplicates. Returns how many there are, or `None` when `mini` is empty or they do not fit.
pub fn merge_mini(mini: &[Interval], request: &[u32], out: &mut [u32]) -> Option<usize> {
    let mut intervals = mini.iter();
    let mut current = intervals.next()?;
    let mut cp = current.first;
    let mut active = true;
    let mut ri = 0;
    let mut n = 0;
    while (active || ri < request.len()) && n < out.len() {
        let wanted = request.get(ri).copied();
        let next = if active && wanted.is_none_or(|r| cp <= r) {
            let next = cp;
            if wanted == Some(cp) {
                ri += 1;
            }
            if cp < current.last {
                cp += 1;
            } else if let Some(following) = intervals.next() {
                current = following;
                cp = current.first;
            } else {
                active = false;
            }
            next
        } else {
            ri += 1;
            wanted.unwrap_or(0)
        };
        push(out, &mut n, next);
    }
    (!active && ri >= request.len()).then_some(n)
}

/// Maps each of the sorted `codepoints` the font covers to its glyph index, into `out` (one entry
/// per codepoint at most). Returns how many it covers.
pub fn find_glyphs(table: &Table, codepoints: &[u32], out: &mut [Mapping]) -> usize {
    let mut count = 0;
    for &codepoint in codepoints {
        if let Some(index) = table.find(codepoint).and_then(|i| i32::try_from(i).ok())
            && let Some(slot) = out.get_mut(count)
        {
            *slot = Mapping { codepoint, index };
            count += 1;
        }
    }
    count
}

/// Lays the mapped codepoints (sorted) out as intervals of consecutive codepoints into `out` (one
/// per mapping at most), each pointing at its first mapping. Returns how many it wrote.
pub fn build_mini_intervals(mappings: &[Mapping], out: &mut [Interval]) -> usize {
    let mut count = 0;
    let mut start = 0;
    for i in 1..=mappings.len() {
        let breaks = match (mappings.get(i), mappings.get(i - 1)) {
            (Some(m), Some(prev)) => m.codepoint != prev.codepoint.wrapping_add(1),
            _ => true,
        };
        if !breaks {
            continue;
        }
        if let (Some(first), Some(last), Some(slot)) =
            (mappings.get(start), mappings.get(i - 1), out.get_mut(count))
        {
            #[allow(clippy::cast_possible_truncation)]
            {
                *slot = Interval {
                    first: first.codepoint,
                    last: last.codepoint,
                    offset: start as u32,
                };
            }
            count += 1;
        }
        start = i;
    }
    count
}

/// Writes the resident `mini` codepoints into `out`, as many as fit. Returns how many.
pub fn mini_codepoints(mini: &[Interval], out: &mut [u32]) -> usize {
    let mut count = 0;
    for iv in mini {
        for cp in iv.first..=iv.last {
            if count >= out.len() {
                return count;
            }
            push(out, &mut count, cp);
        }
    }
    count
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation,
        clippy::range_plus_one
    )]

    use super::{
        Interval, Mapping, build_mini_intervals, find_glyphs, gather, merge_mini, mini_codepoints,
        mini_covers,
    };
    use crate::intervals::Table;
    use std::vec;
    use std::vec::Vec;

    fn ligature(left: u8, right: u8, output: u8) -> [u8; 8] {
        let pair = u32::from(left) << 16 | u32::from(right);
        let mut r = [0u8; 8];
        r[..4].copy_from_slice(&pair.to_le_bytes());
        r[4..].copy_from_slice(&u32::from(output).to_le_bytes());
        r
    }

    #[test]
    fn gathers_unique_codepoints_with_replacement_and_ligatures() {
        let mut out = [0u32; 512];
        let liga = ligature(b'E', b'F', b'A');
        let n = gather([&b"FEF"[..], "ç".as_bytes()], 512, &[&liga], &mut out);
        assert_eq!(&out[..n], &[0x46, 0x45, 0xE7, 0xFFFD, 0x41]);
        let n = gather([&b"E"[..]], 512, &[&liga], &mut out);
        assert_eq!(&out[..n], &[0x45, 0xFFFD]);
        // The budget keeps the first codepoints; a full page has no room for U+FFFD.
        let n = gather([&b"DCBA"[..]], 2, &[], &mut out);
        assert_eq!(&out[..n], &[0x44, 0x43, 0xFFFD]);
        let mut small = [0u32; 2];
        let n = gather([&b"DCBA"[..]], 512, &[&liga], &mut small);
        assert_eq!(&small[..n], &[0x44, 0x43]);
    }

    fn table() -> Vec<u8> {
        // 'A'..'Z' at glyph 0.
        [0x41u16, 0x5A, 0]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }

    #[test]
    fn checks_merges_maps_and_lays_out() {
        let bytes = table();
        let table = Table {
            entries: &bytes,
            compact: true,
        };
        let mini = [
            Interval {
                first: 0x41,
                last: 0x43,
                offset: 0,
            },
            Interval {
                first: 0x50,
                last: 0x50,
                offset: 3,
            },
        ];
        assert_eq!(mini_covers(&[0x41, 0x50, 0x61], &mini, &table), Some(1));
        assert_eq!(mini_covers(&[0x41, 0x44], &mini, &table), None);

        let request = [0x42, 0x45, 0x50, 0x51];
        let mut out = vec![0u32; 4 + request.len()];
        let n = merge_mini(&mini, &request, &mut out).unwrap();
        assert_eq!(&out[..n], &[0x41, 0x42, 0x43, 0x45, 0x50, 0x51]);
        assert_eq!(merge_mini(&mini, &request, &mut [0u32; 3]), None);

        let mut mapped = [Mapping::default(); 4];
        let n = find_glyphs(&table, &[0x41, 0x42, 0x44, 0x61], &mut mapped);
        assert_eq!(
            &mapped[..n],
            &[
                Mapping {
                    codepoint: 0x41,
                    index: 0
                },
                Mapping {
                    codepoint: 0x42,
                    index: 1
                },
                Mapping {
                    codepoint: 0x44,
                    index: 3
                }
            ]
        );
        let mut intervals = [Interval::default(); 3];
        let n = build_mini_intervals(&mapped[..3], &mut intervals);
        assert_eq!(
            &intervals[..n],
            &[
                Interval {
                    first: 0x41,
                    last: 0x42,
                    offset: 0
                },
                Interval {
                    first: 0x44,
                    last: 0x44,
                    offset: 2
                }
            ]
        );

        let mut all = [0u32; 3];
        assert_eq!(mini_codepoints(&mini, &mut all), 3);
        assert_eq!(all, [0x41, 0x42, 0x43]);
    }

    #[test]
    fn any_input_never_panics() {
        let mut seed = 0x1234_abcd_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        let bytes = table();
        let table = Table {
            entries: &bytes,
            compact: true,
        };
        for _ in 0..500 {
            let text: Vec<u8> = (0..next() % 200)
                .map(|_| (next() % 255 + 1) as u8)
                .collect();
            let ligatures: Vec<u8> = (0..next() % 40).map(|_| next() as u8).collect();
            let mut out = vec![0u32; (next() % 600) as usize];
            let n = gather(
                [&text[..]],
                (next() % 600) as usize,
                &[&ligatures],
                &mut out,
            );
            let mut mini: Vec<Interval> = Vec::new();
            let mut cp = next() % 100;
            for _ in 0..1 + next() % 5 {
                let last = cp + next() % 10;
                mini.push(Interval {
                    first: cp,
                    last,
                    offset: 0,
                });
                cp = last + 2 + next() % 10;
            }
            let _ = mini_covers(&out[..n], &mini, &table);
            let mut merged = vec![0u32; (next() % 700) as usize];
            let _ = merge_mini(&mini, &out[..n], &mut merged);
            let mut mapped = vec![Mapping::default(); n];
            let m = find_glyphs(&table, &out[..n], &mut mapped);
            let mut intervals = vec![Interval::default(); (next() % 20) as usize];
            let _ = build_mini_intervals(&mapped[..m], &mut intervals);
            let _ = mini_codepoints(&mini, &mut merged);
        }
    }
}
