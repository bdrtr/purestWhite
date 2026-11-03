//! ESP-IDF's certificate bundle (`esp_crt_bundle.c`): a list of offsets, then each root as its
//! subject name and public key, sorted by name so it is searched by halves.
//!
//! ```text
//! [offset of root 1: u32] ... [offset of root n: u32]   (little-endian; the first is 4 n)
//! [root 1] ... [root n]
//! root: [name length: u16] [key length: u16] [name: DER Name] [key: DER SubjectPublicKeyInfo]
//! ```

use core::cmp::Ordering;

/// The most roots a bundle may hold (the C++ firmware's
/// `CONFIG_MBEDTLS_CERTIFICATE_BUNDLE_MAX_CERTS`).
pub const ROOTS_MOST: usize = 200;
/// A root's two lengths.
const HEADER: usize = 4;

/// Why a bundle was refused (ESP-IDF's `esp_crt_check_bundle`, and every root inside it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleError {
    /// Too short for one offset and one root's lengths.
    Short,
    /// The first offset is not 4 times a count of roots inside the bundle.
    Offsets,
    /// More than `ROOTS_MOST` roots.
    TooMany,
    /// A root does not end where the next one starts, or the last one does not end at the
    /// bundle's end.
    Root,
}

/// A trusted root: its subject name and public key, as encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Root<'a> {
    pub name: &'a [u8],
    pub key: &'a [u8],
}

/// A bundle checked whole.
#[derive(Clone, Copy, Debug)]
pub struct Bundle<'a> {
    bytes: &'a [u8],
    count: usize,
}

impl<'a> Bundle<'a> {
    /// `bytes` checked as ESP-IDF checks a bundle; each root's end is checked too (the last one
    /// against the bundle's end, which ESP-IDF leaves unchecked).
    ///
    /// # Errors
    /// What is wrong with it.
    pub fn new(bytes: &'a [u8]) -> Result<Bundle<'a>, BundleError> {
        if bytes.len() <= 4 + HEADER {
            return Err(BundleError::Short);
        }
        let first = offset(bytes, 0).ok_or(BundleError::Short)?;
        if first == 0 || first % 4 != 0 || first >= bytes.len() {
            return Err(BundleError::Offsets);
        }
        let bundle = Bundle {
            bytes,
            count: first / 4,
        };
        if bundle.count > ROOTS_MOST {
            return Err(BundleError::TooMany);
        }
        for at in 0..bundle.count {
            let end = bundle.root_end(at).ok_or(BundleError::Root)?;
            let next = if at + 1 == bundle.count {
                Some(bytes.len())
            } else {
                offset(bytes, at + 1)
            };
            if next != Some(end) {
                return Err(BundleError::Root);
            }
        }
        Ok(bundle)
    }

    /// How many roots it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Root `at`, in name order.
    #[must_use]
    pub fn get(&self, at: usize) -> Option<Root<'a>> {
        if at >= self.count {
            return None;
        }
        let start = offset(self.bytes, at)?;
        let (name_len, key_len) = lengths(self.bytes, start)?;
        // Each sum checked: an offset near the top of `usize` (on 32 bits) must not wrap.
        let name_at = start.checked_add(HEADER)?;
        let key_at = name_at.checked_add(name_len)?;
        let name = self.bytes.get(name_at..key_at)?;
        let key = self.bytes.get(key_at..key_at.checked_add(key_len)?)?;
        Some(Root { name, key })
    }

    /// The root named `issuer` (a certificate's issuer, as encoded), searched by halves exactly as
    /// ESP-IDF's `esp_crt_find_cert` searches (the same middles, inclusive ends): names compared
    /// over the shorter one's length only, so a name that is the start of the other matches it
    /// (valid DER names of different lengths differ before that).
    #[must_use]
    pub fn find(&self, issuer: &[u8]) -> Option<Root<'a>> {
        if issuer.is_empty() {
            return None;
        }
        let mut start = 0isize;
        let mut end = isize::try_from(self.count).ok()? - 1;
        while start <= end {
            // Rounded towards zero, as C's `(start + end) / 2`.
            let middle = isize::midpoint(start, end);
            let root = self.get(usize::try_from(middle).ok()?)?;
            let len = issuer.len().min(root.name.len());
            match issuer.get(..len)?.cmp(root.name.get(..len)?) {
                Ordering::Equal => return Some(root),
                Ordering::Less => end = middle - 1,
                Ordering::Greater => start = middle + 1,
            }
        }
        None
    }

    /// Where root `at` ends.
    fn root_end(&self, at: usize) -> Option<usize> {
        let start = offset(self.bytes, at)?;
        let (name_len, key_len) = lengths(self.bytes, start)?;
        let end = start
            .checked_add(HEADER)?
            .checked_add(name_len)?
            .checked_add(key_len)?;
        (end <= self.bytes.len()).then_some(end)
    }
}

/// Offset `at` of the list.
fn offset(bytes: &[u8], at: usize) -> Option<usize> {
    let word = bytes.get(at * 4..at * 4 + 4)?;
    let word: [u8; 4] = word.try_into().ok()?;
    usize::try_from(u32::from_le_bytes(word)).ok()
}

/// The name and key lengths of the root at `start`.
fn lengths(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    let header = bytes.get(start..start.checked_add(HEADER)?)?;
    let [a, b, c, d] = header else {
        return None;
    };
    Some((
        usize::from(u16::from_le_bytes([*a, *b])),
        usize::from(u16::from_le_bytes([*c, *d])),
    ))
}
