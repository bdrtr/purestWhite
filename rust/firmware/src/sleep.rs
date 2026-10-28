//! The X4 Pro's deep sleep, as the C++ firmware enters it (`HalPowerManager::startDeepSleep`):
//! the rails it leaves held, and a wake on Power through the low-power path. A deep sleep's wake
//! resets the chip, and the bootloader then starts the app that otadata selects (see
//! `crate::hand_back`, which also drops the bootloader's record of this image).

use esp_hal::delay::Delay;
use esp_hal::gpio::{Event, Input, Level, Output, WakeConfigError, WakeupConfig};
use esp_hal::peripherals::{Interrupt, LPWR};
use esp_hal::rtc_cntl::sleep::{LowPower, RtcSleepConfig};
use esp_hal::rtc_cntl::{Rtc, RwdtStage, RwdtStageAction};
use esp_hal::system::Cpu;
use esp_hal::time::{Duration, Instant};

/// The pins the sleep holds, as outputs.
pub struct Rails<'d> {
    /// The touch controller's supply (GPIO2), active low.
    pub touch_power: Output<'d>,
    /// The SD card's supply (GPIO5), active low.
    pub sd_power: Output<'d>,
    /// The panel's reset (GPIO14).
    pub panel_reset: Output<'d>,
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

/// Sets the RTC watchdog (`crate::arm_watchdog`'s) for a deep sleep that might never end: a core
/// reset after `core`, which keeps the power latch's hold and the report, then a system reset
/// after `system` more, which releases the latch (on battery the device turns off) but ends a
/// chip stuck asleep; counting through the sleep. Set `core` past the sleep's timer.
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

/// Waits until Power (`power`, active low) is up, for at most `most`, calling `between` every
/// 10 ms meanwhile (to feed a watchdog): whether it is up. Before `deep_sleep`, which Power held
/// down would end at once.
pub fn wait_power_up(power: &Input<'_>, most: Duration, mut between: impl FnMut()) -> bool {
    let delay = Delay::new();
    let start = Instant::now();
    while power.is_low() && start.elapsed() < most {
        between();
        delay.delay_millis(10);
    }
    !power.is_low()
}

/// Sleeps deeply until Power (`power`, active low, pulled up) is pressed, or `timer` has passed
/// when given. Power must be up first (see `wait_power_up`): a wake pin at its wake level ends
/// the sleep at once. Returns only when Power cannot be set to wake the chip and there is no
/// timer, as nothing would end the sleep then: why. A test image passes a timer, and
/// `watch_sleep` first.
///
/// As the SDK leaves the rails (`PowerManager::powerDownRailsForSleep`): touch power and SD power
/// held off, the panel's reset held high, as its rail stays powered. The power latch (GPIO1) must
/// already be held high (`crate::hold_power_latch`). The C++ firmware releases these holds at
/// boot.
pub fn deep_sleep(
    mut low_power: LowPower<'_>,
    power: &mut Input<'_>,
    mut rails: Rails<'_>,
    timer: Option<Duration>,
) -> WakeConfigError {
    power.listen(Event::LowLevel);
    // The low-power path wakes the chip without the pin's interrupt reaching the CPU, and nothing
    // here handles it (eink-qemu 0.9.2 also fails on GPIO interrupts).
    esp_hal::interrupt::disable(Cpu::ProCpu, Interrupt::GPIO);
    // If this fails only the timer ends the sleep, and the report shows how it ended.
    if let Err(e) = power.apply_wakeup_config(&WakeupConfig::default().with_low_power_path(true))
        && timer.is_none()
    {
        return e;
    }
    for pin in [
        &mut rails.touch_power,
        &mut rails.sd_power,
        &mut rails.panel_reset,
    ] {
        pin.set_level(Level::High);
        pin.set_pad_hold(true);
    }
    if let Some(timer) = timer {
        low_power.set_wakeup_deadline(Instant::now() + timer);
    }
    low_power.sleep_deep(RtcSleepConfig::deep())
}
