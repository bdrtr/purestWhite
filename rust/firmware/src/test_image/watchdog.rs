//! The RTC watchdog (armed by `crate::boot::arm_watchdog`), kept counting through a test image's
//! deep sleep.

use esp_hal::peripherals::LPWR;
use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};
use esp_hal::time::Duration;

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

/// Sets the RTC watchdog (`crate::boot::arm_watchdog`'s) for a deep sleep that might never end:
/// a core reset after `core`, which keeps the power latch's hold and the report, then a system
/// reset after `system` more, which releases the latch (on battery the device turns off) but ends a
/// chip stuck asleep; counting through the sleep. For test images, whose sleeps have a timer
/// shorter than `core`: the real firmware must not call it, as its sleeps last hours.
pub fn watch_sleep(rtc: &mut Rtc<'_>, core: Duration, system: Duration) {
    // esp-hal 1.2.2 halves every stage's time on the S3, though only stage 0 counts twice as long
    // in hardware (ESP-IDF halves stage 0 alone, in hal/esp32s3's rwdt_ll.h): stage 1 gets twice
    // the time meant.
    rtc.rwdt.set_timeout(RwdtStage::Stage0, core);
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetCore);
    rtc.rwdt.set_timeout(RwdtStage::Stage1, system * 2);
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage1, RwdtStageAction::ResetSystem);
    watchdog_through_sleep();
    rtc.rwdt.feed();
}
