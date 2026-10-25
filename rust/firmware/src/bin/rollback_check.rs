//! The smallest bare-metal image for the second OTA slot: hands the next boot back to the C++
//! firmware, leaves a short report in RTC slow memory and resets; the C++ firmware saves the
//! report. It checks the way there and back without esp-rtos, which the emulator cannot run yet,
//! and reports whether the device's bootloader rolls back on its own.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::gpio::{Level, Output, OutputConfig};
use purestwhite_firmware::{Report, arm_watchdog, hand_back};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware instead
/// of the device hanging (esp-backtrace's default).
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIK\n");
    esp_hal::system::software_reset()
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    // The board's power latch, which the C++ firmware raises first thing: on battery the device
    // turns off while it floats, and the report in RTC memory with it.
    let _latch = Output::new(peripherals.GPIO1, Level::High, OutputConfig::default());
    // A report at once, so a panic from here on is not added to an older one.
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal geri donus denemesi");
    report.save();
    let handed_back = hand_back(peripherals.FLASH, peripherals.CPU_CTRL);
    match handed_back {
        Ok(found) => {
            let _ = writeln!(report, "sonraki acilis: C++ yazilimi");
            match found.booted_state {
                Some(state) => {
                    let _ = writeln!(report, "onyukleyici durumu: {state}");
                }
                None => {
                    let _ = writeln!(report, "onyukleyici durumu: kayit yok");
                }
            }
        }
        Err(e) => {
            let _ = writeln!(report, "sonraki acilis: SECILEMEDI ({e})");
        }
    }
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
