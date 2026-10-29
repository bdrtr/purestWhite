//! The boot counter's record (`purestwhite_bootcount`) in RTC slow memory: the 16 bytes just
//! below the report (`REPORT_ADDR`), clear of the C++ firmware's RTC data, which ends at
//! `0x5000_17BC` in its current build; the C++ firmware never writes there. RTC slow memory keeps
//! it through resets, watchdog core resets and deep sleeps, not through a power cut. During phase 2
//! a power-on starts the C++ firmware, not an image, so it is the power cut that clears the count
//! then, and `begin_run`'s check of the reset reason matters once an image boots first.

use super::report::REPORT_ADDR;
use esp_hal::rtc_cntl::SocResetReason;
use purestwhite_bootcount::{Runs, WORDS};

/// Where the record is: the 16 bytes before the report.
const RUNS_ADDR: usize = REPORT_ADDR - WORDS * 4;

fn read() -> [u32; WORDS] {
    let base = RUNS_ADDR as *const u32;
    // SAFETY: RTC slow memory is plain RAM that nothing else in these images, nor the C++
    // firmware, uses at this address; reads only.
    core::array::from_fn(|i| unsafe { base.add(i).read_volatile() })
}

fn write(words: [u32; WORDS]) {
    let base = RUNS_ADDR as *mut u32;
    for (i, word) in words.into_iter().enumerate() {
        // SAFETY: as `read`; the record is 16 bytes, below the report.
        unsafe { base.add(i).write_volatile(word) };
    }
    core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
}

/// Counts the last run if it never got healthy (from zero after a power-on), marks this run
/// running, and returns the record. Call it once, early in the run.
#[must_use]
pub fn begin_run() -> Runs {
    let powered_on = esp_hal::system::reset_reason() == Some(SocResetReason::ChipPowerOn);
    let runs = Runs::begin(Runs::decode(read()), powered_on);
    write(runs.encode());
    runs
}

/// This run marked healthy: the count starts again from zero.
pub fn finish_run() {
    write(Runs::healthy().encode());
}
