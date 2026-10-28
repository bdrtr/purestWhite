//! The RTC watchdog: armed first, and kept counting through a phase-2 image's deep sleep, so
//! that a sleep that never ends still resets the chip into the C++ firmware.

use esp_hal::peripherals::{LPWR, RTC_TIMER};
use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};
use esp_hal::time::Duration;

/// Arms the RTC watchdog to reset the chip in 60 s, before anything that could hang: the device has
/// no reset button, its power button is read by the firmware. The reset spares the RTC domain, so
/// the report and the power latch's pad hold (see `crate::board::hold_power_latch`) survive it.
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

/// The key that unlocks the RTC watchdog's registers, as esp-hal's `Rwdt` writes it.
pub const WATCHDOG_KEY: u32 = 0x50D8_3AA1;

/// Lets the RTC watchdog count on in sleep, which esp-hal's `enable` pauses it in, so that a
/// sleep that never ends still resets the chip.
pub fn watchdog_through_sleep() {
    let regs = LPWR::regs();
    // SAFETY: the key that unlocks the watchdog's registers, and 0 to lock them again, as esp-hal's
    // own `Rwdt` writes them; nothing else touches them meanwhile.
    regs.wdtwprotect()
        .write(|w| unsafe { w.bits(WATCHDOG_KEY) });
    regs.wdtconfig0()
        .modify(|_, w| w.wdt_pause_in_slp().clear_bit());
    regs.wdtwprotect().write(|w| unsafe { w.bits(0) });
}

/// Sets the RTC watchdog (`arm_watchdog`'s) for a deep sleep that might never end:
/// a core reset after `core`, which keeps the power latch's hold and the report, then a system
/// reset after `system` more, which releases the latch (on battery the device turns off) but ends a
/// chip stuck asleep; counting through the sleep. For phase-2 images, whose sleeps have a timer
/// shorter than `core`; a firmware whose sleeps last hours must not call it.
pub fn watch_sleep(rtc: &mut Rtc<'_>, core: Duration, system: Duration) {
    // esp-hal 1.2.2 halves every stage's time on the S3, though only stage 0 counts twice as long
    // in hardware (ESP-IDF halves stage 0 alone, in hal/esp32s3's rwdt_ll.h): stage 1 would count
    // half its time, so it is given twice `system`.
    rtc.rwdt.set_timeout(RwdtStage::Stage0, core);
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetCore);
    rtc.rwdt.set_timeout(RwdtStage::Stage1, system * 2);
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage1, RwdtStageAction::ResetSystem);
    watchdog_through_sleep();
    rtc.rwdt.feed();
}
