//! What the smoke-test images for the second OTA slot share: the watchdog armed first, handing
//! the next boot back to the C++ firmware, the report they leave it in RTC slow memory, the
//! buttons and touch panel (`input`), the panel (`panel`), the SD card (`sd`) and deep sleep
//! (`sleep`).
#![no_std]
#![warn(clippy::pedantic)]

pub mod input;
pub mod panel;
pub mod sd;
pub mod sleep;

use core::pin::pin;
use core::task::{Context, Poll, Waker};

use esp_bootloader_esp_idf::partitions::{
    FlashStorage, PARTITION_TABLE_MAX_LEN, PartitionEntry, read_partition_table,
};
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{CPU_CTRL, FLASH, GPIO1, RTC_TIMER};
use esp_hal::psram::{FlashFreq, PsramConfig, PsramMode, PsramSize, SpiRamFreq};
use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};
use esp_hal::system::{Cpu, CpuControl};
use esp_hal::time::Duration;

/// Runs a future whose awaits are all ready at once; `None` if one is not.
pub fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

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

/// Where the report goes: the last 256 bytes of RTC slow memory, which the C++ firmware leaves
/// unused (it checks that at boot). lib/hal/HalSystem.cpp reads it with the same layout: magic,
/// length, then the text. A change of layout changes the magic.
const REPORT_ADDR: usize = 0x5000_1F00;
const REPORT_MAGIC: u32 = 0x5057_4254; // "PWBT"
const REPORT_TEXT: usize = 248;

/// Arms the RTC watchdog to reset the chip in 60 s, before anything that could hang: the device
/// has no reset button, its power button is read by the firmware. The reset spares the RTC
/// domain, so the report and the power latch's pad hold (see `hold_power_latch`) survive it.
#[must_use = "keep the RTC driver for the rest of the run"]
pub fn arm_watchdog(rtc_timer: RTC_TIMER<'_>) -> Rtc<'_> {
    let mut rtc = Rtc::new(rtc_timer);
    rtc.rwdt
        .set_timeout(RwdtStage::Stage0, Duration::from_secs(60));
    rtc.rwdt.enable();
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetCore);
    rtc
}

/// The PSRAM's clock, in MHz: 40, not the C++ firmware's 80. ESP-IDF tunes the sampling timing
/// at every boot for octal PSRAM above 40 MHz, and esp-hal 1.2.2 does not yet.
pub const PSRAM_MHZ: u32 = 40;

/// The X4 Pro's PSRAM: 8 MB octal, beside flash at 80 MHz, as the C++ firmware has it (the Arduino
/// core's `dio_opi` variant), at `PSRAM_MHZ`; its size is read from the chip.
#[must_use]
pub fn psram_config() -> PsramConfig {
    PsramConfig {
        mode: PsramMode::OctalSpi,
        size: PsramSize::AutoDetect,
        core_clock: None,
        flash_frequency: FlashFreq::FlashFreq80m,
        ram_frequency: SpiRamFreq::Freq40m,
    }
}

