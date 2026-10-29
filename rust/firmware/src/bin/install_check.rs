//! The SD updater's install, on the device (rust/firmware/DESIGN.md, the SD updater): the C++
//! firmware's release on the card (`RECOVERY_FILE`) written into the C++ firmware's slot, within
//! phase 2's limits (`purestwhite_ota::plan`): only a release the updater knows, and only over the
//! same release, which proves the write path on the real slot and changes nothing, or over a slot
//! whose image no longer passes and which the bootloader has just refused. It hands the next boot
//! back first, as every image; identifies the slot and the card's release (the first pass, nothing
//! written); writes the release sector by sector, each read back (the second pass, whose digest
//! must be the first's); then identifies the slot once more and resets into the C++ firmware.
//!
//! A write cut short (power lost, a reset) leaves the C++ firmware's slot with no image that
//! passes. A bootloader that checks the image it boots then falls back to this one, which writes
//! the release again (seen in eink-qemu with the C++ build's bootloader). When that slot holds no
//! image that passes at the end (the card or its release missing, the write failed), this image
//! sleeps until Power or `SLEEP.timer` wakes it and then tries again, rather than reset straight
//! back into itself. After the title and `next boot`, the report gives the last line an earlier
//! run left when that run ended without the C++ firmware starting (a reset, a panic, a sleep; a
//! power loss clears it). A hand-back that fails while the slot is broken resets at once into this
//! image again (`crate::boot::start`). Runs in the emulator too (`emu/baremetal-install.txt`,
//! `emu/baremetal-install-refuse.txt`, `emu/baremetal-repair.txt`,
//! `emu/baremetal-reset-mid-write.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "the ESP32-S3's usize is 32 bits, and a slot is at most 16 MB"
)]
#![no_main]

use core::fmt::{Arguments, Write as _};
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::{Error as FlashError, FlashStorage, PartitionEntry};
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::peripherals::{GPIO40, GPIO41, GPIO42, SDHOST};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};
use esp_hal::sha::Sha;
use esp_hal::time::{Duration, Instant};
use purestwhite_firmware::board::{Board, CardParts};
use purestwhite_firmware::boot::{self, BoundedSleep, Report, Started, start};
use purestwhite_firmware::images::{SlotError, identify_file, identify_partition, install_file};
use purestwhite_firmware::ready;
use purestwhite_firmware::releases::{Named, RECOVERY_FILE, release};
use purestwhite_firmware::sd::{self, CardVolume};
use purestwhite_firmware::sha::HwSha;
use purestwhite_firmware::sleep::Rails;
use purestwhite_firmware::slots::ota_slots;
use purestwhite_ota::digest::Identified;
use purestwhite_ota::plan::{InSlot, Plan, plan};
use purestwhite_ota::slot::SECTOR;

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset. The bootloader then boots the C++ firmware, or
/// this image again when the C++ firmware's slot holds no image that passes.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
    esp_hal::system::software_reset()
}

const TITLE: &str = "purestWhite install";
/// How much of an earlier run's last line the report keeps: enough to tell which step it reached,
/// and room left (248 bytes) for a Repair that fails, then the sleep line with its marks and a
/// `power held` line (about 240 bytes in all).
const EARLIER: usize = 24;
/// The sleep when the C++ firmware's slot holds no image that passes: Power or the timer ends
/// it, and the bootloader comes back to this image.
const SLEEP: BoundedSleep = BoundedSleep {
    timer: Duration::from_secs(10 * 60),
    core: Duration::from_secs(12 * 60),
    system: Duration::from_secs(30),
    release: Duration::from_secs(10),
};

/// Adds `line` to the report, saved, and prints it.
fn note(report: &mut Report, line: Arguments<'_>) {
    let _ = report.write_fmt(line);
    let _ = writeln!(report);
    report.save();
    esp_println::println!("{line}");
}

/// What the run lends each step.
struct Run<'a, 'd> {
    storage: &'a mut FlashStorage<'d>,
    sha: &'a mut Sha<'d>,
    rtc: &'a mut Rtc<'d>,
    report: &'a mut Report,
}

