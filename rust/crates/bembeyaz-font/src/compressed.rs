//! The firmware's compressed fonts (`EpdFontData` with groups), as `FontDecompressor` reads them:
//! which glyphs a page needs, which groups hold them, where each glyph sits in its decompressed
//! group (rows padded to whole bytes), and the glyph packed without that padding
//! (`test/font_decompressor` pins this behaviour). Decompressing a group is the caller's.

use crate::glyph::Glyph;
use crate::page::{Interval, seen_before};

/// Glyphs a page keeps at most.
pub const MAX_PAGE_GLYPHS: usize = 512;
/// Groups a page decompresses at most.
pub const MAX_PAGE_GROUPS: usize = 128;
const LIGATURE: usize = 8;

/// A DEFLATE-compressed block of glyph bitmaps. `EpdFontGroup` in C++.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Group {
    pub compressed_offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub glyph_count: u16,
    pub first_glyph: u32,
}

/// A glyph of a page and where its bitmap is. `bb_compressed_entry` in C.
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Entry {
    pub glyph: u32,
    /// In the page buffer; `u32::MAX` until extracted.
    pub buffer_offset: u32,
    /// In its decompressed group.
    pub aligned_offset: u32,
}

/// A compressed font's tables.
#[derive(Clone, Copy, Debug)]
pub struct Font<'a> {
    pub glyphs: &'a [Glyph],
    pub intervals: &'a [Interval],
    pub groups: &'a [Group],
    /// Each glyph's group, for frequency-grouped fonts; groups hold consecutive glyphs otherwise.
    pub glyph_to_group: Option<&'a [u16]>,
    /// 8-byte records: left << 16 | right, then the output codepoint (little-endian).
    pub ligatures: &'a [u8],
}

/// Bytes a glyph takes in its decompressed group: rows padded to whole bytes (2 bits a pixel).
fn aligned_size(g: &Glyph) -> u32 {
    if g.width > 0 && g.height > 0 {
        u32::from(g.width).div_ceil(4) * u32::from(g.height)
    } else {
        0
    }
}

impl Font<'_> {
    /// The glyph index of `codepoint`.
    #[must_use]
    pub fn find_glyph(&self, codepoint: u32) -> Option<u32> {
        let (mut low, mut high) = (0, self.intervals.len());
        while low < high {
            let mid = low + (high - low) / 2;
            let iv = self.intervals.get(mid)?;
            if codepoint < iv.first {
                high = mid;
            } else if codepoint > iv.last {
                low = mid + 1;
            } else {
                return Some(iv.offset.wrapping_add(codepoint - iv.first));
            }
        }
        None
    }

    /// The group that holds `glyph`.
    #[must_use]
    pub fn group_of(&self, glyph: u32) -> Option<u16> {
        let group = if let Some(map) = self.glyph_to_group {
            *map.get(glyph as usize)?
        } else {
            let i = self.groups.iter().position(|g| {
                glyph >= g.first_glyph
                    && glyph < g.first_glyph.wrapping_add(u32::from(g.glyph_count))
            })?;
            u16::try_from(i).ok()?
        };
        (usize::from(group) < self.groups.len()).then_some(group)
    }

    /// Where `glyph` starts in its decompressed `group`.
    #[must_use]
    pub fn aligned_offset(&self, group: u16, glyph: u32) -> u32 {
        let before = |i: u32| self.glyphs.get(i as usize).map_or(0, aligned_size);
        if let Some(map) = self.glyph_to_group {
            (0..glyph)
                .filter(|&i| map.get(i as usize) == Some(&group))
                .fold(0u32, |sum, i| sum.wrapping_add(before(i)))
        } else {
            let first = self
                .groups
                .get(usize::from(group))
                .map_or(glyph, |g| g.first_glyph);
            (first..glyph).fold(0u32, |sum, i| sum.wrapping_add(before(i)))
        }
    }

    /// Packs `glyph` from its `decompressed` group into `packed` (see [`compact`]).
    pub fn unpack(&self, group: u16, glyph: u32, decompressed: &[u8], packed: &mut [u8]) {
        let Some(g) = self.glyphs.get(glyph as usize) else {
            return;
        };
        let at = self.aligned_offset(group, glyph) as usize;
        compact(
            decompressed.get(at..).unwrap_or_default(),
            packed,
            g.width,
            g.height,
        );
    }

    // The glyphs the font has, the most a frequency-grouped table can hold.
    fn glyph_count(&self) -> u32 {
        let glyphs = u32::try_from(self.glyphs.len()).unwrap_or(u32::MAX);
        self.glyph_to_group.map_or(glyphs, |map| {
            glyphs.min(u32::try_from(map.len()).unwrap_or(u32::MAX))
        })
    }
}

