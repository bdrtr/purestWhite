//! Selecting this image's own slot for the next boot: the reverse of `crate::boot::hand_back`.
//! Only boot-check uses it, after clearing its own digest, to test the bootloader's check of
//! images.

use crate::boot::select_other_slot;
use crate::slots::ota_slots;
use esp_bootloader_esp_idf::partitions::FlashStorage;

/// Selects this image's own OTA slot for the next boot, valid. Only for an image that no longer
/// passes by then (boot-check clears its own appended digest first): a bootloader that checks
/// images refuses it and boots the C++ firmware; one that does not boots it again, and it hands
/// the next boot back at once, as every image does first.
///
/// # Errors
/// What failed, as `crate::boot::hand_back` says it.
pub fn select_this_image(storage: &mut FlashStorage<'_>) -> Result<(), &'static str> {
    let slots = ota_slots(storage)?;
    // The slot other than the other one: this image's own.
    select_other_slot(storage, slots.otadata, slots.booted_slot.other()).map(|_| ())
}
