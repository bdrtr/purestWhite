//! Handing the next boot back to the C++ firmware: its OTA app slot selected in otadata before
//! anything else runs, and the bootloader's record of this image dropped.

use esp_bootloader_esp_idf::partitions::{
    FlashStorage, PARTITION_TABLE_MAX_LEN, PartitionEntry, read_partition_table,
};
use esp_hal::peripherals::{CPU_CTRL, FLASH};
use esp_hal::system::{Cpu, CpuControl};

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

// An otadata entry, as ESP-IDF's bootloader reads it: the highest sequence number among the two
// entries whose CRC holds and whose state is neither INVALID nor ABORTED selects app slot
// (seq - 1) % 2 here, with two slots.
const OTA_ENTRY: usize = 32;
const OTA_SECTOR: u32 = 0x1000;
const OTA_STATE_VALID: u32 = 2;
const OTA_STATE_INVALID: u32 = 3;
const OTA_STATE_ABORTED: u32 = 4;
const OTA_SEQ_ERASED: u32 = u32::MAX;

/// ESP-IDF's `esp_rom_crc32_le(UINT32_MAX, ...)` over an entry's sequence number.
fn seq_crc(seq: u32) -> u32 {
    let mut crc = 0u32;
    for b in seq.to_le_bytes() {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 == 1 { 0xEDB8_8320 } else { 0 };
        }
    }
    !crc
}

fn word(entry: &[u8; OTA_ENTRY], at: usize) -> u32 {
    let mut bytes = [0; 4];
    bytes.copy_from_slice(entry.get(at..at + 4).unwrap_or(&[0; 4]));
    u32::from_le_bytes(bytes)
}

/// The sequence number and state of an entry the bootloader would take.
fn usable(entry: &[u8; OTA_ENTRY]) -> Option<(u32, u32)> {
    let (seq, state, crc) = (word(entry, 0), word(entry, 24), word(entry, 28));
    (seq != OTA_SEQ_ERASED
        && crc == seq_crc(seq)
        && state != OTA_STATE_INVALID
        && state != OTA_STATE_ABORTED)
        .then_some((seq, state))
}

/// What `hand_back` found.
#[derive(Clone, Copy, Debug)]
pub struct HandBack {
    /// The state of the otadata entry this image was booted from: 1 (pending verify) means the
    /// bootloader rolls back on its own, 0 (new) that it does not. `None` when the entry in force
    /// does not select this image's slot.
    pub booted_state: Option<u32>,
}

/// The OTA app slot number of a partition (`ota_0` is 0), from its raw type, which unlike
/// `PartitionEntry::partition_type` does not panic on a type the crate does not know.
fn ota_slot(p: &PartitionEntry) -> Option<u32> {
    (p.raw_type() == 0 && (0x10..=0x1F).contains(&p.raw_subtype()))
        .then(|| u32::from(p.raw_subtype() - 0x10))
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
/// # Errors
/// What failed, when the partition table or otadata cannot be read or written, the table does
/// not have exactly the two OTA app slots the C++ firmware uses, or this image's slot is not one
/// of them.
pub fn hand_back(flash: FLASH<'_>, cpu: CPU_CTRL<'_>) -> Result<HandBack, &'static str> {
    forget_last_boot();
    // SAFETY: this code runs on the first core.
    unsafe { CpuControl::new(cpu).park_core(Cpu::AppCpu) };
    let mut storage = FlashStorage::new(flash);
    let (otadata, booted) = ota_partitions(&mut storage)?;
    let mut entries = [[0u8; OTA_ENTRY]; 2];
    for (at, entry) in [otadata, otadata + OTA_SECTOR]
        .into_iter()
        .zip(&mut entries)
    {
        storage.read(at, entry).map_err(|_| "otadata okunamadi")?;
    }
    let (found, write) = select_cpp(&entries, booted)?;
    if let Some(EntryWrite { sector, entry }) = write {
        write_entry(&mut storage, otadata + sector * OTA_SECTOR, &entry)?;
    }
    Ok(found)
}

