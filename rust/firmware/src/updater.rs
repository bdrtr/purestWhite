//! The SD updater (rust/firmware/DESIGN.md, the SD updater): the C++ firmware's release on the card
//! (`RECOVERY_FILE`) written into the C++ firmware's slot, within phase 2's limits
//! (`purestwhite_ota::plan`): only a release the updater knows, and only over the same release,
//! which proves the write path on the real slot and changes nothing, or over a slot whose image no
//! longer passes and which the bootloader has just refused. It identifies the slot and the card's
//! release (the first pass, nothing written), writes the release sector by sector, each read back
//! (the second pass, whose digest must be the first's), and identifies the slot once more
//! (`install`). Then `end` resets into the C++ firmware (`purestwhite_ota::plan::resets`), or
//! sleeps (bounded) until Power or a timer rather than reset straight back into the image that
//! runs the updater: a bootloader that checks images falls back to that image while the slot holds
//! none, or keeps refusing an image the updater's own check passes. Used by install-check and by
//! the recovery mode of the `purestwhite` image.
#![allow(
    clippy::cast_possible_truncation,
    reason = "the ESP32-S3's usize is 32 bits, and a slot is at most 16 MB"
)]

use crate::board::CardParts;
use crate::boot::{self, BoundedSleep, Report};
use crate::images::{SlotError, identify_file, identify_partition, install_file};
use crate::ready;
use crate::releases::{Named, RECOVERY_FILE, release};
use crate::sd::{self, CardVolume};
use crate::sha::HwSha;
use crate::sleep::Rails;
use crate::slots::ota_slots;
use esp_bootloader_esp_idf::partitions::{Error as FlashError, FlashStorage, PartitionEntry};
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::peripherals::{GPIO2, GPIO3, GPIO14, GPIO40, GPIO41, GPIO42, LPWR, SDHOST, SHA};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};
use esp_hal::sha::Sha;
use esp_hal::time::{Duration, Instant};
use purestwhite_ota::digest::Identified;
use purestwhite_ota::plan::{InSlot, Plan, plan, resets};
use purestwhite_ota::slot::SECTOR;

/// The sleep when a reset would only bring the bootloader back to the image that runs the
/// updater: Power or the timer ends it, and the bootloader decides again.
const SLEEP: BoundedSleep = BoundedSleep {
    timer: Duration::from_secs(10 * 60),
    core: Duration::from_secs(12 * 60),
    system: Duration::from_secs(30),
    release: Duration::from_secs(10),
};

/// The parts the updater takes: the card, the SHA unit, and what `end`'s sleep holds and wakes
/// on (the touch panel's supply, the panel's reset line, Power, the RTC's low-power control).
pub struct Parts {
    pub card: CardParts,
    pub sha: SHA<'static>,
    pub touch_supply: GPIO2<'static>,
    pub panel_reset: GPIO14<'static>,
    pub power: GPIO3<'static>,
    pub low_power: LPWR<'static>,
}

/// What `install` leaves `end`: whether to reset into the C++ firmware
/// (`purestwhite_ota::plan::resets`), and the parts the sleep needs otherwise.
pub struct Finished {
    pub resets: bool,
    sd_power: Output<'static>,
    touch_supply: GPIO2<'static>,
    panel_reset: GPIO14<'static>,
    power: GPIO3<'static>,
    low_power: LPWR<'static>,
}

/// The updater, its steps noted on `report`: the card's release written into the C++ firmware's
/// slot when the plan allows it (`fell_back`: the bootloader refused that slot and started the
/// image that runs the updater). Nothing is written before both are identified.
pub fn install(
    parts: Parts,
    storage: &mut FlashStorage<'_>,
    rtc: &mut Rtc<'_>,
    report: &mut Report,
    fell_back: bool,
) -> Finished {
    let mut sha = Sha::new(parts.sha);
    let CardParts {
        sdhost,
        supply,
        clk,
        cmd,
        dat0,
    } = parts.card;
    // Off until its first try (`sd::mount`).
    let mut sd_power = Output::new(supply, Level::High, OutputConfig::default());
    let mut run = Updater {
        storage,
        sha: &mut sha,
        rtc,
        report,
    };
    let (after, wrote) = update(&mut run, sdhost, (clk, cmd, dat0), &mut sd_power, fell_back);
    Finished {
        resets: resets(after, wrote, fell_back),
        sd_power,
        touch_supply: parts.touch_supply,
        panel_reset: parts.panel_reset,
        power: parts.power,
        low_power: parts.low_power,
    }
}

/// After `install`: `done` and a reset into the C++ firmware when it `resets`; otherwise the
/// bounded sleep (`SLEEP`), noted `no C++ image`, after which the bootloader decides again.
pub fn end(report: &mut Report, rtc: &mut Rtc<'_>, finished: Finished) -> ! {
    if finished.resets {
        report.note(format_args!("done"));
        esp_hal::system::software_reset()
    }
    let rails = Rails {
        touch_power: Output::new(finished.touch_supply, Level::High, OutputConfig::default()),
        sd_power: finished.sd_power,
        panel_reset: Output::new(finished.panel_reset, Level::High, OutputConfig::default()),
    };
    let power = Input::new(finished.power, InputConfig::default().with_pull(Pull::Up));
    let low_power = LowPower::new(finished.low_power);
    let why = "no C++ image";
    boot::sleep_when_released(report, why, rtc, power, rails, low_power, SLEEP)
}

