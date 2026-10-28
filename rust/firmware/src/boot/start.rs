//! An image's first steps, the same in every image while it hands the next boot back.

use super::{HandBack, Report, arm_watchdog, hand_back};
use crate::board::{Boot, hold_power_latch};
use core::fmt::Write as _;
use esp_hal::gpio::Output;
use esp_hal::rtc_cntl::Rtc;

/// What `start` leaves an image. Keep the RTC driver and the latch for the rest of the run.
pub struct Started<'d> {
    pub rtc: Rtc<'d>,
    pub latch: Output<'d>,
    pub report: Report,
    pub found: HandBack,
}

/// The watchdog armed (`arm_watchdog`) before anything that could hang, the power latch held
/// (`crate::board::hold_power_latch`), the report begun with `title` and saved, so that a panic
/// from here on is not added to an older one, then the next boot handed back to the C++ firmware
/// (`hand_back`) and noted. When that fails the way back rests on the bootloader alone: it notes
/// why and resets at once, touching nothing more.
#[must_use = "keep the RTC driver and the latch for the rest of the run"]
pub fn start(title: &str, boot: Boot) -> Started<'static> {
    let rtc = arm_watchdog(boot.rtc_timer);
    let latch = hold_power_latch(boot.latch);
    let mut report = Report::default();
    let _ = writeln!(report, "{title}");
    report.save();
    let found = match hand_back(boot.flash, boot.cpu) {
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
        found,
    }
}