/// Where otadata is, and the OTA app slot this image was booted from, once the partition table
/// has exactly `ota_0` and `ota_1`, once each.
fn ota_partitions(storage: &mut FlashStorage<'_>) -> Result<(u32, u32), &'static str> {
    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(storage, &mut table).map_err(|_| "bolum tablosu")?;
    let (mut slots, mut seen) = (0u32, 0u32);
    let mut otadata = None;
    for p in table.iter() {
        if let Some(slot) = ota_slot(&p) {
            slots += 1;
            seen |= 1 << slot;
        } else if p.raw_type() == 1 && p.raw_subtype() == 0 {
            otadata = Some(p.offset());
        }
    }
    if slots != 2 || seen != 0b11 {
        return Err("iki OTA yuvasi yok");
    }
    let otadata = otadata.ok_or("otadata yok")?;
    // The image the bootloader mapped, from the MMU.
    let booted = table
        .booted_partition()
        .ok()
        .flatten()
        .as_ref()
        .and_then(ota_slot)
        .ok_or("calisan yuva bilinmiyor")?;
    Ok((otadata, booted))
}

/// An otadata entry to write, and which of the two sectors it goes to (0 or 1).
struct EntryWrite {
    sector: u32,
    entry: [u8; OTA_ENTRY],
}

/// What `hand_back` finds in the two otadata `entries`, this image being in slot `booted`, and
/// the entry to write, unless the entry in force already selects the C++ slot, valid.
fn select_cpp(
    entries: &[[u8; OTA_ENTRY]; 2],
    booted: u32,
) -> Result<(HandBack, Option<EntryWrite>), &'static str> {
    let cpp = 1 - booted;
    let [first, second] = entries;
    // The entry in force, and the sector the new one goes to: the other one.
    let (active, target) = match (usable(first), usable(second)) {
        (Some(a), Some(b)) if b.0 > a.0 => (Some(b), 0),
        (Some(a), _) => (Some(a), 1),
        (None, Some(b)) => (Some(b), 0),
        (None, None) => (None, 0),
    };
    let found = HandBack {
        booted_state: active
            .filter(|&(seq, _)| slot_of(seq) == booted)
            .map(|(_, state)| state),
    };
    if let Some((seq, state)) = active
        && slot_of(seq) == cpp
        && state == OTA_STATE_VALID
    {
        return Ok((found, None));
    }
    let seq = next_seq(active, cpp).ok_or("otadata sira sayisi tasti")?;
    let write = EntryWrite {
        sector: target,
        entry: valid_entry(seq),
    };
    Ok((found, Some(write)))
}

/// The app slot an entry's sequence number selects.
fn slot_of(seq: u32) -> u32 {
    seq.wrapping_sub(1) % 2
}

/// The next sequence number after the entry in force that selects slot `cpp`; `None` past the
/// last, as the bootloader ignores an erased one.
fn next_seq(active: Option<(u32, u32)>, cpp: u32) -> Option<u32> {
    let mut seq = active.map_or(Some(1), |(seq, _)| seq.checked_add(1));
    if seq.is_some_and(|seq| slot_of(seq) != cpp) {
        seq = seq.and_then(|seq| seq.checked_add(1));
    }
    seq.filter(|&seq| seq != OTA_SEQ_ERASED)
}

/// A complete entry selecting `seq`, valid.
fn valid_entry(seq: u32) -> [u8; OTA_ENTRY] {
    let mut entry = [0xFFu8; OTA_ENTRY];
    for (at, value) in [(0, seq), (24, OTA_STATE_VALID), (28, seq_crc(seq))] {
        if let Some(field) = entry.get_mut(at..at + 4) {
            field.copy_from_slice(&value.to_le_bytes());
        }
    }
    entry
}

/// Writes `entry` at `at` and reads it back.
fn write_entry(
    storage: &mut FlashStorage<'_>,
    at: u32,
    entry: &[u8; OTA_ENTRY],
) -> Result<(), &'static str> {
    storage.write(at, entry).map_err(|_| "otadata yazilamadi")?;
    let mut back = [0u8; OTA_ENTRY];
    storage
        .read(at, &mut back)
        .map_err(|_| "otadata okunamadi")?;
    if back != *entry {
        return Err("otadata dogrulanamadi");
    }
    Ok(())
}
