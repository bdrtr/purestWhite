//! Writing an app image into an OTA app slot as it streams in: the SD updater's second pass. The
//! slot is a [`NorFlash`] whose offsets start at the slot's: on the device, the partition's
//! region (`esp-bootloader-esp-idf`'s `FlashRegion`), which cannot write outside it. Each piece is
//! checked before any of it is written; each 4 KB sector is erased, written and read back before
//! the next. Nothing else is touched: the caller changes otadata only once [`SlotWriter::finish`]
//! has passed, so a write that fails leaves the device booting what it booted before.
//!
//! During phase 2 the slot written is the known-good C++ firmware's, so the file must be known
//! before its first byte is written: a first pass with [`crate::digest::Identify`], the digest
//! compared with the release's, then this pass, whose digest must be the first pass's (the file
//! read twice must not have changed).

use crate::digest::{DigestError, Identified, Identify, Sha256};
use crate::image::ImageError;
use embedded_storage::nor_flash::NorFlash;

#[cfg(test)]
mod tests;

/// A flash sector: the unit erased, written and read back at a time.
pub const SECTOR: usize = SECTOR_BYTES as usize;
const SECTOR_BYTES: u32 = 4096;
/// How much is read back and compared at a time.
const READ_BACK: usize = READ_BACK_BYTES as usize;
const READ_BACK_BYTES: u32 = 256;

/// Why a write failed. After `Flash` or `ReadBack` the slot holds part of the image: the caller
/// must not select it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteError<E> {
    Image(ImageError),
    /// The flash lent is not the size of the slot the writer was made for: nothing of this call
    /// was written.
    SlotSize,
    /// The digest appended is not the image's.
    Digest,
    Flash(E),
    /// What was read back at this offset is not what was written.
    ReadBack(u32),
}

impl<E> From<DigestError> for WriteError<E> {
    fn from(e: DigestError) -> Self {
        match e {
            DigestError::Image(e) => WriteError::Image(e),
            DigestError::Mismatch => WriteError::Digest,
        }
    }
}

/// A buffer the flash can take without copying it again (esp-storage copies through one of its
/// own when a buffer is not word aligned).
#[repr(C, align(4))]
struct Aligned<const N: usize>([u8; N]);

/// An image being written into a slot, the slot's flash lent to each call (so that the caller
/// may read from the same flash between them).
pub struct SlotWriter<H> {
    identify: Identify<H>,
    /// The slot's size: every flash lent must be the slot, of this size.
    capacity: usize,
    sector: Aligned<SECTOR>,
    filled: usize,
    /// Where the sector being filled goes.
    at: u32,
}

impl<H: Sha256> SlotWriter<H> {
    /// A writer into a slot of `capacity` bytes (an image may fill all of it).
    pub fn new(capacity: usize, hasher: H) -> Self {
        SlotWriter {
            identify: Identify::new(capacity, hasher),
            capacity,
            sector: Aligned([0xFF; SECTOR]),
            filled: 0,
            at: 0,
        }
    }

    /// Checks the next bytes of the image, then writes them into `flash` (the slot, of the
    /// capacity given), a sector at a time.
    ///
    /// # Errors
    /// The image is refused (nothing of `data` is written then), the flash lent is not the slot's
    /// size (nothing of `data` is written then), or the flash failed.
    pub fn write<F: NorFlash>(
        &mut self,
        flash: &mut F,
        mut data: &[u8],
    ) -> Result<(), WriteError<F::Error>> {
        self.check_size(flash)?;
        self.identify.feed(data).map_err(WriteError::Image)?;
        while !data.is_empty() {
            let n = (SECTOR - self.filled).min(data.len());
            let (head, rest) = data.split_at(n);
            if let Some(to) = self.sector.0.get_mut(self.filled..self.filled + n) {
                to.copy_from_slice(head);
            }
            self.filled += n;
            data = rest;
            if self.filled == SECTOR {
                self.flush(flash)?;
            }
        }
        Ok(())
    }

    /// Writes what is left, then checks the whole image and its digest: the image written, once
    /// it all is and it holds.
    ///
    /// # Errors
    /// The image ended early or is refused, its digest does not hold, the flash lent is not the
    /// slot's size, or the flash failed.
    pub fn finish<F: NorFlash>(
        mut self,
        flash: &mut F,
    ) -> Result<Identified, WriteError<F::Error>> {
        self.check_size(flash)?;
        if self.filled > 0 {
            self.flush(flash)?;
        }
        Ok(self.identify.finish()?)
    }

    /// Refuses a flash that is not the slot's size, before anything is written to it.
    fn check_size<F: NorFlash>(&self, flash: &F) -> Result<(), WriteError<F::Error>> {
        if flash.capacity() != self.capacity {
            return Err(WriteError::SlotSize);
        }
        Ok(())
    }

    /// Erases the sector at `at`, writes what was filled of it (padded with 0xFF to the flash's
    /// write size), and reads it back.
    fn flush<F: NorFlash>(&mut self, flash: &mut F) -> Result<(), WriteError<F::Error>> {
        let len = self.filled.next_multiple_of(F::WRITE_SIZE).min(SECTOR);
        if let Some(pad) = self.sector.0.get_mut(self.filled..len) {
            pad.fill(0xFF);
        }
        let end = self.at + SECTOR_BYTES;
        flash.erase(self.at, end).map_err(WriteError::Flash)?;
        let bytes = self.sector.0.get(..len).unwrap_or_default();
        flash.write(self.at, bytes).map_err(WriteError::Flash)?;
        self.read_back(flash, len)?;
        self.at = end;
        self.filled = 0;
        Ok(())
    }

    fn read_back<F: NorFlash>(
        &mut self,
        flash: &mut F,
        len: usize,
    ) -> Result<(), WriteError<F::Error>> {
        let mut back = Aligned([0u8; READ_BACK]);
        let mut at = self.at;
        for written in self
            .sector
            .0
            .get(..len)
            .unwrap_or_default()
            .chunks(READ_BACK)
        {
            let back = back.0.get_mut(..written.len()).unwrap_or_default();
            flash.read(at, back).map_err(WriteError::Flash)?;
            if written != back {
                return Err(WriteError::ReadBack(at));
            }
            at += READ_BACK_BYTES;
        }
        Ok(())
    }
}
