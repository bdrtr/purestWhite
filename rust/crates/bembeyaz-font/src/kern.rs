//! The per-page kern matrix that `SdCardFont` builds from a style's class tables (see
//! [`crate::cpfont`] for the layout): only the class pairs a page's codepoints can reach,
//! renumbered 1..N (`test/sd_card_font` pins it). Class tables are read 64 entries at a time.

use crate::Source;
use crate::cpfont::{KERN_ENTRY_SIZE, Style};

const ENTRY: usize = KERN_ENTRY_SIZE as usize;
// Class-table entries per read, and per block of the block index.
const CLASS_BLOCK: u16 = 64;
const BLOCK_BYTES: usize = CLASS_BLOCK as usize * ENTRY;
const BLOCK_STRIDE: u32 = CLASS_BLOCK as u32 * KERN_ENTRY_SIZE;

fn class_blocks(entries: u16) -> u16 {
    entries.div_ceil(CLASS_BLOCK)
}

/// Kern block index entries for `style`: the first codepoint of each block of its left class
/// table, then the table's last codepoint, then the same for the right table.
#[must_use]
pub fn index_len(style: &Style) -> usize {
    usize::from(class_blocks(style.kern_left_entries))
        + 1
        + usize::from(class_blocks(style.kern_right_entries))
        + 1
}

/// Scratch bytes [`build_mini`] needs for `count` page codepoints.
#[must_use]
pub fn scratch_len(style: &Style, count: usize) -> usize {
    count * 2 + BLOCK_BYTES.max(usize::from(style.kern_right_classes))
}

/// Why [`build_mini`] failed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// `scratch` is shorter than [`scratch_len`].
    Scratch,
    /// A class table could not be read.
    ClassRead,
    /// `reserve` gave no tables.
    Reserve,
    /// The matrix row of this left class could not be reached.
    RowSeek(u8),
    /// The matrix row of this left class could not be read.
    RowRead(u8),
}

/// The size of a page's matrix.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct MiniKern {
    /// `left_classes × right_classes`.
    pub matrix_bytes: u32,
    /// Page codepoints with a left class, then with a right class.
    pub left_entries: u16,
    pub right_entries: u16,
    /// Classes in the matrix; 0 when the page has no kern pairs.
    pub left_classes: u8,
    pub right_classes: u8,
}

/// Where [`build_mini`] writes: the left and right class tables (3 bytes per entry, as in the
/// file) and the matrix.
pub struct Tables<'a> {
    pub left: &'a mut [u8],
    pub right: &'a mut [u8],
    pub matrix: &'a mut [i8],
}

// One side's class table.
struct ClassTable {
    offset: u32,
    entries: u16,
    classes: u8,
}

impl ClassTable {
    fn left(style: &Style) -> Self {
        ClassTable {
            offset: style.kern_left_offset,
            entries: style.kern_left_entries,
            classes: style.kern_left_classes,
        }
    }

    fn right(style: &Style) -> Self {
        ClassTable {
            offset: style.kern_right_offset,
            entries: style.kern_right_entries,
            classes: style.kern_right_classes,
        }
    }

    // Stores the class of each sorted page codepoint in `classes` (0 for none). With the index
    // filled in, only blocks whose range holds a page codepoint are read; otherwise every block
    // is, and the index (if any) is filled on the way.
    fn read(
        &self,
        file: &mut impl Source,
        codepoints: &[u32],
        classes: &mut [u8],
        block: &mut [u8],
        mut index: Option<&mut [u16]>,
        index_ready: bool,
    ) -> bool {
        classes.fill(0);
        let blocks = class_blocks(self.entries);
        let count = codepoints.len();
        let at = |i: usize| codepoints.get(i).copied().unwrap_or(u32::MAX);
        let mut next = 0;
        for b in 0..blocks {
            if next >= count && (index.is_none() || index_ready) {
                break;
            }
            if index_ready && let Some(index) = index.as_deref() {
                let entry = |i: u16| u32::from(index.get(usize::from(i)).copied().unwrap_or(0));
                while next < count && at(next) < entry(b) {
                    next += 1;
                }
                let end = if b + 1 < blocks {
                    entry(b + 1)
                } else {
                    entry(blocks) + 1
                };
                if next == count || at(next) > entry(blocks) {
                    break;
                }
                if at(next) >= end {
                    continue;
                }
            }
            let in_block = CLASS_BLOCK.min(self.entries - b * CLASS_BLOCK);
            let Some(block) = block.get_mut(..usize::from(in_block) * ENTRY) else {
                return false;
            };
            if !file.seek(self.offset.wrapping_add(u32::from(b) * BLOCK_STRIDE))
                || file.read(block) != block.len()
            {
                return false;
            }
            let (block, _) = block.as_chunks::<ENTRY>();
            let codepoint = |e: &[u8; ENTRY]| u16::from_le_bytes([e[0], e[1]]);
            if !index_ready && let Some(index) = index.as_deref_mut() {
                if let (Some(slot), Some(first)) = (index.get_mut(usize::from(b)), block.first()) {
                    *slot = codepoint(first);
                }
                if b + 1 == blocks
                    && let (Some(slot), Some(last)) =
                        (index.get_mut(usize::from(blocks)), block.last())
                {
                    *slot = codepoint(last);
                }
            }
            for e in block {
                if next >= count {
                    break;
                }
                let cp = u32::from(codepoint(e));
                // An id past the matrix would index outside it: the entry is unkerned.
                let class = if e[2] <= self.classes { e[2] } else { 0 };
                while next < count && at(next) < cp {
                    next += 1;
                }
                let mut j = next;
                while j < count && at(j) == cp {
                    if let Some(c) = classes.get_mut(j) {
                        *c = class;
                    }
                    j += 1;
                }
            }
        }
        true
    }
}

