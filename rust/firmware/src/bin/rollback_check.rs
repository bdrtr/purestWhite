//! The smallest bare-metal image for the second OTA slot: hands the next boot back to the C++
//! firmware, leaves a short report in RTC slow memory and resets; the C++ firmware saves the
//! report. It checks the way there and back without esp-rtos, which the emulator cannot run yet,
//! and reports whether the device's bootloader rolls back on its own.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::FlashStorage;
use purestwhite_firmware::board::{Board, hold_power_latch};
use purestwhite_firmware::boot::{Report, arm_watchdog, hand_back};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware instead
/// of the device hanging (esp-backtrace's default).
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
    esp_hal::system::software_reset()
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let _rtc = arm_watchdog(board.boot.rtc_timer);
    let _latch = hold_power_latch(board.boot.latch);
    // A report at once, so a panic from here on is not added to an older one.
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal rollback check");
    report.save();
    let mut flash = FlashStorage::new(board.boot.flash);
    let handed_back = hand_back(&mut flash, board.boot.cpu);
    match handed_back {
        Ok(found) => {
            let _ = writeln!(report, "next boot: C++ firmware");
            match found.booted_state {
                Some(state) => {
                    let _ = writeln!(report, "bootloader state: {state}");
                }
                None => {
                    let _ = writeln!(report, "bootloader state: no entry");
                }
            }
        }
        Err(e) => {
            let _ = writeln!(report, "next boot: NOT SET ({e})");
        }
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
