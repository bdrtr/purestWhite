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
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
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

/// The three buttons, active low with the SDK's internal pull-ups.
struct Keys<'d> {
    up: Input<'d>,
    down: Input<'d>,
    power: Input<'d>,
}

impl Keys<'_> {
    /// The buttons held now, as `UP`, `DOWN` and `POWER` bits.
    fn sample(&self) -> u8 {
        let held = |input: &Input<'_>, bit: u8| if input.is_low() { bit } else { 0 };
        held(&self.up, UP) | held(&self.down, DOWN) | held(&self.power, POWER)
    }
}

/// Each button's presses, debounced, for up to `WINDOW`, and whether Power was held for
/// `LONG_MS`, which ends the count early.
fn count_presses(keys: &Keys<'_>) -> ([u32; 3], bool) {
    let delay = Delay::new();
    let mut buttons = Buttons::new(POWER_BIT);
    let mut presses = [0u32; 3];
    let start = Instant::now();
    while start.elapsed() < WINDOW {
        let now = millis();
        let change = buttons.update(now, keys.sample());
        for (bit, count) in presses.iter_mut().enumerate() {
            if change.pressed & (1 << bit) != 0 {
                *count += 1;
            }
        }
        if buttons.state() & POWER != 0 && buttons.power_held_ms(now) >= LONG_MS {
            return (presses, true);
        }
        delay.delay_millis(1);
    }
    (presses, false)
}

/// Waits up to `RELEASE_WAIT` for Power to be up again, so that the C++ firmware does not start
/// with it held; whether it is.
fn wait_power_up(power: &Input<'_>) -> bool {
    let delay = Delay::new();
    let released = Instant::now();
    while power.is_low() && released.elapsed() < RELEASE_WAIT {
        delay.delay_millis(10);
    }
    !power.is_low()
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal button check", board.boot);

    // Up is GPIO0, a boot strap: held through a reset it starts the ROM's download mode, so the
    // count never resets the chip.
    let pulled_up = InputConfig::default().with_pull(Pull::Up);
    let keys = Keys {
        up: Input::new(board.buttons.left, pulled_up),
        down: Input::new(board.buttons.right, pulled_up),
        power: Input::new(board.buttons.power, pulled_up),
    };
    esp_println::println!("buttons: ready");
    let ([ups, downs, powers], long) = count_presses(&keys);
    let _ = writeln!(report, "keys: left {ups}, right {downs}, power {powers}");
    let _ = writeln!(
        report,
        "power long: {}",
        if long { "yes" } else { "no, 40 s passed" }
    );
    report.save();
    if !wait_power_up(&keys.power) {
        let _ = writeln!(report, "power held");
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
