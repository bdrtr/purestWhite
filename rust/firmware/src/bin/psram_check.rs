//! The X4 Pro's PSRAM from the bare-metal firmware: hands the next boot back to the C++ firmware,
//! starts the PSRAM (see `purestwhite_firmware::psram_config`: at 40 MHz, as timing tuning is not
//! there yet), adds
//! it to the heap, and fills nearly all of it through one allocation twice, with a pattern and
//! with its inverse, reading each back and timing both; then reports and resets. Without
//! esp-rtos, so it runs in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::psram::Psram;
use esp_hal::time::Instant;
use purestwhite_firmware::{
    PSRAM_MHZ, Report, arm_watchdog, hand_back, hold_power_latch, psram_config,
};

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

/// What the test leaves to other allocations.
const SPARE: usize = 64 * 1024;

/// Saves the report with its last line and resets into the C++ firmware.
fn finish(report: &mut Report) -> ! {
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}

/// The test's word at `i`: a different value for nearby words, so a swapped or shorted address
/// line shows.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn pattern(i: usize) -> u32 {
    (i as u32).wrapping_mul(0x9E37_79B9) ^ (i >> 16) as u32
}

/// Fills `words` with `pattern` (inverted when asked) and reads it back: the words that differ,
/// and the milliseconds the write and the read took.
fn pass(words: &mut [u32], invert: bool) -> (usize, u64, u64) {
    let flip = if invert { u32::MAX } else { 0 };
    let start = Instant::now();
    for (i, w) in words.iter_mut().enumerate() {
        *w = pattern(i) ^ flip;
    }
    let written = start.elapsed().as_millis();
    let words = core::hint::black_box(words);
    let start = Instant::now();
    let errors = words
        .iter()
        .enumerate()
        .filter(|&(i, &w)| w != pattern(i) ^ flip)
        .count();
    (errors, written, start.elapsed().as_millis())
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal PSRAM denemesi");
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
    // The internal heap, as the other images take it: the RAM the ROM bootloader used, which is
    // free once the app runs.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    let psram = Psram::new(peripherals.PSRAM, psram_config());
    let (start, size) = psram.raw_parts();
    if size == 0 {
        let _ = writeln!(report, "psram: bulunamadi");
        finish(&mut report)
    }
    let _ = writeln!(
        report,
        "psram: {} KB, {PSRAM_MHZ} MHz, adres {start:p}",
        size / 1024
    );
    report.save();
    esp_alloc::psram_allocator!(&psram);

    let count = size.saturating_sub(SPARE) / 4;
    let mut words: Vec<u32> = Vec::new();
    if words.try_reserve_exact(count).is_err() {
        let _ = writeln!(report, "yigin: {} KB ayrilamadi", count * 4 / 1024);
        finish(&mut report)
    }
    words.resize(count, 0);
    let at = words.as_ptr() as usize;
    let inside = (start as usize..start as usize + size).contains(&at);
    let _ = writeln!(
        report,
        "yigin: {} KB, {}",
        count * 4 / 1024,
        if inside { "psram'de" } else { "PSRAM DISINDA" }
    );
    report.save();
    for invert in [false, true] {
        let (errors, written, read) = pass(&mut words, invert);
        let _ = writeln!(
            report,
            "bellek{}: hata {errors}, yazma {written} ms, okuma {read} ms",
            if invert { " ters" } else { "" }
        );
        report.save();
    }
    finish(&mut report)
}
