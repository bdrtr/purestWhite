//! The device's two OTA app slots and otadata, from its partition table: what the hand-back, the
//! update gate's SD updater and the boot counter all need, whatever the phase. The device keeps
//! the table it left the factory with (rust/firmware/DESIGN.md), so nothing here assumes
//! `partitions.csv`.

use esp_bootloader_esp_idf::partitions::{
    FlashStorage, PARTITION_TABLE_MAX_LEN, PartitionEntry, read_partition_table,
};
use purestwhite_ota::Slot;

/// The OTA app slot number of a partition (`ota_0` is 0), from its raw type, which unlike
/// `PartitionEntry::partition_type` does not panic on a type the crate does not know.
fn ota_slot(p: &PartitionEntry) -> Option<u32> {
    (p.raw_type() == 0 && (0x10..=0x1F).contains(&p.raw_subtype()))
        .then(|| u32::from(p.raw_subtype() - 0x10))
}

/// The two OTA app slots and otadata, from the partition table.
#[derive(Clone, Copy)]
pub struct OtaSlots {
    /// The slot this image was booted from, as the MMU maps it.
    pub booted: PartitionEntry,
    /// Which of the two it is.
    pub booted_slot: Slot,
    /// The other one: the C++ firmware's, while it is the known-good one.
    pub other: PartitionEntry,
    pub otadata: u32,
}

/// The OTA app slots and otadata, once the partition table has exactly `ota_0` and `ota_1`, once
/// each, and the image was booted from one of them.
///
/// # Errors
/// What is wrong with the table.
pub fn ota_slots(storage: &mut FlashStorage<'_>) -> Result<OtaSlots, &'static str> {
    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(storage, &mut table).map_err(|_| "partition table")?;
    let mut slots: [Option<PartitionEntry>; 2] = [None, None];
    let mut otadata = None;
    for p in table.iter() {
        if let Some(slot) = ota_slot(&p).and_then(|n| slots.get_mut(n as usize)) {
            if slot.replace(p).is_some() {
                return Err("not two OTA slots");
            }
        } else if ota_slot(&p).is_some() {
            return Err("not two OTA slots");
        } else if p.raw_type() == 1 && p.raw_subtype() == 0 {
            otadata = Some(p.offset());
        }
    }
    let [Some(ota_0), Some(ota_1)] = slots else {
        return Err("not two OTA slots");
    };
    let otadata = otadata.ok_or("no otadata")?;
    // The image the bootloader mapped, from the MMU.
    let booted = table
        .booted_partition()
        .ok()
        .flatten()
        .ok_or("running slot unknown")?;
    let (booted, other, booted_slot) = match ota_slot(&booted) {
        Some(0) => (ota_0, ota_1, Slot::Ota0),
        Some(1) => (ota_1, ota_0, Slot::Ota1),
        _ => return Err("running slot unknown"),
    };
    Ok(OtaSlots {
        booted,
        booted_slot,
        other,
        otadata,
    })
}