/// How much of an earlier run's last line `note_earlier` keeps: enough to tell which step it
/// reached, with room left in install-check's report (248 bytes) for a repair that fails and its
/// sleep line. The `purestwhite` image's recovery report, with its `runs:` and `recovery:` lines,
/// has no room for it.
const EARLIER: usize = 24;

/// Notes the last line an earlier run of the same image left (`earlier`, as
/// `crate::boot::Started` gives it, kept when its first line is this report's), cut to 24 bytes:
/// which step a run that ended before the C++ firmware started had reached.
pub fn note_earlier(report: &mut Report, earlier: Option<Report>) {
    let title = report.as_str().lines().next().unwrap_or_default();
    let Some(earlier) = earlier.filter(|e| e.as_str().lines().next() == Some(title)) else {
        return;
    };
    let text = earlier.as_str().trim_end();
    let last = text.rsplit('\n').next().unwrap_or(text);
    let last = last.get(..EARLIER).unwrap_or(last);
    report.note(format_args!("earlier: {last}"));
}

/// What the updater borrows from the run: the one flash storage, the SHA unit, the RTC (its
/// watchdog is fed between pieces) and the report it notes each step in.
struct Updater<'a, 's, 'h, 'r> {
    storage: &'a mut FlashStorage<'s>,
    sha: &'a mut Sha<'h>,
    rtc: &'a mut Rtc<'r>,
    report: &'a mut Report,
}

impl Updater<'_, '_, '_, '_> {
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
                self.report
                    .note(format_args!("slot {len} B {ms} ms {named}"));
            }
            Err(SlotError::Flash(at, e)) => {
                self.report
                    .note(format_args!("slot: read at {at:x}: {e:?}"));
            }
            Err(e) => self.report.note(format_args!("slot: {e:?}")),
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
                self.report
                    .note(format_args!("card {len} B {ms} ms {named}"));
                Some(image)
            }
            Some(Err(e)) => {
                self.report.note(format_args!("card: {e:?}"));
                None
            }
            None => {
                self.report.note(format_args!("card: not ready"));
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
            Err(e) => return self.report.note(format_args!("region: {e:?}")),
        };
        let ms = start.elapsed().as_millis();
        let sectors = card.checked.len.div_ceil(SECTOR);
        match written {
            Some(Ok(_)) => self
                .report
                .note(format_args!("{plan:?}: {sectors} sectors {ms} ms")),
            Some(Err(e)) => self.report.note(format_args!("{plan:?}: {e:?}")),
            None => self.report.note(format_args!("{plan:?}: not ready")),
        }
    }

    /// What `target` holds after the write: noted when it is an image that passes
    /// (`installed: same digest` for the card's release); otherwise the write's own line says why,
    /// and the sleep line follows.
    fn installed(&mut self, target: PartitionEntry, card: &Identified) -> InSlot {
        let (found, _) = self.identify(target);
        match &found {
            Ok(image) if image.digest == card.digest => {
                self.report.note(format_args!("installed: same digest"));
            }
            Ok(image) => self
                .report
                .note(format_args!("installed: {}", Named(&image.digest))),
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

/// The updater's steps: what the C++ firmware's slot holds at the end, and whether the updater
/// wrote the card's release there (it reads back as that release). `supply` is the card's
/// (`sd::open_volume`).
fn update(
    run: &mut Updater<'_, '_, '_, '_>,
    sdhost: SDHOST<'_>,
    pins: (GPIO41<'_>, GPIO42<'_>, GPIO40<'_>),
    supply: &mut Output<'_>,
    fell_back: bool,
) -> (InSlot, bool) {
    let target = match ota_slots(run.storage) {
        Ok(slots) => slots.other,
        Err(e) => {
            run.report.note(format_args!("slot: {e}"));
            return (InSlot::Unreadable, false);
        }
    };
    let in_slot = run.slot_state(target);
    let Ok(controller) = SdHostController::new(sdhost, SdConfig::default()) else {
        run.report.note(format_args!("card: no controller"));
        return (in_slot, false);
    };
    let rtc = &mut *run.rtc;
    let mut volume = match sd::open_volume(&controller, pins, supply, |_| rtc.rwdt.feed()) {
        Ok(volume) => volume,
        Err(e) => {
            run.report.note(format_args!("card: {e:?}"));
            return (in_slot, false);
        }
    };
    let Some(card) = run.card_release(&mut volume, target.len() as usize) else {
        return (in_slot, false);
    };
    let known = release(&card.digest).is_some();
    match plan(&card.digest, known, in_slot, fell_back) {
        Plan::Refuse(why) => {
            run.report.note(format_args!("refused: {why:?}"));
            (in_slot, false)
        }
        plan => {
            run.write(&mut volume, target, &card, plan);
            let after = run.installed(target, &card);
            (after, after == InSlot::Image(card.digest))
        }
    }
}
