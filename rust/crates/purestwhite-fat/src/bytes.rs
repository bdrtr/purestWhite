//! Little-endian numbers read from on-disk structures, 0 past their end.

/// A little-endian `u16` at `at`, or 0 past the end.
pub(crate) fn u16_at(bytes: &[u8], at: usize) -> u16 {
    bytes
        .get(at..at + 2)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u16::from_le_bytes)
}

/// A little-endian `u32` at `at`, or 0 past the end.
pub(crate) fn u32_at(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .and_then(|b| b.try_into().ok())
        .map_or(0, u32::from_le_bytes)
}

/// The byte at `at`, or 0 past the end.
pub(crate) fn byte_at(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}
