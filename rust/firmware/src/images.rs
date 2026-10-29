//! App images on the SD card and in an OTA app slot, checked and hashed as they stream in: the
//! update gate's SD updater, its reading side (rust/firmware/DESIGN.md). Nothing here writes.
//! `identify_file` is async over the card, as `crate::storage` is: an image without an executor
//! drives it with `crate::ready` over the blocking bus.

use aligned::{A4, Aligned};
use embedded_storage::nor_flash::ReadNorFlash;
use purestwhite_fat::{BlockDevice, Volume};
use purestwhite_ota::digest::{DigestError, Identified, Identify, Sha256};
use purestwhite_ota::image::ImageError;
use purestwhite_ota::slot::SECTOR;

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