impl Run<'_, '_> {
    /// The image in `target`, measured and hashed where it lies, and how long that took in ms.
    fn identify(&mut self, target: PartitionEntry) -> (Found, u64) {
        let start = Instant::now();
        let rtc = &mut *self.rtc;
        let found = identify_partition(self.storage, target, HwSha::new(self.sha), || {
            rtc.rwdt.feed();
        });
        (found, start.elapsed().as_millis())
    }

    /// What `target` (the C++ firmware's slot) holds, noted.
    fn slot_state(&mut self, target: PartitionEntry) -> InSlot {
        let (found, ms) = self.identify(target);
        match &found {
            Ok(image) => {
                let (len, named) = (image.checked.len, Named(&image.digest));
                note(self.report, format_args!("slot {len} B {ms} ms {named}"));
            }
            Err(SlotError::Flash(at, e)) => {
                note(self.report, format_args!("slot: read at {at:x}: {e:?}"));
            }
            Err(e) => note(self.report, format_args!("slot: {e:?}")),
        }
        in_slot(&found)
    }

    /// The release on the card, at most `limit` bytes (the slot it would go to), checked and
    /// hashed as it streams in (the first pass), noted.
    fn card_release(&mut self, volume: &mut CardVolume<'_>, limit: usize) -> Option<Identified> {
        let start = Instant::now();
        let rtc = &mut *self.rtc;
        let sha = HwSha::new(self.sha);
        let found = ready(identify_file(volume, RECOVERY_FILE, limit, sha, || {
            rtc.rwdt.feed();
        }));
        let ms = start.elapsed().as_millis();
        match found {
            Some(Ok(image)) => {
                let (len, named) = (image.checked.len, Named(&image.digest));
                note(self.report, format_args!("card {len} B {ms} ms {named}"));
                Some(image)
            }
            Some(Err(e)) => {
                note(self.report, format_args!("card: {e:?}"));
                None
            }
            None => {
                note(self.report, format_args!("card: not ready"));
                None
            }
        }
    }

    /// Writes the card's release, as the first pass found it (`card`), into `target` (the second
    /// pass), noting how it went after `plan`'s name.
    fn write(
        &mut self,
        volume: &mut CardVolume<'_>,
        target: PartitionEntry,
        card: &Identified,
        plan: Plan,
    ) {
        let start = Instant::now();
        let mut region = target.as_flash_region(self.storage);
        let rtc = &mut *self.rtc;
        let written = match region.as_nor_flash() {
            Ok(mut flash) => {
                let sha = HwSha::new(self.sha);
                ready(install_file(
                    volume,
                    RECOVERY_FILE,
                    &mut flash,
                    &card.digest,
                    sha,
                    || {
                        rtc.rwdt.feed();
                    },
                ))
            }
            Err(e) => return note(self.report, format_args!("region: {e:?}")),
        };
        let ms = start.elapsed().as_millis();
        let sectors = card.checked.len.div_ceil(SECTOR);
        match written {
            Some(Ok(_)) => note(
                self.report,
                format_args!("{plan:?}: {sectors} sectors {ms} ms"),
            ),
            Some(Err(e)) => note(self.report, format_args!("{plan:?}: {e:?}")),
            None => note(self.report, format_args!("{plan:?}: not ready")),
        }
    }

    /// What `target` holds after the write: noted when it is an image that passes
    /// (`installed: same digest` for the card's release); otherwise the write's own line says why,
    /// and the sleep line follows.
    fn installed(&mut self, target: PartitionEntry, card: &Identified) -> InSlot {
        let (found, _) = self.identify(target);
        match &found {
            Ok(image) if image.digest == card.digest => {
                note(self.report, format_args!("installed: same digest"));
            }
            Ok(image) => note(
                self.report,
                format_args!("installed: {}", Named(&image.digest)),
            ),
            Err(_) => {}
        }
        in_slot(&found)
    }
}

