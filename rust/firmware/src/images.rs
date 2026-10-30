//! An OTA app slot's image identified through the device's partition table: the slot as a flash of
//! its own (`esp-bootloader-esp-idf`'s region, which cannot reach outside it), measured and hashed
//! by `purestwhite_updater::images::identify_slot`.

use esp_bootloader_esp_idf::partitions::{self, FlashStorage, PartitionEntry};
use purestwhite_ota::digest::{Identified, Sha256};
use purestwhite_updater::images::{SlotError, identify_slot};

/// The image at the start of the partition `entry` (an OTA app slot), measured and hashed where it
/// lies through the partition's own flash region. `between` runs after each piece, to feed a
/// watchdog.
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
