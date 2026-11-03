//! Bounded text without an allocator.

/// Up to `N` bytes of text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Text<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    /// Empty text.
    #[must_use]
    pub const fn new() -> Self {
        Text {
            bytes: [0; N],
            len: 0,
        }
    }

    /// `bytes` as text; `None` when they do not fit.
    #[must_use]
    pub(crate) fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut text = Self::new();
        text.bytes.get_mut(..bytes.len())?.copy_from_slice(bytes);
        text.len = bytes.len();
        Some(text)
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or_default()
    }

    /// How many bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The text as UTF-8, if it is.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        core::str::from_utf8(self.as_bytes()).ok()
    }

    /// Each byte masked (XOR) with `key`'s, in turn (the C++ firmware's `xorTransform`).
    pub(crate) fn xor(&mut self, key: &[u8]) {
        for (byte, k) in self.bytes.iter_mut().take(self.len).zip(key.iter().cycle()) {
            *byte ^= k;
        }
    }
}

/// The length only: a `Text` may hold a password.
impl<const N: usize> core::fmt::Debug for Text<N> {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(out, "Text({} bytes)", self.len)
    }
}

impl<const N: usize> Default for Text<N> {
    fn default() -> Self {
        Self::new()
    }
}
