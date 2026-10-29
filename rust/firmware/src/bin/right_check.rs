//! The Right key as an image starts (rust/hardware-tests/bare-metal.md, the recovery mode): the
//! `purestwhite` image enters its recovery mode when Right (GPIO7) is low in five samples 10 ms
//! apart as it starts, and on the device it did not, twice, with Right held through the C++
//! firmware's restart. This image starts as that one does (the CPU at its fastest, `start`), then
//! samples Right the same way (pulled up, every 10 ms) for 30 s and notes what it saw: whether
//! the first five samples were all low (what `purestwhite` decides on), the first sample's level,
//! how many samples were low and when the first and last of them came (from the first sample: the
//! system timer may keep counting through the C++ firmware's restart), and the presses (high to
//! low after the first sample). Then it resets into the C++ firmware. It writes nothing
//! but the hand-back's otadata entry, as every image.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::time::Instant;
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

/// How many samples, and how far apart: 30 s.
const SAMPLES: u32 = 3000;
const APART_MS: u32 = 10;
/// How many first samples `purestwhite` reads.
const DECIDES: u32 = 5;

/// Milliseconds since `from`.
#[allow(clippy::cast_possible_truncation, reason = "a run lasts seconds")]
fn millis(from: Instant) -> u32 {
    from.elapsed().as_millis() as u32
}

/// What the samples of Right showed.
#[derive(Default)]
struct Seen {
    samples: u32,
    /// Whether the first sample was low.
    first: Option<bool>,
    /// Whether the first `DECIDES` samples were all low.
    held: bool,
    low: u32,
    first_low: Option<u32>,
    last_low: u32,
    presses: u32,
    previous: bool,
}

impl Seen {
    fn add(&mut self, low: bool, at: u32) {
        if self.first.is_none() {
            self.first = Some(low);
            self.held = true;
        } else if low && !self.previous {
            self.presses += 1;
        }
        if self.samples < DECIDES {
            self.held &= low;
        }
        if low {
            self.low += 1;
            self.first_low.get_or_insert(at);
            self.last_low = at;
        }
        self.previous = low;
        self.samples += 1;
    }

    fn note(&self, report: &mut Report) {
        let level = |low| if low { "low" } else { "high" };
        let held = if self.held { "yes" } else { "no" };
        report.note(format_args!("held as purestwhite reads: {held}"));
        let first = level(self.first.unwrap_or_default());
        report.note(format_args!("first: {first}"));
        match self.first_low {
            Some(from) => report.note(format_args!(
                "low: {} of {}, {from}..{} ms",
                self.low, self.samples, self.last_low
            )),
            None => report.note(format_args!("low: 0 of {}", self.samples)),
        }
        report.note(format_args!("presses: {}", self.presses));
    }
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    ));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite right check", board.boot);
    // `note` prints each line it adds; these are `start`'s.
    esp_println::println!("{}", report.as_str().trim_end());
    let right = Input::new(
        board.buttons.right,
        InputConfig::default().with_pull(Pull::Up),
    );
    let delay = Delay::new();
    let mut seen = Seen::default();
    let from = Instant::now();
    for _ in 0..SAMPLES {
        seen.add(right.is_low(), millis(from));
        delay.delay_millis(APART_MS);
        rtc.rwdt.feed();
    }
    seen.note(&mut report);
    report.note(format_args!("done"));
    esp_hal::system::software_reset()
}