/// Packs a glyph's rows from `aligned` (each padded to whole bytes) into `packed` without the
/// padding. Bytes past either slice are left out.
pub fn compact(aligned: &[u8], packed: &mut [u8], width: u8, height: u8) {
    if width == 0 || height == 0 {
        return;
    }
    let stride = usize::from(width).div_ceil(4);
    if width.is_multiple_of(4) {
        let n = (stride * usize::from(height))
            .min(aligned.len())
            .min(packed.len());
        if let (Some(out), Some(src)) = (packed.get_mut(..n), aligned.get(..n)) {
            out.copy_from_slice(src);
        }
        return;
    }
    let (mut byte, mut bits, mut at) = (0u8, 0u8, 0usize);
    for y in 0..usize::from(height) {
        for x in 0..usize::from(width) {
            let src = aligned.get(y * stride + x / 4).copied().unwrap_or(0);
            byte = (byte << 2) | ((src >> ((3 - (x % 4)) * 2)) & 0x3);
            bits += 2;
            if bits == 8 {
                if let Some(slot) = packed.get_mut(at) {
                    *slot = byte;
                }
                at += 1;
                byte = 0;
                bits = 0;
            }
        }
    }
    if bits > 0
        && let Some(slot) = packed.get_mut(at)
    {
        *slot = byte << (8 - bits);
    }
}

/// Gathers the glyphs of `text` (a C string without its terminator) the font has, unique, in
/// the order they first appear, up to `out`'s length (at most [`MAX_PAGE_GLYPHS`]), then the
/// outputs of ligatures whose two parts are both among them while there is room. Returns how many
/// there are and whether the text had more than fit.
pub fn gather_glyphs(font: &Font, text: &[u8], out: &mut [u32]) -> (usize, bool) {
    let room = out.len().min(MAX_PAGE_GLYPHS);
    let mut count = 0;
    let mut capped = false;
    let has = |out: &[u32], count: usize, g: u32| {
        out.get(..count).is_some_and(|seen| seen_before(seen, g))
    };
    for cp in bembeyaz_utf8::codepoints(text) {
        let Some(glyph) = font.find_glyph(cp) else {
            continue;
        };
        if has(out, count, glyph) {
            continue;
        }
        if count < room
            && let Some(slot) = out.get_mut(count)
        {
            *slot = glyph;
            count += 1;
        } else {
            capped = true;
        }
    }
    for record in font.ligatures.as_chunks::<LIGATURE>().0 {
        if count >= room {
            break;
        }
        let pair = u32::from_le_bytes([record[0], record[1], record[2], record[3]]);
        let output = u32::from_le_bytes([record[4], record[5], record[6], record[7]]);
        let (Some(left), Some(right)) =
            (font.find_glyph(pair >> 16), font.find_glyph(pair & 0xFFFF))
        else {
            continue;
        };
        if !has(out, count, left) || !has(out, count, right) {
            continue;
        }
        let Some(glyph) = font.find_glyph(output) else {
            continue;
        };
        if !has(out, count, glyph)
            && let Some(slot) = out.get_mut(count)
        {
            *slot = glyph;
            count += 1;
        }
    }
    (count, capped)
}

