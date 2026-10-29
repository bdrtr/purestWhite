//! The SD updater's install, on the device (rust/firmware/DESIGN.md, the SD updater): the
//! firmware's `updater` run once, as the recovery mode of the `purestwhite` image does: the C++
//! firmware's release on the card written into the C++ firmware's slot within phase 2's limits
//! (only a release it knows, and only over the same release, or over a slot whose image no longer
//! passes and which the bootloader has just refused). It hands the next boot back first, as every
//! image, then resets into the C++ firmware, or sleeps (bounded) when a reset would only bring the
//! bootloader back to it (`purestwhite_ota::plan::resets`).
//!
//! A write cut short (power lost, a reset) leaves the C++ firmware's slot with no image that
//! passes. A bootloader that checks the image it boots then falls back to this one, which writes
//! the release again (seen in eink-qemu with the C++ build's bootloader, and the device's own
//! bootloader refuses such an image after a reset: boot-check). After the title and `next boot`,
//! the report gives the last line an earlier run left when that run ended without the C++
//! firmware starting (a reset, a panic, a sleep; a power loss clears it). A hand-back that fails
//! while the slot is broken resets at once into this image again (`crate::boot::start`). Runs in
//! the emulator too (`emu/baremetal-install.txt`, `emu/baremetal-install-refuse.txt`,
//! `emu/baremetal-repair.txt`, `emu/baremetal-reset-mid-write.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use esp_backtrace as _;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::updater;

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset. The bootloader then boots the C++ firmware, or
/// this image again when the C++ firmware's slot holds no image that passes.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
    esp_hal::system::software_reset()
}

const TITLE: &str = "purestWhite install";

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        earlier,
        found,
        flash: mut storage,
    } = start(TITLE, board.boot);
    // `note` prints each line it adds; these are `start`'s.
    esp_println::println!("{}", report.as_str().trim_end());
    updater::note_earlier(&mut report, earlier);
    let parts = updater::Parts {
        card: board.card,
        sha: board.sha,
        touch_supply: board.i2c.supply,
        panel_reset: board.panel.rst,
        power: board.buttons.power,
        low_power: board.low_power,
    };
    let fell_back = found.fell_back();
    let finished = updater::install(parts, &mut storage, &mut rtc, &mut report, fell_back);
    updater::end(&mut report, &mut rtc, finished)
}