// The classes one side of a page uses, renumbered 1..N in order, and back.
struct Renumbered {
    // Page codepoints with a class.
    entries: u16,
    count: u16,
    new_of_old: [u8; 256],
    old_of_new: [u8; 256],
}

fn renumber(classes: &[u8]) -> Renumbered {
    let mut r = Renumbered {
        entries: 0,
        count: 0,
        new_of_old: [0; 256],
        old_of_new: [0; 256],
    };
    for &class in classes {
        if class != 0 {
            r.entries = r.entries.wrapping_add(1);
            if let Some(used) = r.new_of_old.get_mut(usize::from(class)) {
                *used = 1;
            }
        }
    }
    for old in 1..=u8::MAX {
        if let Some(slot) = r.new_of_old.get_mut(usize::from(old))
            && *slot != 0
        {
            r.count += 1;
            let new = u8::try_from(r.count).unwrap_or(0);
            *slot = new;
            if let Some(back) = r.old_of_new.get_mut(usize::from(new)) {
                *back = old;
            }
        }
    }
    r
}

fn get(table: &[u8; 256], i: u8) -> u8 {
    table.get(usize::from(i)).copied().unwrap_or(0)
}

/// Builds the mini kern matrix for the sorted page `codepoints`.
///
/// `scratch` holds [`scratch_len`] bytes. `index` is the style's kern block index ([`index_len`]
/// entries), if it has one; `index_ready` says it is filled in; once both class tables are read
/// it becomes whether `index` was given. `reserve` is asked for tables of the given sizes once
/// the page has kern pairs. On success the counts say how much of them is set; all 0 when the page
/// has no kern pairs.
///
/// # Errors
///
/// A short `scratch`, a class table or matrix row that cannot be read, or no tables from
/// `reserve`.
pub fn build_mini<'t>(
    file: &mut impl Source,
    style: &Style,
    codepoints: &[u32],
    scratch: &mut [u8],
    index: Option<&mut [u16]>,
    index_ready: &mut bool,
    reserve: impl FnOnce(&MiniKern) -> Option<Tables<'t>>,
) -> Result<MiniKern, Error> {
    let count = codepoints.len();
    let Some(scratch) = scratch.get_mut(..scratch_len(style, count)) else {
        return Err(Error::Scratch);
    };
    let (left_classes, rest) = scratch.split_at_mut(count);
    let (right_classes, io) = rest.split_at_mut(count);
    let has_index = index.is_some();
    let (left_index, right_index) = match index {
        Some(index) => {
            let at = (usize::from(class_blocks(style.kern_left_entries)) + 1).min(index.len());
            let (l, r) = index.split_at_mut(at);
            (Some(l), Some(r))
        }
        None => (None, None),
    };
    let ready = *index_ready;
    if !ClassTable::left(style).read(file, codepoints, left_classes, io, left_index, ready)
        || !ClassTable::right(style).read(file, codepoints, right_classes, io, right_index, ready)
    {
        return Err(Error::ClassRead);
    }
    *index_ready = has_index;

    let left = renumber(left_classes);
    let right = renumber(right_classes);
    if left.count == 0 || right.count == 0 {
        return Ok(MiniKern::default());
    }
    let kern = MiniKern {
        matrix_bytes: u32::from(left.count) * u32::from(right.count),
        left_entries: left.entries,
        right_entries: right.entries,
        left_classes: u8::try_from(left.count).unwrap_or(0),
        right_classes: u8::try_from(right.count).unwrap_or(0),
    };
    let Some(tables) = reserve(&kern) else {
        return Err(Error::Reserve);
    };

    // The mini class tables, sorted because the codepoints are. Classes are only set for BMP
    // codepoints.
    let mut left_out = tables.left.as_chunks_mut::<ENTRY>().0.iter_mut();
    let mut right_out = tables.right.as_chunks_mut::<ENTRY>().0.iter_mut();
    for ((&cp, &lc), &rc) in codepoints
        .iter()
        .zip(left_classes.iter())
        .zip(right_classes.iter())
    {
        #[allow(clippy::cast_possible_truncation)]
        let [lo, hi] = (cp as u16).to_le_bytes();
        for (class, out, side) in [(lc, &mut left_out, &left), (rc, &mut right_out, &right)] {
            if class != 0
                && let Some(e) = out.next()
            {
                *e = [lo, hi, get(&side.new_of_old, class)];
            }
        }
    }

    // One seek and one read per used left class; keep the columns of the used right classes.
    let row_len = usize::from(style.kern_right_classes);
    let row = io.get_mut(..row_len).unwrap_or_default();
    let mut out_rows = tables.matrix.chunks_exact_mut(usize::from(right.count));
    for new_left in 1..=left.count {
        let old = get(&left.old_of_new, u8::try_from(new_left).unwrap_or(0));
        let position = style.kern_matrix_offset.wrapping_add(
            u32::from(old)
                .wrapping_sub(1)
                .wrapping_mul(u32::from(style.kern_right_classes)),
        );
        if !file.seek(position) {
            return Err(Error::RowSeek(old));
        }
        if file.read(row) != row_len {
            return Err(Error::RowRead(old));
        }
        let Some(out) = out_rows.next() else { break };
        for (new_right, cell) in (1..=right.count).zip(out.iter_mut()) {
            let old_right = get(&right.old_of_new, u8::try_from(new_right).unwrap_or(0));
            let byte = usize::from(old_right)
                .checked_sub(1)
                .and_then(|i| row.get(i))
                .copied()
                .unwrap_or(0);
            *cell = i8::from_le_bytes([byte]);
        }
    }
    Ok(kern)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used)]

    use super::{Error, MiniKern, Tables, build_mini, index_len, scratch_len};
    use crate::Source;
    use crate::cpfont::Style;
    use std::vec;
    use std::vec::Vec;

    struct File {
        bytes: Vec<u8>,
        at: usize,
        reads: usize,
    }

    impl Source for File {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            self.reads += 1;
            let n = buf.len().min(self.bytes.len().saturating_sub(self.at));
            buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
            self.at += n;
            n
        }
        fn seek(&mut self, position: u32) -> bool {
            self.at = position as usize;
            self.at <= self.bytes.len()
        }
    }

    // Left classes for A (1) and C (2), right for B (1) and D (2), a 2×2 matrix.
    fn font() -> (File, Style) {
        let mut bytes = Vec::new();
        for (cp, class) in [(b'A', 1), (b'C', 2), (b'B', 1), (b'D', 2)] {
            bytes.extend_from_slice(&[cp, 0, class]);
        }
        bytes.extend_from_slice(&[(-3i8).cast_unsigned(), 0, 4, (-5i8).cast_unsigned()]);
        let style = Style {
            kern_left_entries: 2,
            kern_right_entries: 2,
            kern_left_classes: 2,
            kern_right_classes: 2,
            kern_left_offset: 0,
            kern_right_offset: 6,
            kern_matrix_offset: 12,
            ..Style::default()
        };
        (
            File {
                bytes,
                at: 0,
                reads: 0,
            },
            style,
        )
    }

    struct Out {
        left: Vec<u8>,
        right: Vec<u8>,
        matrix: Vec<i8>,
    }

    fn build(
        file: &mut File,
        style: &Style,
        codepoints: &[u32],
        index: Option<&mut [u16]>,
        ready: &mut bool,
    ) -> (Result<MiniKern, Error>, Out) {
        let mut scratch = vec![0u8; scratch_len(style, codepoints.len())];
        let mut out = Out {
            left: vec![0; 3 * codepoints.len()],
            right: vec![0; 3 * codepoints.len()],
            matrix: vec![0; 255 * 255],
        };
        let result = build_mini(file, style, codepoints, &mut scratch, index, ready, |_| {
            Some(Tables {
                left: &mut out.left,
                right: &mut out.right,
                matrix: &mut out.matrix,
            })
        });
        (result, out)
    }

    #[test]
    fn builds_only_the_pairs_a_page_reaches() {
        let (mut file, style) = font();
        let mut ready = false;
        let (result, out) = build(
            &mut file,
            &style,
            &[u32::from(b'C'), u32::from(b'D'), 0x1F600],
            None,
            &mut ready,
        );
        let kern = result.unwrap();
        assert_eq!(
            (
                kern.left_entries,
                kern.right_entries,
                kern.left_classes,
                kern.right_classes
            ),
            (1, 1, 1, 1)
        );
        assert_eq!(&out.left[..3], &[b'C', 0, 1]);
        assert_eq!(&out.right[..3], &[b'D', 0, 1]);
        assert_eq!(out.matrix[0], -5);
        assert!(!ready);

        let (result, _) = build(&mut file, &style, &[u32::from(b'A')], None, &mut ready);
        assert_eq!(result.unwrap(), MiniKern::default());
    }

    #[test]
    fn the_block_index_skips_blocks_on_later_pages() {
        let (mut file, style) = font();
        let mut index = vec![0u16; index_len(&style)];
        let mut ready = false;
        let page = [u32::from(b'A'), u32::from(b'B')];
        let (result, out) = build(&mut file, &style, &page, Some(&mut index), &mut ready);
        assert_eq!(result.unwrap().matrix_bytes, 1);
        assert_eq!(out.matrix[0], -3);
        assert!(ready);
        assert_eq!(
            index,
            [
                u16::from(b'A'),
                u16::from(b'C'),
                u16::from(b'B'),
                u16::from(b'D')
            ]
        );
        file.reads = 0;
        let (result, _) = build(&mut file, &style, &[0x30], Some(&mut index), &mut ready);
        assert_eq!((result.unwrap(), file.reads), (MiniKern::default(), 0));
    }

    #[test]
    fn damaged_tables_fail() {
        let (mut file, style) = font();
        file.bytes.truncate(14);
        let page = [u32::from(b'C'), u32::from(b'D')];
        let mut ready = false;
        let (result, _) = build(&mut file, &style, &page, None, &mut ready);
        assert_eq!(result, Err(Error::RowRead(2)));
        file.bytes.truncate(10);
        let (result, _) = build(&mut file, &style, &page, None, &mut ready);
        assert_eq!(result, Err(Error::ClassRead));
        let (mut file, style) = font();
        let mut scratch = vec![0u8; scratch_len(&style, page.len()) - 1];
        let failed = build_mini(
            &mut file,
            &style,
            &page,
            &mut scratch,
            None,
            &mut ready,
            |_| None,
        );
        assert_eq!(failed, Err(Error::Scratch));
        scratch.push(0);
        let failed = build_mini(
            &mut file,
            &style,
            &page,
            &mut scratch,
            None,
            &mut ready,
            |_| None,
        );
        assert_eq!(failed, Err(Error::Reserve));
    }

    #[test]
    fn any_tables_never_panic() {
        let mut seed = 0x2468_1357_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..1000 {
            let bytes: Vec<u8> = (0..next() % 2000)
                .map(|_| next().to_le_bytes()[0])
                .collect();
            let style = Style {
                kern_left_entries: u16::try_from(next() % 300).unwrap(),
                kern_right_entries: u16::try_from(next() % 300).unwrap(),
                kern_left_classes: next().to_le_bytes()[0],
                kern_right_classes: next().to_le_bytes()[0],
                kern_left_offset: next() % 2100,
                kern_right_offset: next() % 2100,
                kern_matrix_offset: if next() % 8 == 0 {
                    next()
                } else {
                    next() % 2100
                },
                ..Style::default()
            };
            let mut codepoints: Vec<u32> = (0..next() % 80).map(|_| next() % 300).collect();
            codepoints.sort_unstable();
            let mut file = File {
                bytes,
                at: 0,
                reads: 0,
            };
            let mut index = vec![0u16; index_len(&style)];
            let mut ready = next() % 2 == 0;
            let _ = build(&mut file, &style, &codepoints, Some(&mut index), &mut ready);
            let _ = build(&mut file, &style, &codepoints, None, &mut ready);
        }
    }
}