/// A page's plan: how many groups it needs and the bytes its packed bitmaps take.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Plan {
    pub group_count: usize,
    pub total_bytes: u32,
    /// More groups were needed than `groups` holds.
    pub capped: bool,
}

/// Plans a page: the groups of `glyphs` in the order they are first needed, into `groups` (at
/// most [`MAX_PAGE_GROUPS`]), and the bytes of all their packed bitmaps.
pub fn plan(font: &Font, glyphs: &[u32], groups: &mut [u16]) -> Plan {
    let len = groups.len().min(MAX_PAGE_GROUPS);
    let groups = groups.get_mut(..len).unwrap_or_default();
    let mut plan = Plan::default();
    for &glyph in glyphs {
        let length = font
            .glyphs
            .get(glyph as usize)
            .map_or(0, |g| u32::from(g.data_length));
        plan.total_bytes = plan.total_bytes.wrapping_add(length);
        let Some(group) = font.group_of(glyph) else {
            continue;
        };
        let known = groups
            .get(..plan.group_count)
            .is_some_and(|g| g.contains(&group));
        if known {
            continue;
        }
        if let Some(slot) = groups.get_mut(plan.group_count) {
            *slot = group;
            plan.group_count += 1;
        } else {
            plan.capped = true;
        }
    }
    plan
}

fn find_entry_index(entries: &[Entry], glyph: u32) -> Option<usize> {
    entries.binary_search_by_key(&glyph, |e| e.glyph).ok()
}

/// Lays out a page's `entries` (one per glyph): sorted by glyph, none extracted yet, each with
/// its offset in its decompressed group when `planned` (the plan's groups, at most
/// [`MAX_PAGE_GROUPS`]) holds that group.
pub fn index(font: &Font, glyphs: &[u32], planned: &[u16], entries: &mut [Entry]) {
    for (entry, &glyph) in entries.iter_mut().zip(glyphs) {
        *entry = Entry {
            glyph,
            buffer_offset: u32::MAX,
            aligned_offset: 0,
        };
    }
    let entries = entries.get_mut(..glyphs.len()).unwrap_or_default();
    // Insertion sort: a page is small, and this keeps the code size down.
    for i in 1..entries.len() {
        let mut j = i;
        while j > 0
            && entries
                .get(j - 1)
                .zip(entries.get(j))
                .is_some_and(|(a, b)| a.glyph > b.glyph)
        {
            entries.swap(j - 1, j);
            j -= 1;
        }
    }
    let planned = planned.get(..MAX_PAGE_GROUPS).unwrap_or(planned);
    if let Some(map) = font.glyph_to_group {
        // One pass over the glyphs, keeping a running offset per planned group.
        let mut offsets = [0u32; MAX_PAGE_GROUPS];
        for i in 0..font.glyph_count() {
            let Some(&group) = map.get(i as usize) else {
                break;
            };
            let Some(at) = planned.iter().position(|&g| g == group) else {
                continue;
            };
            if let Some(e) = find_entry_index(entries, i).and_then(|k| entries.get_mut(k))
                && let Some(&offset) = offsets.get(at)
            {
                e.aligned_offset = offset;
            }
            let size = font.glyphs.get(i as usize).map_or(0, aligned_size);
            if let Some(offset) = offsets.get_mut(at) {
                *offset = offset.wrapping_add(size);
            }
        }
    } else {
        for &group in planned {
            let Some(g) = font.groups.get(usize::from(group)) else {
                continue;
            };
            let mut offset = 0u32;
            for j in 0..u32::from(g.glyph_count) {
                let i = g.first_glyph.wrapping_add(j);
                if let Some(e) = find_entry_index(entries, i).and_then(|k| entries.get_mut(k)) {
                    e.aligned_offset = offset;
                }
                offset = offset.wrapping_add(font.glyphs.get(i as usize).map_or(0, aligned_size));
            }
        }
    }
}

