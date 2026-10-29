//! App images on the SD card and in an OTA app slot, checked and hashed as they stream in, and a
//! card's image written into a slot: the update gate's SD updater (rust/firmware/DESIGN.md). Only
//! `install_file` writes. The card's functions are async, as `crate::storage` is: an image
//! without an executor drives them with `crate::ready` over the blocking bus.

use aligned::{A4, Aligned};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_bootloader_esp_idf::partitions::{self, FlashStorage, PartitionEntry};
use purestwhite_fat::{BlockDevice, Volume};
use purestwhite_ota::digest::{DigestError, Identified, Identify, Sha256};
use purestwhite_ota::image::{DIGEST, ImageError};
use purestwhite_ota::slot::{SECTOR, SlotWriter, WriteError};

/// How much is read at a time: a sector, word aligned (the card's DMA and esp-storage want it).
type Buf = Aligned<A4, [u8; SECTOR]>;

/// Why a file's image could not be identified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileError<E> {
    /// The file is not there, or could not be read.
    File(purestwhite_fat::Error<E>),
    Image(ImageError),
    /// The digest appended is not the image's.
    Digest,
}

impl<E> From<DigestError> for FileError<E> {
    fn from(e: DigestError) -> Self {
        match e {
            DigestError::Image(e) => FileError::Image(e),
            DigestError::Mismatch => FileError::Digest,
        }
    }
}

/// Why a slot's image could not be identified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotError<E> {
    /// Reading the flash failed at this offset.
    Flash(u32, E),
    Image(ImageError),
    /// The digest appended is not the image's.
    Digest,
}

impl<E> From<DigestError> for SlotError<E> {
    fn from(e: DigestError) -> Self {
        match e {
            DigestError::Image(e) => SlotError::Image(e),
            DigestError::Mismatch => SlotError::Digest,
        }
    }
}

/// The image in the file at `path` on `volume`: at most `limit` bytes (the slot it would go
/// to), and nothing after it in the file. `between` runs after each piece, to feed a watchdog.
///
/// # Errors
/// The file cannot be read, or its image is refused.
pub async fn identify_file<D: BlockDevice, H: Sha256>(
    volume: &mut Volume<D>,
    path: &str,
    limit: usize,
    hasher: H,
    mut between: impl FnMut(),
) -> Result<Identified, FileError<D::Error>> {
    let entry = volume.find(path).await.map_err(FileError::File)?;
    let mut file = volume.open(&entry).map_err(FileError::File)?;
    let mut identify = Identify::new(limit, hasher);
    let mut buf: Buf = Aligned([0; SECTOR]);
    loop {
        let n = volume
            .read(&mut file, &mut *buf)
            .await
            .map_err(FileError::File)?;
        if n == 0 {
            break;
        }
        identify
            .feed(buf.get(..n).unwrap_or_default())
            .map_err(FileError::Image)?;
        between();
    }
    Ok(identify.finish()?)
}

/// Why a file's image was not written whole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallError<E, F> {
    /// The file could not be read.
    File(purestwhite_fat::Error<E>),
    Write(WriteError<F>),
    /// What was written is whole, but not the image the first pass read: the file has changed.
    Changed,
}

/// Writes the image in the file at `path` on `volume` into `slot`, as big as the slot may be: the
/// SD updater's second pass, which must write the image the first pass (`identify_file`) read,
/// whose digest is `expected`. `between` runs after each piece, to feed a watchdog.
///
/// # Errors
/// The file cannot be read, the write fails, or what was written is not the first pass's image.
/// The slot then holds part of an image, unless the error came before its first sector was
/// written. After `Changed` the slot holds a whole image that passes, but not the one the first
/// pass read: the file changed on the card between the passes (nothing else writes the card
/// meanwhile, so only a card swapped mid-run gives that).
pub async fn install_file<D: BlockDevice, F: NorFlash, H: Sha256>(
    volume: &mut Volume<D>,
    path: &str,
    slot: &mut F,
    expected: &[u8; DIGEST],
    hasher: H,
    mut between: impl FnMut(),
) -> Result<Identified, InstallError<D::Error, F::Error>> {
    let entry = volume.find(path).await.map_err(InstallError::File)?;
    let mut file = volume.open(&entry).map_err(InstallError::File)?;
    let mut writer = SlotWriter::new(slot.capacity(), hasher);
    let mut buf: Buf = Aligned([0; SECTOR]);
    loop {
        let n = volume
            .read(&mut file, &mut *buf)
            .await
            .map_err(InstallError::File)?;
        if n == 0 {
            break;
        }
        writer
            .write(slot, buf.get(..n).unwrap_or_default())
            .map_err(InstallError::Write)?;
        between();
    }
    let written = writer.finish(slot).map_err(InstallError::Write)?;
    if written.digest != *expected {
        return Err(InstallError::Changed);
    }
    Ok(written)
}

/// The image at the start of `slot`, measured and hashed where it lies. `between` runs after
/// each piece, to feed a watchdog.
///
/// # Errors
/// The flash cannot be read, or the slot holds no image that passes.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the ESP32-S3's usize is 32 bits, and a slot is at most 16 MB"
)]
pub fn identify_slot<F: ReadNorFlash, H: Sha256>(
    slot: &mut F,
    hasher: H,
    mut between: impl FnMut(),
) -> Result<Identified, SlotError<F::Error>> {
    let capacity = slot.capacity();
    let mut identify = Identify::new(capacity, hasher);
    let mut buf: Buf = Aligned([0; SECTOR]);
    let mut at = 0;
    while !identify.is_done() && at < capacity {
        let piece = buf.get_mut(..SECTOR.min(capacity - at)).unwrap_or_default();
        slot.read(at as u32, piece)
            .map_err(|e| SlotError::Flash(at as u32, e))?;
        identify.feed_some(piece).map_err(SlotError::Image)?;
        at += SECTOR;
        between();
    }
    Ok(identify.finish()?)
}

/// The image at the start of the partition `entry` (an OTA app slot), measured and hashed where it
/// lies through the partition's own flash region (`identify_slot`).
///
/// # Errors
/// The region or the flash cannot be read (`SlotError::Flash`, at 0 for the region), or the slot
/// holds no image that passes.
pub fn identify_partition<H: Sha256>(
    storage: &mut FlashStorage<'_>,
    entry: PartitionEntry,
    hasher: H,
    between: impl FnMut(),
) -> Result<Identified, SlotError<partitions::Error>> {
    let mut region = entry.as_flash_region(storage);
    let mut flash = region.as_nor_flash().map_err(|e| SlotError::Flash(0, e))?;
    identify_slot(&mut flash, hasher, between)
}
