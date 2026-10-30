//! The SD updater on the X4 Pro (rust/firmware/DESIGN.md, the SD updater): `purestwhite-updater`'s
//! steps over the device's parts. The C++ firmware's slot is a flash of its own (its partition's
//! region, which cannot write outside it), the card is read over the blocking bus, digests come from
//! the chip's SHA unit, the RTC watchdog is fed between pieces, and each step is noted in the
//! report. Then `end` resets into the C++ firmware (`purestwhite_updater::Done::resets`), or sleeps
//! (bounded) until Power or a timer rather than reset straight back into the image that runs the
//! updater: a bootloader that checks images falls back to that image while the slot holds none, or
//! keeps refusing an image the updater's own check passes. Used by install-check and by the
//! recovery mode of the `purestwhite` image.

use crate::board::CardParts;
use crate::boot::{self, BoundedSleep, Report};
use crate::sd::{self, BlockingBus, CardBlocks, OpenError, SpinDelay};
use crate::sha::ShaUnit;
use crate::sleep::Rails;
use crate::slots::ota_slots;
use core::fmt::{Arguments, Debug, Formatter};
use esp_bootloader_esp_idf::partitions::FlashStorage;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::peripherals::{GPIO2, GPIO3, GPIO14, GPIO40, GPIO41, GPIO42, LPWR, SDHOST, SHA};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};
use esp_hal::sha::Sha;
use esp_hal::time::{Duration, Instant};
use purestwhite_fat::Volume;
use purestwhite_ota::plan::InSlot;
use purestwhite_updater::releases::X4_PRO;
use purestwhite_updater::{Card, Done, Notes, Updater};

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
/// (`purestwhite_updater::Done::resets`), and the parts the sleep needs otherwise.
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
    let mut card = SdCard {
        parts: Some((sdhost, (clk, cmd, dat0))),
        controller: None,
        supply: &mut sd_power,
    };
    let done = update(
        storage,
        &mut ShaUnit(&mut sha),
        rtc,
        report,
        &mut card,
        fell_back,
    );
    drop(card);
    Finished {
        resets: done.resets(fell_back),
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

/// The updater's steps on the C++ firmware's slot: the slot this image does not run from.
fn update(
    storage: &mut FlashStorage<'_>,
    hashes: &mut ShaUnit<'_, '_>,
    rtc: &mut Rtc<'_>,
    report: &mut Report,
    card: &mut SdCard<'_>,
    fell_back: bool,
) -> Done {
    let unreadable = Done {
        after: InSlot::Unreadable,
        wrote: false,
    };
    let target = match ota_slots(storage) {
        Ok(slots) => slots.other,
        Err(e) => {
            report.note(format_args!("slot: {e}"));
            return unreadable;
        }
    };
    let mut region = target.as_flash_region(storage);
    let mut slot = match region.as_nor_flash() {
        Ok(slot) => slot,
        Err(e) => {
            report.note(format_args!("slot: read at 0: {e:?}"));
            return unreadable;
        }
    };
    Updater {
        known: &X4_PRO,
        hashes,
        notes: report,
        feed: || rtc.rwdt.feed(),
        now_ms,
    }
    .update(&mut slot, card, fell_back)
}

/// Milliseconds since the chip started, for the times the updater notes.
fn now_ms() -> u64 {
    Instant::now().duration_since_epoch().as_millis()
}

impl Notes for Report {
    fn note(&mut self, line: Arguments<'_>) {
        Report::note(self, line);
    }
}

/// The card's pins, until it is opened.
type CardPins = (
    SDHOST<'static>,
    (GPIO41<'static>, GPIO42<'static>, GPIO40<'static>),
);

/// The card, opened once through a controller made then (`sd::open_volume`), with its supply
/// (`sd::mount`).
struct SdCard<'a> {
    parts: Option<CardPins>,
    controller: Option<SdHostController<'static>>,
    supply: &'a mut Output<'static>,
}

/// Why the card could not be opened, as the report names it.
enum CardError {
    /// It was opened before (the updater opens it once).
    Opened,
    NoController,
    Open(OpenError),
}

impl Debug for CardError {
    fn fmt(&self, out: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            CardError::Opened => out.write_str("opened before"),
            CardError::NoController => out.write_str("no controller"),
            CardError::Open(e) => e.fmt(out),
        }
    }
}

impl Card for SdCard<'_> {
    type Blocks<'c>
        = CardBlocks<BlockingBus<'c>, SpinDelay>
    where
        Self: 'c;
    type Error = CardError;

    fn open(&mut self, feed: &mut dyn FnMut()) -> Result<Volume<Self::Blocks<'_>>, CardError> {
        let (sdhost, pins) = self.parts.take().ok_or(CardError::Opened)?;
        let controller = SdHostController::new(sdhost, SdConfig::default())
            .map_err(|_| CardError::NoController)?;
        let controller = self.controller.insert(controller);
        sd::open_volume(controller, pins, self.supply, |_| feed()).map_err(CardError::Open)
    }
}
