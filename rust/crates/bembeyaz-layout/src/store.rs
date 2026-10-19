//! Word text in 2 KB chunks, as the former C++ `WordStore` kept it: words are copied in NUL-terminated, and a
//! chunk is freed once every word in it is released (tokens are consumed in order as lines go
//! out). Uniform 2 KB requests stay allocatable on a fragmented heap where one growing block
//! would not.

use alloc::vec::Vec;

const CHUNK_SIZE: usize = 2048;
/// Offsets are 16-bit; parser tokens are far shorter.
const MAX_WORD_BYTES: usize = 0xFFFE;
/// Asked for beyond a failed request when evicting caches, for the allocations that follow.
const RECLAIM_HEADROOM: usize = 8 * 1024;

/// Frees rebuildable caches until about this many bytes are free (`MemoryManager::ensureFree`).
pub type Reclaim = extern "C" fn(usize);

/// A stored word: `len` bytes at `offset` in chunk `chunk`, then a NUL.
#[derive(Clone, Copy)]
pub(crate) struct Word {
    chunk: u32,
    offset: u16,
    pub(crate) len: u16,
}

impl Word {
    /// The word's last `len - at` bytes, sharing its NUL and its release.
    pub(crate) fn suffix(self, at: usize) -> Word {
        let at = u16::try_from(at).unwrap_or(self.len).min(self.len);
        Word {
            chunk: self.chunk,
            offset: self.offset + at,
            len: self.len - at,
        }
    }
}

struct Chunk {
    /// Never grown past its first capacity; `None` once retired.
    data: Option<Vec<u8>>,
    live: u16,
}

/// Tries `allocate`, then once more after evicting caches for `bytes`.
fn try_twice(reclaim: Option<Reclaim>, bytes: usize, mut allocate: impl FnMut() -> bool) -> bool {
    if allocate() {
        return true;
    }
    match reclaim {
        Some(reclaim) => {
            reclaim(bytes + RECLAIM_HEADROOM);
            allocate()
        }
        None => false,
    }
}

pub(crate) struct WordStore {
    chunks: Vec<Chunk>,
    reclaim: Option<Reclaim>,
}

impl WordStore {
    pub(crate) const fn new(reclaim: Option<Reclaim>) -> Self {
        WordStore {
            chunks: Vec::new(),
            reclaim,
        }
    }

    /// Copies `text` (up to 0xFFFE bytes) and a NUL in; `None` when out of memory.
    pub(crate) fn append(&mut self, text: &[u8]) -> Option<Word> {
        let text = text.get(..MAX_WORD_BYTES).unwrap_or(text);
        let need = text.len() + 1;
        let fits = self
            .chunks
            .last()
            .and_then(|c| c.data.as_ref())
            .is_some_and(|d| d.capacity() - d.len() >= need);
        if !fits {
            // Words larger than a chunk get an exact-fit chunk of their own.
            let capacity = need.max(CHUNK_SIZE);
            let reclaim = self.reclaim;
            let mut data = Vec::new();
            if !try_twice(reclaim, capacity, || self.chunks.try_reserve(1).is_ok())
                || !try_twice(reclaim, capacity, || {
                    data.try_reserve_exact(capacity).is_ok()
                })
            {
                return None;
            }
            self.chunks.push(Chunk {
                data: Some(data),
                live: 0,
            });
        }
        let index = self.chunks.len().checked_sub(1)?;
        let chunk = self.chunks.get_mut(index)?;
        let data = chunk.data.as_mut()?;
        let offset = u16::try_from(data.len()).ok()?;
        data.extend_from_slice(text);
        data.push(0);
        chunk.live += 1;
        Some(Word {
            chunk: u32::try_from(index).ok()?,
            offset,
            len: u16::try_from(text.len()).ok()?,
        })
    }

    pub(crate) fn view(&self, word: Word) -> &[u8] {
        let start = usize::from(word.offset);
        self.chunks
            .get(word.chunk as usize)
            .and_then(|c| c.data.as_ref())
            .and_then(|d| d.get(start..start + usize::from(word.len)))
            .unwrap_or_default()
    }

    /// A word is consumed: its chunk goes once nothing else lives in it (the last chunk stays
    /// for appends).
    pub(crate) fn release(&mut self, word: Word) {
        let last = self.chunks.len().saturating_sub(1);
        let index = word.chunk as usize;
        if let Some(chunk) = self.chunks.get_mut(index) {
            chunk.live = chunk.live.saturating_sub(1);
            if chunk.live == 0 && index != last {
                chunk.data = None;
            }
        }
    }

    /// Every chunk still held: NUL-separated words, consumed ones included.
    pub(crate) fn segments(&self) -> impl Iterator<Item = &[u8]> {
        self.chunks.iter().filter_map(|c| c.data.as_deref())
    }

    pub(crate) fn segment_count(&self) -> usize {
        self.chunks.iter().filter(|c| c.data.is_some()).count()
    }
}
