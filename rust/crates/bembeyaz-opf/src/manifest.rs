//! The manifest items the spine looks up: each id with its resolved href, the first item of an id
//! only, found through an open-addressing table.

use alloc::vec::Vec;

/// Where an item's id and href are in `Manifest::bytes`.
struct Item {
    start: usize,
    id_len: usize,
    href_len: usize,
}

pub(crate) struct Manifest {
    /// Each item's id, then its href.
    bytes: Vec<u8>,
    items: Vec<Item>,
    /// Index + 1 into `items`, or 0 for an empty slot. Its length is a power of two, and it is at
    /// most half full.
    slots: Vec<u32>,
}

use crate::Full;

fn hash(id: &[u8]) -> u32 {
    id.iter().fold(2_166_136_261, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    })
}

impl Manifest {
    pub(crate) const fn new() -> Self {
        Manifest {
            bytes: Vec::new(),
            items: Vec::new(),
            slots: Vec::new(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.bytes.clear();
        self.items.clear();
        self.slots.clear();
    }

    fn id(&self, item: &Item) -> &[u8] {
        self.bytes
            .get(item.start..item.start + item.id_len)
            .unwrap_or_default()
    }

    /// The slot that holds `id`, or the empty one where it would go.
    fn slot(&self, id: &[u8]) -> Option<usize> {
        let mask = self.slots.len().checked_sub(1)?;
        let mut at = hash(id) as usize & mask;
        loop {
            match self.slots.get(at).copied()? {
                0 => return Some(at),
                n => {
                    let item = self.items.get(n as usize - 1)?;
                    if self.id(item) == id {
                        return Some(at);
                    }
                }
            }
            at = (at + 1) & mask;
        }
    }

    pub(crate) fn find(&self, id: &[u8]) -> Option<&[u8]> {
        let n = *self.slots.get(self.slot(id)?)?;
        let item = self.items.get((n as usize).checked_sub(1)?)?;
        let href = item.start + item.id_len;
        self.bytes.get(href..href + item.href_len)
    }

    /// The memory the manifest keeps with `bytes` more text, `items` items and `slots` slots.
    fn size(&self, bytes: usize, items: usize, slots: usize) -> usize {
        self.bytes.len() + bytes + items * size_of::<Item>() + slots * size_of::<u32>()
    }

    /// Adds an item unless an earlier one has its id. `limit` bounds the ids, hrefs and index
    /// kept; the vectors grow by doubling, so what they allocate can reach about twice that.
    pub(crate) fn add(&mut self, id: &[u8], href: &[u8], limit: usize) -> Result<(), Full> {
        if (self.items.len() + 1) * 2 > self.slots.len() {
            let size = (self.slots.len() * 2).max(16);
            if self.size(0, self.items.len(), size) > limit {
                return Err(Full);
            }
            self.grow(size)?;
        }
        let at = self.slot(id).ok_or(Full)?;
        if self.slots.get(at).copied() != Some(0) {
            return Ok(());
        }
        let len = id.len() + href.len();
        if self.size(len, self.items.len() + 1, self.slots.len()) > limit
            || self.bytes.try_reserve(len).is_err()
            || self.items.try_reserve(1).is_err()
        {
            return Err(Full);
        }
        let n = u32::try_from(self.items.len() + 1).map_err(|_| Full)?;
        self.items.push(Item {
            start: self.bytes.len(),
            id_len: id.len(),
            href_len: href.len(),
        });
        self.bytes.extend_from_slice(id);
        self.bytes.extend_from_slice(href);
        if let Some(slot) = self.slots.get_mut(at) {
            *slot = n;
        }
        Ok(())
    }

    fn grow(&mut self, size: usize) -> Result<(), Full> {
        let mut slots = Vec::new();
        slots.try_reserve_exact(size).map_err(|_| Full)?;
        slots.resize(size, 0);
        let old = core::mem::replace(&mut self.slots, slots);
        for n in old.into_iter().filter(|&n| n != 0) {
            let item = self.items.get(n as usize - 1).ok_or(Full)?;
            let at = self.slot(self.id(item)).ok_or(Full)?;
            if let Some(slot) = self.slots.get_mut(at) {
                *slot = n;
            }
        }
        Ok(())
    }
}
