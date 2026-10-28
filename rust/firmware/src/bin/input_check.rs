//! The X4 Pro's input and sleep from the bare-metal firmware: hands the next boot back to the C++
//! firmware, starts the touch panel, then polls the buttons and the panel every 15 ms
//! (`purestwhite_firmware::input`) and counts what they give, until the C++ firmware would sleep:
//! Power held longer than 400 ms, or 60 s without input (the C++ firmware waits its setting,
//! 10 minutes by default), or at the latest after 3 minutes. Then, once Power is up, it reports
//! and sleeps deeply (`purestwhite_firmware::sleep`) until Power is pressed, or for at most 60 s;
//! the wake boots the C++ firmware, which adds what woke the chip and what reset it. The RTC
//! watchdog counts through the sleep: a sleep that never ends resets the chip after 90 s, and
//! after 30 s more resets it fully (on battery the device then turns off). Without esp-rtos, so it
//! runs in the emulator too.
//!
//! The report's sleep line gets a mark before each step of the sleep, so that a sleep that never
//! ends shows its step (see `test_image::report_and_sleep`).
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::time::{Duration, Instant};
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::input::{self, ButtonPins, Input, Polled};
use purestwhite_firmware::ready;
use purestwhite_firmware::sleep::Rails;
use purestwhite_firmware::test_image::{self, TestSleep};

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

/// How long without input puts the device to sleep, and how long it runs at most.
const IDLE: Duration = Duration::from_secs(60);
const MOST: Duration = Duration::from_secs(180);
/// The sleep: until Power, or for at most 60 s; the watchdog resets the chip if it has not
/// woken after 90 s, and fully 30 s later. Power may stay down 10 s before it starts anyway.
const TEST_SLEEP: TestSleep = TestSleep {
    timer: Duration::from_secs(60),
    core: Duration::from_secs(90),
    system: Duration::from_secs(30),
    release: Duration::from_secs(10),
};

/// Milliseconds since boot, wrapping as `millis()` does.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn millis() -> u32 {
    Instant::now().duration_since_epoch().as_millis() as u32
}

/// What the run counted.
#[derive(Default)]
struct Counts {
    presses: [u32; 3],
    touches: u32,
    taps: u32,
    swipes: u32,
    long_presses: u32,
    home_taps: u32,
    home_long_presses: u32,
    /// Contacts lifted (also those of several fingers, which give no touch-down), those lifted
    /// neither as a tap nor as a swipe, split by whether they lasted longer than a swipe may, and
    /// touch reads that failed: what tells a contact never seen from one not taken as a gesture.
    releases: u32,
    slow: u32,
    other: u32,
    failed_reads: u32,
}

impl Counts {
    fn add(&mut self, polled: &Polled, held_ms: u32) {
        for (bit, count) in self.presses.iter_mut().enumerate() {
            if polled.buttons.pressed & (1 << bit) != 0 {
                *count += 1;
            }
        }
        let t = &polled.touch;
        self.touches += u32::from(t.pressed.is_some());
        self.taps += u32::from(t.tap.is_some());
        self.swipes += u32::from(t.swipe.is_some());
        self.long_presses += u32::from(t.long_press.is_some());
        self.home_taps += u32::from(t.home_tapped);
        self.home_long_presses += u32::from(t.home_long_press);
        self.failed_reads += u32::from(polled.touch_failed);
        if t.released {
            self.releases += 1;
            if t.tap.is_none() && t.swipe.is_none() {
                if held_ms > purestwhite_touch::SWIPE_MAX_MS {
                    self.slow += 1;
                } else {
                    self.other += 1;
                }
            }
        }
    }

    fn write(&self, report: &mut Report) {
        let [left, right, power] = self.presses;
        let _ = writeln!(report, "keys: left {left} right {right} power {power}");
        let _ = writeln!(
            report,
            "down {} tap {} swipe {} long {} home {}/{}",
            self.touches,
            self.taps,
            self.swipes,
            self.long_presses,
            self.home_taps,
            self.home_long_presses
        );
        let _ = writeln!(
            report,
            "lift {} slow {} other {} failed {}",
            self.releases, self.slow, self.other, self.failed_reads
        );
    }
}

/// Polls the input every `POLL_MS` and counts what it gives, until the C++ firmware would sleep:
/// why, in a word or two.
fn watch<B: embedded_hal_async::i2c::I2c>(
    input: &mut Input<'_, B>,
    rtc: &mut Rtc<'_>,
    counts: &mut Counts,
) -> &'static str {
    let delay = Delay::new();
    let start = Instant::now();
    let mut last_activity = start;
    loop {
        let Some(polled) = ready(input.poll(millis())) else {
            return "not ready";
        };
        counts.add(&polled, input.last_touch_held_ms());
        if polled.touch.long_press.is_some() {
            // As the C++ firmware's callers: the lift of a long press does not also tap.
            input.suppress_touch();
        }
        if polled.activity() {
            last_activity = Instant::now();
        }
        if polled.sleep {
            return "power";
        }
        if last_activity.elapsed() >= IDLE {
            return "idle 60 s";
        }
        if start.elapsed() >= MOST {
            return "time up";
        }
        rtc.rwdt.feed();
        delay.delay_millis(input::POLL_MS);
    }
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal input check", board.boot);

    let touch = test_image::open_touch(board.touch, board.i2c);
    test_image::note_touch(&mut report, touch.found.as_ref().map(|&(_, addr)| addr));
    let pins = ButtonPins::new(board.buttons);
    let mut input = Input::new(pins, touch.found, input::POWER_HOLD_MS);
    let mut counts = Counts::default();
    esp_println::println!("input: ready");
    let why = watch(&mut input, &mut rtc, &mut counts);
    counts.write(&mut report);
    report.save();

    let rails = Rails {
        touch_power: touch.supply,
        sd_power: Output::new(board.card.supply, Level::High, OutputConfig::default()),
        panel_reset: Output::new(board.panel.rst, Level::High, OutputConfig::default()),
    };
    test_image::sleep_when_released(
        &mut report,
        why,
        &mut rtc,
        input.into_pins().power,
        rails,
        LowPower::new(board.low_power),
        TEST_SLEEP,
    )
}
