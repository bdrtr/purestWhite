//! An image's first steps, the same in every image while it hands the next boot back.

use super::{HandBack, Report, arm_watchdog, hand_back};
use crate::board::{Boot, flash_storage, hold_power_latch};
use core::fmt::Write as _;
use esp_bootloader_esp_idf::partitions::FlashStorage;
use esp_hal::gpio::Output;
use esp_hal::rtc_cntl::Rtc;

/// What `start` leaves an image. Keep the RTC driver and the latch for the rest of the run.
pub struct Started<'d> {
    pub rtc: Rtc<'d>,
    pub latch: Output<'d>,
    pub report: Report,
    /// The report saved before this run's, while the C++ firmware has not taken it: what an image
    /// left that ran before the C++ firmware could start again.
    pub earlier: Option<Report>,
    pub found: HandBack,
    /// The flash, for an image that reads or writes it: the one storage of the run
    /// (`crate::board::flash_storage`). On the device one made after the hand-back's write could
    /// not read anything (scratch-check's first run, 2026-10-29), so every image keeps this one,
    /// made before any write.
    pub flash: FlashStorage<'d>,
}

/// The watchdog armed (`arm_watchdog`) before anything that could hang, the power latch held
/// (`crate::board::hold_power_latch`), the report saved before kept, the report begun with `title`
/// and saved, so that a panic from here on is not added to an older one, then the flash opened
/// (`crate::board::flash_storage`) and the next boot handed back to the C++ firmware (`hand_back`)
/// and noted. When that fails the way back rests on the bootloader alone: it notes why and resets
/// at once, touching nothing more.
#[must_use = "keep the RTC driver and the latch for the rest of the run"]
pub fn start(title: &str, boot: Boot) -> Started<'static> {
    let rtc = arm_watchdog(boot.rtc_timer);
    let latch = hold_power_latch(boot.latch);
    let earlier = Report::saved();
    let mut report = Report::default();
    let _ = writeln!(report, "{title}");
    report.save();
    let mut flash = flash_storage(boot.flash);
    let found = match hand_back(&mut flash, boot.cpu) {
        Ok(found) => found,
        Err(e) => {
            let _ = writeln!(report, "next boot: NOT SET ({e})");
            report.save();
            esp_hal::system::software_reset()
        }
    };
    let _ = writeln!(report, "next boot: C++ firmware");
    report.save();
    Started {
        rtc,
        latch,
        report,
        earlier,
        found,
        flash,
    }
}
