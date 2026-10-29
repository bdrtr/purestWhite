//! Handing the next boot back to the C++ firmware: its OTA app slot selected in otadata before
//! anything else runs (the choice is `purestwhite_ota`'s; the partition table and the flash are
//! read and written here), and the bootloader's record of this image dropped.

use crate::slots::ota_slots;
use esp_bootloader_esp_idf::partitions::FlashStorage;
use esp_hal::peripherals::CPU_CTRL;
use esp_hal::system::{Cpu, CpuControl};
use purestwhite_ota as ota;

/// The CRC of the record the ESP-IDF bootloader keeps at the top of RTC fast memory (its
/// `rtc_retain_mem_t`, 16 bytes at `SOC_RTC_DRAM_HIGH - 16` on the ESP32-S3): the app partition
/// it booted last, which with `CONFIG_BOOTLOADER_SKIP_VALIDATE_IN_DEEP_SLEEP` it boots again after
/// a deep sleep, without reading otadata. The address holds while sdkconfig.defaults keeps that
/// option, `CONFIG_BOOTLOADER_RESERVE_RTC_SIZE=0x10` without a custom reserve, and secure boot off
/// (its lines 387-401).
const BOOTLOADER_RETAIN_CRC: usize = 0x600F_FFFC;

/// Makes the bootloader choose the app after a deep sleep from otadata rather than boot this
/// image again: it drops the record of the app booted last, whose CRC it then finds unset (all
/// ones). The bootloader writes the record only while it boots, so once is enough.
fn forget_last_boot() {
    // SAFETY: the address is the last word of RTC fast memory, which these images do not use
    // (esp-hal places its RTC fast data from the start); only the bootloader reads it, at the next
    // reset.
    unsafe {
        (BOOTLOADER_RETAIN_CRC as *mut u32).write_volatile(u32::MAX);
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
}

/// What `hand_back` found.
#[derive(Clone, Copy, Debug)]
pub struct HandBack {
    /// The state of the otadata entry this image was booted from: 1 (pending verify) means the
    /// bootloader rolls back on its own, 0 (new) that it does not. `None` when the entry in force
    /// does not select this image's slot.
    pub booted_state: Option<u32>,
}

/// Selects the C++ firmware, the OTA app slot this image was not booted from, for the next boot,
/// valid. Done first, before anything that could fail: the device's bootloader may not roll back
/// on its own, so from here on any reset, panic, watchdog or deep-sleep wake lands in the C++
/// firmware (for the wake, see `forget_last_boot`).
///
/// Like the C++ firmware's own switch (src/network/OtaBootSwitch.cpp), it writes one complete
/// entry into the other otadata sector, so a power loss mid-write leaves the entry in force. It
/// parks the second core for good first: a restart from the C++ firmware can leave it running
/// that firmware's code, and a flash write would refuse to run beside it. These images use one
/// core; call it from the first.
///
/// `storage` must have been made before any write to the flash, and is to be kept for the rest of
/// the run (see `crate::boot::start`).
///
/// # Errors
/// What failed, when the partition table or otadata cannot be read or written, the table does
/// not have exactly the two OTA app slots the C++ firmware uses, or this image's slot is not one
/// of them.
pub fn hand_back(
    storage: &mut FlashStorage<'_>,
    cpu: CPU_CTRL<'_>,
) -> Result<HandBack, &'static str> {
    forget_last_boot();
    // SAFETY: this code runs on the first core.
    unsafe { CpuControl::new(cpu).park_core(Cpu::AppCpu) };
    let slots = ota_slots(storage)?;
    let (otadata, booted) = (slots.otadata, slots.booted_slot);
    let mut entries = [[0u8; ota::ENTRY]; 2];
    for (at, entry) in [otadata, otadata + ota::SECTOR]
        .into_iter()
        .zip(&mut entries)
    {
        storage.read(at, entry).map_err(|_| "otadata read failed")?;
    }
    let selection = ota::select_other(&entries, booted).map_err(|_| "otadata sequence ran out")?;
    if let Some(write) = selection.write {
        write_entry(storage, otadata + write.sector * ota::SECTOR, &write.entry)?;
    }
    Ok(HandBack {
        booted_state: selection.booted_state,
    })
}

/// Writes `entry` at `at` and reads it back.
fn write_entry(
    storage: &mut FlashStorage<'_>,
    at: u32,
    entry: &ota::Entry,
) -> Result<(), &'static str> {
    storage
        .write(at, entry)
        .map_err(|_| "otadata write failed")?;
    let mut back = [0u8; ota::ENTRY];
    storage
        .read(at, &mut back)
        .map_err(|_| "otadata read failed")?;
    if back != *entry {
        return Err("otadata check failed");
    }
    Ok(())
}