/// Raises the X4 Pro's power latch (GPIO1) and holds its pad: on battery the device turns off as
/// soon as the latch floats, and a system reset releases every pin it does not hold. The C++
/// firmware releases the hold at boot (`BoardConfig::holdPowerRails`).
#[must_use = "keep the pin for the rest of the run"]
pub fn hold_power_latch(pin: GPIO1<'_>) -> Output<'_> {
    let mut latch = Output::new(pin, Level::High, OutputConfig::default());
    latch.set_pad_hold(true);
    latch
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
    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = read_partition_table(&mut storage, &mut table).map_err(|_| "bolum tablosu")?;
    // Exactly ota_0 and ota_1, once each.
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
    let cpp = 1 - booted;

    let mut entries = [[0u8; OTA_ENTRY]; 2];
    for (at, entry) in [otadata, otadata + OTA_SECTOR]
        .into_iter()
        .zip(&mut entries)
    {
        storage.read(at, entry).map_err(|_| "otadata okunamadi")?;
    }
    let [first, second] = entries;
    // The entry in force, and the sector the new one goes to: the other one.
    let (active, target) = match (usable(&first), usable(&second)) {
        (Some(a), Some(b)) if b.0 > a.0 => (Some(b), 0),
        (Some(a), _) => (Some(a), 1),
        (None, Some(b)) => (Some(b), 0),
        (None, None) => (None, 0),
    };
    let slot_of = |seq: u32| seq.wrapping_sub(1) % 2;
    let booted_state = active
        .filter(|&(seq, _)| slot_of(seq) == booted)
        .map(|(_, state)| state);
    if let Some((seq, state)) = active
        && slot_of(seq) == cpp
        && state == OTA_STATE_VALID
    {
        return Ok(HandBack { booted_state });
    }
    // The next sequence number that selects the C++ slot; the bootloader ignores an erased one.
    let mut seq = active.map_or(Some(1), |(seq, _)| seq.checked_add(1));
    if seq.is_some_and(|seq| slot_of(seq) != cpp) {
        seq = seq.and_then(|seq| seq.checked_add(1));
    }
    let seq = seq
        .filter(|&seq| seq != OTA_SEQ_ERASED)
        .ok_or("otadata sira sayisi tasti")?;
    let mut entry = [0xFFu8; OTA_ENTRY];
    for (at, value) in [(0, seq), (24, OTA_STATE_VALID), (28, seq_crc(seq))] {
        if let Some(field) = entry.get_mut(at..at + 4) {
            field.copy_from_slice(&value.to_le_bytes());
        }
    }
    let at = otadata + target * OTA_SECTOR;
    storage
        .write(at, &entry)
        .map_err(|_| "otadata yazilamadi")?;
    let mut back = [0u8; OTA_ENTRY];
    storage
        .read(at, &mut back)
        .map_err(|_| "otadata okunamadi")?;
    if back != entry {
        return Err("otadata dogrulanamadi");
    }
    Ok(HandBack { booted_state })
}

/// The report's text, saved again after every stage so a run that dies halfway still says how far
/// it got. Text past its room is dropped and the last byte becomes `~`, so a cut report does not
/// read as a run that stopped.
pub struct Report {
    text: [u8; REPORT_TEXT],
    len: usize,
}

impl Default for Report {
    fn default() -> Self {
        Report {
            text: [0; REPORT_TEXT],
            len: 0,
        }
    }
}

impl core::fmt::Write for Report {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for &b in s.as_bytes() {
            if let Some(slot) = self.text.get_mut(self.len) {
                *slot = b;
                self.len += 1;
            } else if let Some(last) = self.text.last_mut() {
                *last = b'~';
            }
        }
        Ok(())
    }
}

impl Report {
    #[must_use]
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(self.text.get(..self.len).unwrap_or_default()).unwrap_or("")
    }

    /// Leaves the report for the C++ firmware's next boot.
    pub fn save(&self) {
        let base = REPORT_ADDR as *mut u32;
        // SAFETY: RTC slow memory is plain RAM that nothing else in these images uses at this
        // address; the C++ firmware reads it only after the next reset.
        #[allow(clippy::cast_possible_truncation)]
        unsafe {
            base.add(1).write_volatile(self.len as u32);
            let text = base.add(2).cast::<u8>();
            for (i, &b) in self.text.iter().enumerate() {
                text.add(i).write_volatile(b);
            }
            base.write_volatile(REPORT_MAGIC);
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }

    /// Adds a line to the report saved last, for a panic, which cannot reach the `Report`.
    pub fn append_saved(line: &str) {
        let base = REPORT_ADDR as *mut u32;
        // SAFETY: as `save`; the length is checked against the room before any write.
        unsafe {
            if base.read_volatile() != REPORT_MAGIC {
                return;
            }
            let mut len = base.add(1).read_volatile() as usize;
            let text = base.add(2).cast::<u8>();
            for &b in line.as_bytes() {
                if len >= REPORT_TEXT {
                    break;
                }
                text.add(len).write_volatile(b);
                len += 1;
            }
            #[allow(clippy::cast_possible_truncation)]
            base.add(1).write_volatile(len as u32);
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
}