/// A slot's image, identified where it lies.
type Found = Result<Identified, SlotError<FlashError>>;

/// What a slot holds, as `found` says.
fn in_slot(found: &Found) -> InSlot {
    match found {
        Ok(image) => InSlot::Image(image.digest),
        Err(SlotError::Flash(..)) => InSlot::Unreadable,
        Err(_) => InSlot::Broken,
    }
}

/// The card's release written into the C++ firmware's slot when the plan allows it (`fell_back`:
/// the bootloader refused that slot and started this image): what that slot holds at the end.
fn install(
    run: &mut Run<'_, '_>,
    sdhost: SDHOST<'_>,
    pins: (GPIO41<'_>, GPIO42<'_>, GPIO40<'_>),
    supply: &mut Output<'_>,
    fell_back: bool,
) -> InSlot {
    let target = match ota_slots(run.storage) {
        Ok(slots) => slots.other,
        Err(e) => {
            note(run.report, format_args!("slot: {e}"));
            return InSlot::Unreadable;
        }
    };
    let in_slot = run.slot_state(target);
    let Ok(controller) = SdHostController::new(sdhost, SdConfig::default()) else {
        note(run.report, format_args!("card: no controller"));
        return in_slot;
    };
    let rtc = &mut *run.rtc;
    let mut volume = match sd::open_volume(&controller, pins, supply, |_| rtc.rwdt.feed()) {
        Ok(volume) => volume,
        Err(e) => {
            note(run.report, format_args!("card: {e:?}"));
            return in_slot;
        }
    };
    let Some(card) = run.card_release(&mut volume, target.len() as usize) else {
        return in_slot;
    };
    let known = release(&card.digest).is_some();
    match plan(&card.digest, known, in_slot, fell_back) {
        Plan::Refuse(why) => {
            note(run.report, format_args!("refused: {why:?}"));
            in_slot
        }
        plan => {
            run.write(&mut volume, target, &card, plan);
            run.installed(target, &card)
        }
    }
}

/// Notes the last line an earlier run left, cut to `EARLIER` bytes.
fn note_earlier(report: &mut Report, earlier: &Report) {
    let text = earlier.as_str().trim_end();
    let last = text.rsplit('\n').next().unwrap_or(text);
    let last = last.get(..EARLIER).unwrap_or(last);
    note(report, format_args!("earlier: {last}"));
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        earlier,
        found,
        flash: mut storage,
    } = start(TITLE, board.boot);
    // `note` prints each line it adds; these are `start`'s.
    esp_println::println!("{}", report.as_str().trim_end());
    if let Some(earlier) = earlier.filter(|r| r.as_str().starts_with(TITLE)) {
        note_earlier(&mut report, &earlier);
    }
    let mut sha = Sha::new(board.sha);
    let CardParts {
        sdhost,
        supply,
        clk,
        cmd,
        dat0,
    } = board.card;
    // Off until its first try (`sd::mount`).
    let mut sd_power = Output::new(supply, Level::High, OutputConfig::default());
    let mut run = Run {
        storage: &mut storage,
        sha: &mut sha,
        rtc: &mut rtc,
        report: &mut report,
    };
    let pins = (clk, cmd, dat0);
    let ends = install(&mut run, sdhost, pins, &mut sd_power, found.fell_back());
    if matches!(ends, InSlot::Image(_)) {
        note(&mut report, format_args!("done"));
        esp_hal::system::software_reset()
    }
    let rails = Rails {
        touch_power: Output::new(board.i2c.supply, Level::High, OutputConfig::default()),
        sd_power,
        panel_reset: Output::new(board.panel.rst, Level::High, OutputConfig::default()),
    };
    let power = Input::new(
        board.buttons.power,
        InputConfig::default().with_pull(Pull::Up),
    );
    let low_power = LowPower::new(board.low_power);
    let why = "no C++ image";
    boot::sleep_when_released(&mut report, why, &mut rtc, power, rails, low_power, SLEEP)
}
