//! The decoder's two queues, as the SDK keeps them: a ring of `N` slots that holds `N - 1`, and
//! drops what does not fit rather than wait.

/// A ring of `N` slots.
pub(crate) struct Ring<T: Copy + Default, const N: usize> {
    slots: [T; N],
    head: usize,
    tail: usize,
}

impl<T: Copy + Default, const N: usize> Ring<T, N> {
    pub(crate) fn new() -> Self {
        Ring {
            slots: [T::default(); N],
            head: 0,
            tail: 0,
        }
    }

    /// How many are queued.
    fn used(&self) -> usize {
        (self.head + N - self.tail) % N
    }

    /// Adds `item` only when more than `keep` slots are free, so at least `keep` stay free after
    /// it: whether it did. With
    /// `keep` 0, the SDK's key ring (`enqueue`); with more, its raw ring (`pushRawLocked`), which
    /// keeps room for the release a press will need.
    pub(crate) fn push(&mut self, item: T, keep: usize) -> bool {
        if N - 1 - self.used() <= keep {
            return false;
        }
        if let Some(slot) = self.slots.get_mut(self.head) {
            *slot = item;
        }
        self.head = (self.head + 1) % N;
        true
    }

    pub(crate) fn pop(&mut self) -> Option<T> {
        if self.head == self.tail {
            return None;
        }
        let item = self.slots.get(self.tail).copied();
        self.tail = (self.tail + 1) % N;
        item
    }
}
