//! The boot counter on the device (rust/firmware/DESIGN.md, the boot counter): each run counts the
//! runs before it that ended without getting healthy (`boot::begin_run`), notes the count, and then
//! ends without getting healthy on purpose, a different way each time: the first run by a panic,
//! the second by a plain reset, the third by a panic again. (eink-qemu 0.9.2 does not model the RTC
//! watchdog, so a hang is left out: the counter counts runs that did not get healthy, however they
//! ended.) The fourth sees three in a row, notes that the recovery mode is due, marks itself
//! healthy (`boot::finish_run`) and resets. It hands the next boot back first, as every image, so
//! during phase 2 each run ends in the C++ firmware, and the next run is the owner's next install
//! of it; the count lives in RTC memory meanwhile. Runs in the emulator too, where the bootloader
//! starts it again by itself (`emu/baremetal-count.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use esp_backtrace as _;
use purestwhite_bootcount::Runs;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{self, Report, Started, start};
use purestwhite_firmware::test_image::note;

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

/// Ends the run without getting healthy: a reset when one run before it is unfinished, else a
/// panic.
fn crash(report: &mut Report, runs: Runs) -> ! {
    if runs.unfinished == 1 {
        note(report, format_args!("crash: reset"));
        esp_hal::system::software_reset()
    }
    note(report, format_args!("crash: panic"));
    panic!("on purpose")
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite count check", board.boot);
    // `note` prints each line it adds; these are `start`'s.
    esp_println::println!("{}", report.as_str().trim_end());
    let runs = boot::begin_run();
    note(&mut report, format_args!("{runs}"));
    if !runs.recovery_due() {
        crash(&mut report, runs);
    }
    boot::finish_run();
    note(&mut report, format_args!("healthy"));
    note(&mut report, format_args!("done"));
    esp_hal::system::software_reset()
}
