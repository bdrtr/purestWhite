//! The RTC watchdog, armed first.

use esp_hal::peripherals::RTC_TIMER;
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
