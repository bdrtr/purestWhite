//! The X4 Pro's deep sleep, as the C++ firmware enters it (`HalPowerManager::startDeepSleep`):
//! the rails it leaves held, and a wake on Power through the low-power path. A deep sleep's wake
//! resets the chip, and the bootloader then starts the app that otadata selects (see
//! `crate::boot::hand_back`, which also drops the bootloader's record of this image).
//!
//! On the device, with esp-hal 1.2.2, a deep sleep that is the first sleep since boot did not
//! end: neither Power nor its timer woke the chip, which most likely never entered the sleep
//! (button-check's first run on 2026-10-26, with the watchdog off, until the owner restarted the
//! device; input-check's first run on 2026-10-28, until the watchdog reset it after 90 s). After
//! one light sleep it started and woke (sleep-check, input-check's second run).
//!
//! The cause: just before it asks for the sleep, esp-hal disconnects every digital pad
//! without a hold (`isolate_pads_for_deep_sleep` in its gpio/wakeup/mod.rs), the flash's (GPIO
//! 27-32) too, which ESP-IDF skips, as touching them may hang the chip
//! (`esp_sleep_isolate_digital_gpio` in its `esp_hw_support/sleep_gpio.c`). The code that runs
//! after that must then come from the cache, which a light sleep before has filled and a first
//! sleep has not. esp-hal does it only while the digital pads' auto-hold is enabled, an RTC
//! setting that survives the reset into this image (the C++ firmware enables it for its own
//! sleeps); `deep_sleep` disables it. Confirmed on the device on 2026-10-28: input-check found the
//! auto-hold enabled, and with it disabled its first sleep, a deep one, woke on Power. eink-qemu
//! models no flash pads and sleeps either way.

use esp_hal::delay::Delay;
use esp_hal::gpio::{Event, Input, Level, Output, WakeConfigError, WakeupConfig};
use esp_hal::peripherals::{Interrupt, LPWR};
use esp_hal::rtc_cntl::sleep::{LowPower, RtcSleepConfig};
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

/// A step of `deep_sleep`, for a caller that notes its progress: each but `ArmFailed` comes
/// before its step.
#[derive(Clone, Copy, Debug)]
pub enum Step {
    /// Arming Power.
    Arm,
    /// Power could not be armed: only the timer, if any, ends the sleep.
    ArmFailed,
    /// Holding the rails.
    Rails,
    /// The deep sleep itself.
    Sleep,
}

/// Sleeps deeply until Power (`power`, active low, pulled up) is pressed, or `timer` has passed
/// when given: Power armed through the low-power path, then the rails held as the SDK leaves
/// them (`PowerManager::powerDownRailsForSleep`: touch power and SD power off, the panel's reset
/// high, as its rail stays powered), then the digital pads' auto-hold disabled (see the module's
/// doc), then the deep sleep. `on_step` hears of each step.
///
/// Power must be up first (see `wait_power_up`): a wake pin at its wake level ends the sleep at
/// once. The power latch (GPIO1) must already be held high (`crate::board::hold_power_latch`). The
/// C++ firmware releases these holds at boot. Returns only when Power cannot be armed and there is
/// no timer, as nothing would end the sleep then: why.
pub fn deep_sleep(
    mut low_power: LowPower<'_>,
    power: &mut Input<'_>,
    mut rails: Rails<'_>,
    timer: Option<Duration>,
    mut on_step: impl FnMut(Step),
) -> WakeConfigError {
    on_step(Step::Arm);
    power.listen(Event::LowLevel);
    // The low-power path wakes the chip without the pin's interrupt reaching the CPU, and nothing
    // here handles it (eink-qemu 0.9.2 also fails on GPIO interrupts).
    esp_hal::interrupt::disable(Cpu::ProCpu, Interrupt::GPIO);
    if let Err(e) = power.apply_wakeup_config(&WakeupConfig::default().with_low_power_path(true)) {
        on_step(Step::ArmFailed);
        if timer.is_none() {
            return e;
        }
    }

    on_step(Step::Rails);
    for pin in [
        &mut rails.touch_power,
        &mut rails.sd_power,
        &mut rails.panel_reset,
    ] {
        pin.set_level(Level::High);
        pin.set_pad_hold(true);
    }

    // The digital pads (GPIO 22-48) then float through the sleep, unheld: none of them needs a
    // level here, and the C++ firmware enables the auto-hold again for its own sleeps. The pads
    // held above, and Power, are low-power pads, held on their own.
    LPWR::regs()
        .dig_iso()
        .modify(|_, w| w.dg_pad_autohold_en().clear_bit());

    on_step(Step::Sleep);
    if let Some(timer) = timer {
        low_power.set_wakeup_deadline(Instant::now() + timer);
    }
    low_power.sleep_deep(RtcSleepConfig::deep())
}
