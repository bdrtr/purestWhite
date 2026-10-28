//! The X4 Pro's buttons from the bare-metal firmware: hands the next boot back to the C++
//! firmware, then for up to 40 s counts the presses of Left (GPIO0, the SDK's Up), Right
//! (GPIO7, its Down) and Power (GPIO3), debounced by `purestwhite-buttons` as the SDK debounces them. Power held for a second
//! ends the count early. Once Power is up again (or after 10 s), it reports and resets into the
//! C++ firmware. Without esp-rtos, so it runs in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::time::{Duration, Instant};
use purestwhite_buttons::Buttons;
use purestwhite_firmware::hold_power_latch;
use purestwhite_firmware::test_image::{Report, arm_watchdog, hand_back};

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
/// How long Power may stay down after that before the reset.
const RELEASE_WAIT: Duration = Duration::from_secs(10);

/// Milliseconds since boot, wrapping as `millis()` does.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn millis() -> u32 {
    Instant::now().duration_since_epoch().as_millis() as u32
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
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
    let power = Input::new(peripherals.GPIO3, pulled_up);
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
    let _ = writeln!(report, "dugme: sol {ups}, sag {downs}, guc {powers}");
    let _ = writeln!(
        report,
        "guc uzun: {}",
        if long { "evet" } else { "yok, 40 s doldu" }
    );
    report.save();

    // Power up again before the reset, so the C++ firmware does not start with it held.
    let released = Instant::now();
    while power.is_low() && released.elapsed() < RELEASE_WAIT {
        delay.delay_millis(10);
    }
    if power.is_low() {
        let _ = writeln!(report, "guc birakilmadi");
    }
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
