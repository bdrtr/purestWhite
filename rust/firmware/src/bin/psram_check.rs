//! The X4 Pro's PSRAM from the bare-metal firmware: hands the next boot back to the C++ firmware,
//! starts the PSRAM (see `purestwhite_firmware::board::psram_config`: at 40 MHz, as timing tuning
//! is not there yet), adds it to the heap, and fills nearly all of it through one allocation twice,
//! with a pattern and with its inverse, reading each back and timing both; then reports and resets.
//! Without esp-rtos, so it runs in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::vec::Vec;
use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::psram::Psram;
use esp_hal::time::Instant;
use purestwhite_firmware::board::{Board, PSRAM_MHZ, psram_config};
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

/// What the test leaves to other allocations.
const SPARE: usize = 64 * 1024;

/// Saves the report with its last line and resets into the C++ firmware.
fn finish(report: &mut Report) -> ! {
    let _ = writeln!(report, "done");
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

/// The words the passes go over: the PSRAM's `size` bytes from `start` less `SPARE`, from the
/// heap, which should place them in the PSRAM; noted in the report. Ends the run when the heap
/// cannot give them.
fn reserve_words(report: &mut Report, start: *mut u8, size: usize) -> Vec<u32> {
    let count = size.saturating_sub(SPARE) / 4;
    let mut words: Vec<u32> = Vec::new();
    if words.try_reserve_exact(count).is_err() {
        let _ = writeln!(report, "heap: {} KB not allocated", count * 4 / 1024);
        finish(report)
    }
    words.resize(count, 0);
    let at = words.as_ptr() as usize;
    let inside = (start as usize..start as usize + size).contains(&at);
    let _ = writeln!(
        report,
        "heap: {} KB, {}",
        count * 4 / 1024,
        if inside { "in PSRAM" } else { "OUTSIDE PSRAM" }
    );
    report.save();
    words
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal PSRAM check", board.boot);
    // The internal heap, as the other images take it: the RAM the ROM bootloader used, which is
    // free once the app runs.
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    let psram = Psram::new(board.psram, psram_config());
    let (start, size) = psram.raw_parts();
    if size == 0 {
        let _ = writeln!(report, "psram: not found");
        finish(&mut report)
    }
    let _ = writeln!(
        report,
        "psram: {} KB, {PSRAM_MHZ} MHz, at {start:p}",
        size / 1024
    );
    report.save();
    esp_alloc::psram_allocator!(&psram);

    let mut words = reserve_words(&mut report, start, size);
    for invert in [false, true] {
        let (errors, written, read) = pass(&mut words, invert);
        let _ = writeln!(
            report,
            "memory{}: errors {errors}, write {written} ms, read {read} ms",
            if invert { " inverted" } else { "" }
        );
        report.save();
    }
    finish(&mut report)
}
