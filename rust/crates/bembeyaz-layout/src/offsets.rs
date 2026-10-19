//! Where each token starts in the chapter's visible text, and the source codepoints NFC
//! composition absorbed, kept as lib/Epub's `ParsedText` kept them: 16-bit deltas from a base with
//! sparse rebases, and absorbed offsets in chunks of 64.

use alloc::vec::Vec;

#[derive(Clone, Copy)]
struct Rebase {
    word: usize,
    base: u32,
}

/// The visible-text offset of each token.
#[derive(Default)]
pub(crate) struct VisibleOffsets {
    deltas: Vec<u16>,
    base: u32,
    rebases: Vec<Rebase>,
}

/// Out of memory.
pub(crate) struct Full;

fn out_of_range(offset: u32, base: u32) -> bool {
    offset < base || offset - base > u32::from(u16::MAX)
}

fn delta(offset: u32, base: u32) -> u16 {
    #[allow(clippy::cast_possible_truncation)]
    let delta = offset.wrapping_sub(base) as u16;
    delta
}

impl VisibleOffsets {
    fn base_at(&self, word: usize) -> u32 {
        let mut base = self.base;
        for rebase in &self.rebases {
            if rebase.word > word {
                break;
            }
            base = rebase.base;
        }
        base
    }

    pub(crate) fn at(&self, word: usize) -> u32 {
        self.deltas
            .get(word)
            .map_or(0, |&d| self.base_at(word).wrapping_add(u32::from(d)))
    }

    pub(crate) fn reserve(&mut self, additional: usize) -> Result<(), Full> {
        self.deltas.try_reserve(additional).map_err(|_| Full)
    }

    pub(crate) fn push(&mut self, offset: u32) -> Result<(), Full> {
        let mut base = self.base;
        if self.deltas.is_empty() {
            self.base = offset;
            base = offset;
        } else if let Some(last) = self.rebases.last() {
            base = last.base;
        }
        if out_of_range(offset, base) {
            self.rebases.try_reserve(1).map_err(|_| Full)?;
            self.rebases.push(Rebase {
                word: self.deltas.len(),
                base: offset,
            });
            base = offset;
        }
        self.deltas.try_reserve(1).map_err(|_| Full)?;
        self.deltas.push(delta(offset, base));
        Ok(())
    }

    /// Takes back the last `push`.
    pub(crate) fn pop(&mut self) {
        self.deltas.pop();
        if self
            .rebases
            .last()
            .is_some_and(|r| r.word == self.deltas.len())
        {
            self.rebases.pop();
        }
    }

    /// Room for one `insert`, so it cannot fail after this.
    pub(crate) fn reserve_insert(&mut self) -> Result<(), Full> {
        self.rebases.try_reserve(1).map_err(|_| Full)?;
        self.deltas.try_reserve(1).map_err(|_| Full)
    }

    /// Kept as the C++ did it: a rebase inserted here also becomes the base of the word after
    /// `word`, whose delta was stored against the old one. To be fixed in its own commit.
    pub(crate) fn insert(&mut self, word: usize, offset: u32) -> Result<(), Full> {
        let base = if word > 0 {
            self.base_at(word - 1)
        } else {
            self.base
        };
        self.rebases.try_reserve(1).map_err(|_| Full)?;
        self.deltas.try_reserve(1).map_err(|_| Full)?;
        for rebase in &mut self.rebases {
            if rebase.word >= word {
                rebase.word += 1;
            }
        }
        let mut insertion_base = base;
        if out_of_range(offset, base) {
            let at = self
                .rebases
                .iter()
                .position(|r| r.word > word)
                .unwrap_or(self.rebases.len());
            self.rebases.insert(at, Rebase { word, base: offset });
            insertion_base = offset;
        }
        self.deltas
            .insert(word.min(self.deltas.len()), delta(offset, insertion_base));
        Ok(())
    }

