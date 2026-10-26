//! The X4 Pro's buttons and deep sleep from the bare-metal firmware: hands the next boot back to
//! the C++ firmware, then for up to 40 s counts the presses of Up (GPIO0), Down (GPIO7) and Power
//! (GPIO3), debounced by `purestwhite-buttons` as the SDK debounces them. Power held for a second
//! ends the count early. Then, with Power released, the chip goes into deep sleep as the C++
//! firmware does on the X4 Pro: the power latch, the touch and SD power and the panel's reset held
//! where the SDK holds them, and Power armed to wake it; a 60 s timer wakes it if Power does not.
//! The wake is a reset that `hand_back` sends to the C++ firmware, which saves the report, with
//! what ended the sleep. Without esp-rtos, so it runs in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Event, Input, InputConfig, Level, Output, OutputConfig, Pull, WakeupConfig};
use esp_hal::rtc_cntl::sleep::{LowPower, RtcSleepConfig};
use esp_hal::rtc_cntl::{RwdtStage, RwdtStageAction};
use esp_hal::time::{Duration, Instant};
use purestwhite_buttons::Buttons;
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIK\n");
    esp_hal::system::software_reset()
}

/// The buttons' bits in the sampled state.
const UP: u8 = 1 << 0;
const DOWN: u8 = 1 << 1;
const POWER_BIT: u8 = 2;
const POWER: u8 = 1 << POWER_BIT;
/// How long the presses are counted, and how long Power must be held to end the count.
const WINDOW: Duration = Duration::from_secs(40);
const LONG_MS: u32 = 1000;
/// How long Power may stay down after that before the sleep starts anyway.
const RELEASE_WAIT: Duration = Duration::from_secs(10);
/// The timer that wakes the chip if Power does not, and the watchdog's time from the sleep on.
const TIMER_WAKE: Duration = Duration::from_secs(60);
const WATCHDOG_IN_SLEEP: Duration = Duration::from_secs(120);

/// Milliseconds since boot, wrapping as `millis()` does.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn millis() -> u32 {
    Instant::now().duration_since_epoch().as_millis() as u32
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal dugme denemesi");
    report.save();
    match hand_back(peripherals.FLASH, peripherals.CPU_CTRL) {
        Ok(_) => {
            let _ = writeln!(report, "sonraki acilis: C++ yazilimi");
        }
        Err(e) => {
            // The way back rests on the bootloader alone now: touch nothing more.
            let _ = writeln!(report, "sonraki acilis: SECILEMEDI ({e})");
            report.save();
            esp_hal::system::software_reset()
        }
    }
    report.save();

    // Active low, with the SDK's internal pull-ups. Up is GPIO0, a boot strap: held through a
    // reset it starts the ROM's download mode, so the count never resets the chip.
    let pulled_up = InputConfig::default().with_pull(Pull::Up);
    let up = Input::new(peripherals.GPIO0, pulled_up);
    let down = Input::new(peripherals.GPIO7, pulled_up);
    let mut power = Input::new(peripherals.GPIO3, pulled_up);
    let held = |input: &Input<'_>, bit: u8| if input.is_low() { bit } else { 0 };
    let sample = |up: &Input<'_>, down: &Input<'_>, power: &Input<'_>| {
        held(up, UP) | held(down, DOWN) | held(power, POWER)
    };

    let delay = Delay::new();
    let mut buttons = Buttons::new(POWER_BIT);
    let mut presses = [0u32; 3];
    let start = Instant::now();
    let mut long = false;
    esp_println::println!("dugmeler: hazir");
    while start.elapsed() < WINDOW {
        let now = millis();
        let change = buttons.update(now, sample(&up, &down, &power));
        for (bit, count) in presses.iter_mut().enumerate() {
            if change.pressed & (1 << bit) != 0 {
                *count += 1;
            }
        }
        if buttons.state() & POWER != 0 && buttons.power_held_ms(now) >= LONG_MS {
            long = true;
            break;
        }
        delay.delay_millis(1);
    }
    let [ups, downs, powers] = presses;
    let _ = writeln!(report, "dugme: yukari {ups}, asagi {downs}, guc {powers}");
    let _ = writeln!(
        report,
        "guc uzun: {}",
        if long { "evet" } else { "yok, 40 s doldu" }
    );
    report.save();

    // Power down again first: a wake pad already at its wake level ends the sleep at once.
    let released = Instant::now();
    while power.is_low() && released.elapsed() < RELEASE_WAIT {
        delay.delay_millis(10);
    }
    if power.is_low() {
        let _ = writeln!(report, "guc birakilmadi, yine de uyku");
    }

    // As the SDK leaves the X4 Pro's rails for a deep sleep (PowerManager::powerDownRailsForSleep,
    // HalPowerManager::startDeepSleep): touch power (GPIO2) and SD power (GPIO5), both active
    // low, held off; the panel's reset (GPIO14) held high, as its rail stays powered. The latch
    // (GPIO1) is already held high. The C++ firmware releases these at boot.
    let mut touch_power = Output::new(peripherals.GPIO2, Level::High, OutputConfig::default());
    touch_power.set_pad_hold(true);
    let mut sd_power = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    sd_power.set_pad_hold(true);
    let mut panel_reset = Output::new(peripherals.GPIO14, Level::High, OutputConfig::default());
    panel_reset.set_pad_hold(true);

    // Power wakes the chip while it is low, through the low-power path deep sleep needs; the
    // timer wakes it if Power does not.
    power.listen(Event::LowLevel);
    if let Err(e) = power.apply_wakeup_config(&WakeupConfig::default().with_low_power_path(true)) {
        let _ = writeln!(report, "uyku: guc dugmesi kurulamadi ({e:?})");
    }
    let mut low_power = LowPower::new(peripherals.LPWR);
    low_power.set_wakeup_deadline(Instant::now() + TIMER_WAKE);
    let _ = writeln!(report, "uyku: guc dugmesi ya da 60 s");
    esp_println::println!("{}", report.as_str());
    report.save();
    // The watchdog stays the way back if the sleep never starts; past the 60 s timer, so that a
    // watchdog that runs on in the sleep (esp-hal pauses it there) does not cut it short. A
    // system reset, as a hang in the RTC's sleep sequence may outlast a core reset.
    rtc.rwdt.set_timeout(RwdtStage::Stage0, WATCHDOG_IN_SLEEP);
    rtc.rwdt
        .set_stage_action(RwdtStage::Stage0, RwdtStageAction::ResetSystem);
    rtc.rwdt.feed();
    low_power.sleep_deep(RtcSleepConfig::deep())
}
