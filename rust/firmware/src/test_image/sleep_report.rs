//! A test image's deep sleep, each step marked on the report's last line as it is taken.

use super::{Report, watch_sleep};
use crate::sleep::{Rails, Step, deep_sleep, wait_power_up};
use core::fmt::Write as _;
use esp_hal::gpio::Input;
use esp_hal::peripherals::LPWR;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::time::Duration;

/// A test image's deep sleep: its timer, and the watchdog's stages past it (see `watch_sleep`);
/// and how long Power may stay down before it starts anyway (`sleep_when_released`).
#[derive(Clone, Copy, Debug)]
pub struct TestSleep {
    pub timer: Duration,
    pub core: Duration,
    pub system: Duration,
    pub release: Duration,
}

/// `report_and_sleep`, once Power is up again or after `sleep.release`, feeding the watchdog
/// meanwhile: Power at its wake level would end the sleep at once. Notes it when Power stays
/// down.
pub fn sleep_when_released(
    report: &mut Report,
    why: &str,
    rtc: &mut Rtc<'_>,
    mut power: Input<'_>,
    rails: Rails<'_>,
    low_power: LowPower<'_>,
    sleep: TestSleep,
) -> ! {
    if !wait_power_up(&power, sleep.release, || rtc.rwdt.feed()) {
        let _ = writeln!(report, "guc birakilmadi");
    }
    report_and_sleep(report, why, rtc, &mut power, rails, low_power, sleep)
}

/// For test images: ends `report` with the sleep's line, `uyku: derin, ` and `why`, which the C++
/// firmware looks for to add what woke and reset the chip, then sleeps deeply as `sleep` says,
/// marking on that line first the sleep interrupt flags (`i`) and the digital pads' isolation and
/// hold settings (`p`, bit 11 being the auto-hold `deep_sleep` disables) as found, then a letter
/// before each step: `a` arming Power (`!` if that failed), `r` holding the rails, `s` the deep
/// sleep. A sleep that never ends shows its step.
pub fn report_and_sleep(
    report: &mut Report,
    why: &str,
    rtc: &mut Rtc<'_>,
    power: &mut Input<'_>,
    rails: Rails<'_>,
    low_power: LowPower<'_>,
    sleep: TestSleep,
) -> ! {
    let _ = write!(report, "uyku: derin, {why}");
    let mut line = SleepLine::new(report);
    let regs = LPWR::regs();
    line.mark(format_args!(
        " i{:x} p{:x}",
        regs.int_raw().read().bits(),
        regs.dig_iso().read().bits()
    ));
    esp_println::println!("{}", line.report.as_str());
    watch_sleep(rtc, sleep.core, sleep.system);
    // With a timer it does not return.
    let _ = deep_sleep(low_power, power, rails, Some(sleep.timer), |step| {
        line.mark(match step {
            Step::Arm => format_args!(" a"),
            Step::ArmFailed => format_args!("!"),
            Step::Rails => format_args!(" r"),
            Step::Sleep => format_args!(" s"),
        });
    });
    esp_hal::system::software_reset()
}

/// The report's last line, saved again with a mark added each time.
struct SleepLine<'r> {
    report: &'r mut Report,
    /// Where the line's text ends, before its newline.
    end: usize,
}

impl<'r> SleepLine<'r> {
    /// The line written last, without its newline yet.
    fn new(report: &'r mut Report) -> Self {
        let end = report.len();
        SleepLine { report, end }
    }

    fn mark(&mut self, text: core::fmt::Arguments<'_>) {
        self.report.truncate(self.end);
        let _ = self.report.write_fmt(text);
        self.end = self.report.len();
        let _ = writeln!(self.report);
        self.report.save();
    }
}