    /// Drops the first `count` tokens; returns where the remaining text starts (for
    /// `Absorbed::retire`), or `u32::MAX` when none remain.
    pub(crate) fn erase_prefix(&mut self, count: usize) -> u32 {
        if count >= self.deltas.len() {
            self.deltas.clear();
            self.rebases.clear();
            self.base = 0;
            return u32::MAX;
        }
        let remaining_start = self.at(count);
        let new_base = self.base_at(count);
        self.deltas.drain(..count);
        self.rebases.retain_mut(|rebase| {
            if rebase.word <= count {
                return false;
            }
            rebase.word -= count;
            true
        });
        self.base = new_base;
        remaining_start
    }
}

struct Chunk {
    base: u32,
    deltas: [u16; 64],
    begin: u8,
    count: u8,
}

impl Chunk {
    fn last(&self) -> u32 {
        let last = usize::from(self.count).saturating_sub(1);
        self.base
            .wrapping_add(u32::from(self.deltas.get(last).copied().unwrap_or(0)))
    }

    /// The first index in `from..count` whose delta is above `value`.
    fn upper_bound(&self, from: usize, value: u32) -> usize {
        let live = self
            .deltas
            .get(from..usize::from(self.count))
            .unwrap_or_default();
        from + live.partition_point(|&d| u32::from(d) <= value)
    }
}

/// Source codepoints NFC composition merged into the one before them, so a token's source range
/// can span what it absorbed.
#[derive(Default)]
pub(crate) struct Absorbed {
    chunks: Vec<Chunk>,
    /// The chunk the last `offset_after` search started from, with its start.
    cursor: Option<(usize, u32)>,
}

impl Absorbed {
    pub(crate) fn record(&mut self, offset: u32) -> Result<(), Full> {
        let needs_chunk = self
            .chunks
            .last()
            .is_none_or(|tail| tail.count == 64 || out_of_range(offset, tail.base));
        if needs_chunk {
            self.chunks.try_reserve(1).map_err(|_| Full)?;
            self.chunks.push(Chunk {
                base: offset,
                deltas: [0; 64],
                begin: 0,
                count: 0,
            });
        }
        if let Some(tail) = self.chunks.last_mut() {
            if let Some(slot) = tail.deltas.get_mut(usize::from(tail.count)) {
                *slot = delta(offset, tail.base);
            }
            tail.count += 1;
        }
        Ok(())
    }

    /// Where text `rendered` codepoints long that starts at `start` ends in the source, counting
    /// the absorbed codepoints inside it.
    pub(crate) fn offset_after(&mut self, start: u32, rendered: u32) -> u32 {
        let mut end = start.wrapping_add(rendered);
        let mut index = match self.cursor {
            Some((chunk, from)) if start >= from => chunk,
            _ => 0,
        };
        while self.chunks.get(index).is_some_and(|c| c.last() <= start) {
            index += 1;
        }
        self.cursor = (index < self.chunks.len()).then_some((index, start));
        for chunk in self.chunks.get(index..).unwrap_or_default() {
            let mut first = usize::from(chunk.begin);
            if start >= chunk.base {
                first = chunk.upper_bound(first, start - chunk.base);
            }
            for &d in chunk
                .deltas
                .get(first..usize::from(chunk.count))
                .unwrap_or_default()
            {
                if chunk.base.wrapping_add(u32::from(d)) > end {
                    return end;
                }
                end = end.wrapping_add(1);
            }
        }
        end
    }

    /// Forgets the offsets before `remaining_start`.
    pub(crate) fn retire(&mut self, remaining_start: u32) {
        self.cursor = None;
        let dead = self
            .chunks
            .iter()
            .take_while(|c| c.last() <= remaining_start)
            .count();
        self.chunks.drain(..dead);
        if let Some(head) = self.chunks.first_mut()
            && remaining_start >= head.base
        {
            let begin = head.upper_bound(0, remaining_start - head.base);
            head.begin = u8::try_from(begin).unwrap_or(u8::MAX);
        }
    }
}