/// Packs the page glyphs of `group` from its `decompressed` bitmaps into `buffer`, starting at
/// `write`, and records where each went. Returns where the next one goes.
pub fn extract(
    font: &Font,
    group: u16,
    decompressed: &[u8],
    entries: &mut [Entry],
    buffer: &mut [u8],
    write: u32,
) -> u32 {
    let mut write = write;
    for e in entries.iter_mut() {
        if e.buffer_offset != u32::MAX || font.group_of(e.glyph) != Some(group) {
            continue;
        }
        let Some(glyph) = font.glyphs.get(e.glyph as usize) else {
            continue;
        };
        let src = decompressed
            .get(e.aligned_offset as usize..)
            .unwrap_or_default();
        let dst = buffer.get_mut(write as usize..).unwrap_or_default();
        compact(src, dst, glyph.width, glyph.height);
        e.buffer_offset = write;
        write = write.wrapping_add(u32::from(glyph.data_length));
    }
    write
}

/// Where `glyph`'s bitmap is in the page buffer, unless the page lacks it or did not extract it.
#[must_use]
pub fn find_entry(entries: &[Entry], glyph: u32) -> Option<u32> {
    let e = entries.get(find_entry_index(entries, glyph)?)?;
    (e.buffer_offset != u32::MAX).then_some(e.buffer_offset)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::indexing_slicing,
        clippy::unwrap_used,
        clippy::cast_possible_truncation
    )]

    use super::{Entry, Font, Group, compact, extract, find_entry, gather_glyphs, index, plan};
    use crate::glyph::Glyph;
    use crate::page::Interval;
    use std::vec;
    use std::vec::Vec;

    fn glyph(width: u8, height: u8) -> Glyph {
        let length = (u16::from(width) * u16::from(height)).div_ceil(4);
        Glyph {
            width,
            height,
            data_length: length,
            ..Glyph::default()
        }
    }

    // Glyphs 'A'..'E': 5×2, 4×1, 3×3 in group 0; 8×1, 2×2 in group 1.
    fn font<'a>(
        glyphs: &'a [Glyph],
        groups: &'a [Group],
        map: Option<&'a [u16]>,
        ligatures: &'a [u8],
    ) -> Font<'a> {
        Font {
            glyphs,
            intervals: &[Interval {
                first: 0x41,
                last: 0x45,
                offset: 0,
            }],
            groups,
            glyph_to_group: map,
            ligatures,
        }
    }

    #[test]
    fn compacts_padded_rows() {
        // 5 pixels a row: 2 bytes each, the second holding one pixel.
        let aligned = [0b1110_0100, 0b1100_0000, 0b0001_1011, 0b0100_0000];
        let mut packed = [0u8; 3];
        compact(&aligned, &mut packed, 5, 2);
        assert_eq!(packed, [0b1110_0100, 0b1100_0110, 0b1101_0000]);
        let mut short = [0u8; 1];
        compact(&aligned, &mut short, 5, 2);
        assert_eq!(short, [0b1110_0100]);
    }

    #[test]
    fn plans_indexes_and_extracts_a_page() {
        let glyphs = [
            glyph(5, 2),
            glyph(4, 1),
            glyph(3, 3),
            glyph(8, 1),
            glyph(2, 2),
        ];
        let groups = [
            Group {
                glyph_count: 3,
                first_glyph: 0,
                ..Group::default()
            },
            Group {
                glyph_count: 2,
                first_glyph: 3,
                ..Group::default()
            },
        ];
        let mut liga = Vec::new();
        liga.extend_from_slice(&(0x41u32 << 16 | 0x45).to_le_bytes());
        liga.extend_from_slice(&0x42u32.to_le_bytes());
        for map in [None, Some(&[0u16, 0, 0, 1, 1][..])] {
            let f = font(&glyphs, &groups, map, &liga);
            assert_eq!(
                (f.group_of(4), f.group_of(5), f.aligned_offset(0, 2)),
                (Some(1), None, 4 + 1)
            );
            let mut page = [0u32; 512];
            let (n, capped) = gather_glyphs(&f, b"EAzE", &mut page);
            assert!(!capped);
            assert_eq!(&page[..n], &[4, 0, 1]);
            let mut groups = [0u16; 128];
            let p = plan(&f, &page[..n], &mut groups);
            assert_eq!(
                (&groups[..p.group_count], p.total_bytes),
                (&[1u16, 0][..], 1 + 3 + 1)
            );
            assert!(plan(&f, &page[..n], &mut groups[..1]).capped);
            let mut entries = vec![Entry::default(); n];
            index(&f, &page[..n], &groups[..p.group_count], &mut entries);
            assert_eq!(
                entries
                    .iter()
                    .map(|e| (e.glyph, e.aligned_offset))
                    .collect::<Vec<_>>(),
                [(0, 0), (1, 4), (4, 2)]
            );
            let mut buffer = vec![0u8; p.total_bytes as usize];
            let second = [0xAA, 0x55, 0xF0, 0x0F];
            let write = extract(&f, 1, &second, &mut entries, &mut buffer, 0);
            assert_eq!(
                (
                    write,
                    find_entry(&entries, 4),
                    find_entry(&entries, 0),
                    find_entry(&entries, 3)
                ),
                (1, Some(0), None, None)
            );
            assert_eq!(buffer[0], 0b1111_0000);
            let mut one = [0u8; 1];
            f.unpack(1, 4, &second, &mut one);
            assert_eq!(one[0], 0b1111_0000);
        }
    }

    #[test]
    fn any_font_never_panics() {
        let mut seed = 0xfeed_5eed_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..300 {
            let glyphs: Vec<Glyph> = (0..next() % 40)
                .map(|_| glyph(next() as u8 % 50, next() as u8 % 50))
                .collect();
            let groups: Vec<Group> = (0..next() % 6)
                .map(|_| Group {
                    glyph_count: (next() % 20) as u16,
                    first_glyph: next() % 50,
                    uncompressed_size: next() % 500,
                    ..Group::default()
                })
                .collect();
            let map: Vec<u16> = (0..glyphs.len()).map(|_| (next() % 8) as u16).collect();
            let liga: Vec<u8> = (0..next() % 40).map(|_| next() as u8).collect();
            let intervals = [Interval {
                first: 0x20,
                last: 0x20 + next() % 60,
                offset: next() % 10,
            }];
            let f = Font {
                glyphs: &glyphs,
                intervals: &intervals,
                groups: &groups,
                glyph_to_group: (next() % 2 == 0).then_some(&map[..]),
                ligatures: &liga,
            };
            let text: Vec<u8> = (0..next() % 100)
                .map(|_| (next() % 0x60 + 0x20) as u8)
                .collect();
            let mut page = [0u32; 512];
            let (n, _) = gather_glyphs(&f, &text, &mut page);
            let mut groups = [0u16; 128];
            let p = plan(&f, &page[..n], &mut groups);
            let mut entries = vec![Entry::default(); n];
            index(&f, &page[..n], &groups[..p.group_count], &mut entries);
            let decompressed: Vec<u8> = (0..next() % 300).map(|_| next() as u8).collect();
            let mut buffer = vec![0u8; (next() % 200) as usize];
            for &g in &groups[..p.group_count] {
                let _ = extract(&f, g, &decompressed, &mut entries, &mut buffer, 0);
            }
            let mut packed = vec![0u8; (next() % 40) as usize];
            f.unpack((next() % 8) as u16, next() % 60, &decompressed, &mut packed);
        }
    }
}
