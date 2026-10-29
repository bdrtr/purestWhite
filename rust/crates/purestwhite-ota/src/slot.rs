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

/// An image being written into a slot.
pub struct SlotWriter<F, H> {
    flash: F,
    identify: Identify<H>,
    sector: [u8; SECTOR],
    filled: usize,
    /// Where the sector being filled goes.
    at: u32,
}

impl<F: NorFlash, H: Sha256> SlotWriter<F, H> {
    /// A writer into `flash`, the whole slot (an image may fill all of it).
    pub fn new(flash: F, hasher: H) -> Self {
        let limit = flash.capacity();
        SlotWriter {
            flash,
            identify: Identify::new(limit, hasher),
            sector: [0xFF; SECTOR],
            filled: 0,
            at: 0,
        }
    }

    /// Checks the next bytes of the image, then writes them, a sector at a time.
    ///
    /// # Errors
    /// The image is refused (nothing of `data` is written then), or the flash failed.
    pub fn write(&mut self, mut data: &[u8]) -> Result<(), WriteError<F::Error>> {
        self.identify.feed(data).map_err(WriteError::Image)?;
        while !data.is_empty() {
            let n = (SECTOR - self.filled).min(data.len());
            let (head, rest) = data.split_at(n);
            if let Some(to) = self.sector.get_mut(self.filled..self.filled + n) {
                to.copy_from_slice(head);
            }
            self.filled += n;
            data = rest;
            if self.filled == SECTOR {
                self.flush()?;
            }
        }
        Ok(())
    }

    /// Writes what is left, then checks the whole image and its digest: the image written, once
    /// it all is and it holds.
    ///
    /// # Errors
    /// The image ended early or is refused, its digest does not hold, or the flash failed.
    pub fn finish(mut self) -> Result<Identified, WriteError<F::Error>> {
        if self.filled > 0 {
            self.flush()?;
        }
        Ok(self.identify.finish()?)
    }

    /// Erases the sector at `at`, writes what was filled of it (padded with 0xFF to the flash's
    /// write size), and reads it back.
    fn flush(&mut self) -> Result<(), WriteError<F::Error>> {
        let len = self.filled.next_multiple_of(F::WRITE_SIZE).min(SECTOR);
        if let Some(pad) = self.sector.get_mut(self.filled..len) {
            pad.fill(0xFF);
        }
        let end = self.at + SECTOR_BYTES;
        self.flash.erase(self.at, end).map_err(WriteError::Flash)?;
        let bytes = self.sector.get(..len).unwrap_or_default();
        self.flash
            .write(self.at, bytes)
            .map_err(WriteError::Flash)?;
        self.read_back(len)?;
        self.at = end;
        self.filled = 0;
        Ok(())
    }

    fn read_back(&mut self, len: usize) -> Result<(), WriteError<F::Error>> {
        let mut back = [0u8; READ_BACK];
        let mut at = self.at;
        for written in self.sector.get(..len).unwrap_or_default().chunks(READ_BACK) {
            let back = back.get_mut(..written.len()).unwrap_or_default();
            self.flash.read(at, back).map_err(WriteError::Flash)?;
            if written != back {
                return Err(WriteError::ReadBack(at));
            }
            at += READ_BACK_BYTES;
        }
        Ok(())
    }
}
