//! An image's SHA-256, as ESP-IDF appends it: the digest of every byte before the last 32, which
//! are the digest itself. The hashing is the caller's ([`Sha256`]: on the device, the chip's SHA
//! unit); [`Identify`] checks an image and works out its digest without writing anything, which
//! is the first of the SD updater's two passes (see [`crate::slot`]).

use crate::image::{Checked, DIGEST, ImageCheck, ImageError};

#[cfg(test)]
mod tests;

/// A SHA-256 being computed.
pub trait Sha256 {
    fn update(&mut self, data: &[u8]);
    fn finish(self) -> [u8; DIGEST];
}

/// Why an image's digest is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigestError {
    Image(ImageError),
    /// The digest appended to the image is not its digest.
    Mismatch,
}

impl From<ImageError> for DigestError {
    fn from(e: ImageError) -> Self {
        DigestError::Image(e)
    }
}

/// A SHA-256 of all but the last 32 bytes fed to it, which it keeps.
#[derive(Clone, Debug)]
pub struct HeldBack<H> {
    hasher: H,
    tail: [u8; DIGEST],
    tail_len: usize,
}

impl<H: Sha256> HeldBack<H> {
    pub fn new(hasher: H) -> Self {
        HeldBack {
            hasher,
            tail: [0; DIGEST],
            tail_len: 0,
        }
    }

    /// Takes the next bytes: those that are no longer among the last 32 are hashed.
    pub fn update(&mut self, data: &[u8]) {
        let total = self.tail_len + data.len();
        let leaving = total.saturating_sub(DIGEST);
        let from_tail = leaving.min(self.tail_len);
        self.hasher
            .update(self.tail.get(..from_tail).unwrap_or_default());
        let from_data = leaving - from_tail;
        self.hasher
            .update(data.get(..from_data).unwrap_or_default());
        // What stays: the rest of the tail, then the rest of `data`.
        self.tail.copy_within(from_tail..self.tail_len, 0);
        self.tail_len -= from_tail;
        let rest = data.get(from_data..).unwrap_or_default();
        if let Some(to) = self.tail.get_mut(self.tail_len..self.tail_len + rest.len()) {
            to.copy_from_slice(rest);
        }
        self.tail_len += rest.len();
    }

    /// The digest of the bytes before the last 32, once it matches those 32.
    ///
    /// # Errors
    /// Fewer than 32 bytes came, or the last 32 are not the digest.
    pub fn finish(self) -> Result<[u8; DIGEST], DigestError> {
        let digest = self.hasher.finish();
        if self.tail_len == DIGEST && digest == self.tail {
            Ok(digest)
        } else {
            Err(DigestError::Mismatch)
        }
    }
}

/// An image that passed and its digest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Identified {
    pub checked: Checked,
    pub digest: [u8; DIGEST],
}

/// An image checked and hashed as it streams in, nothing written: what it is, before anything is
/// written for it.
#[derive(Clone, Debug)]
pub struct Identify<H> {
    check: ImageCheck,
    digest: HeldBack<H>,
}

impl<H: Sha256> Identify<H> {
    /// For a slot of `limit` bytes.
    pub fn new(limit: usize, hasher: H) -> Self {
        Identify {
            check: ImageCheck::new(limit),
            digest: HeldBack::new(hasher),
        }
    }

    /// Takes the next bytes of the image; only bytes that passed the check are hashed.
    ///
    /// # Errors
    /// As `ImageCheck::feed`.
    pub fn feed(&mut self, data: &[u8]) -> Result<(), ImageError> {
        self.check.feed(data)?;
        self.digest.update(data);
        Ok(())
    }

    /// As `feed`, but stops at the image's end: how many bytes of `data` were the image's (for an
    /// image read where it lies, as in an app slot, whose length is not known before).
    ///
    /// # Errors
    /// As `ImageCheck::feed_some`.
    pub fn feed_some(&mut self, data: &[u8]) -> Result<usize, ImageError> {
        let used = self.check.feed_some(data)?;
        self.digest.update(data.get(..used).unwrap_or_default());
        Ok(used)
    }

    /// Whether the whole image has come.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.check.is_done()
    }

    /// The image and its digest, once it has all come.
    ///
    /// # Errors
    /// As `ImageCheck::finish`, or the digest appended is not the image's.
    pub fn finish(self) -> Result<Identified, DigestError> {
        let checked = self.check.finish()?;
        let digest = self.digest.finish()?;
        Ok(Identified { checked, digest })
    }
}
