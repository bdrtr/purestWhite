//! Feedback on the X4 Pro's panel from the bare-metal firmware, the first image that answers
//! input on screen: hands the next boot back to the C++ firmware, starts the panel and the touch
//! panel, draws four key boxes (Left, Right, Power, Home), then polls the input every 15 ms
//! (`purestwhite_firmware::input`) and draws what it gives with a fast refresh: a tick under a
//! key's box for each press (for Home, each tap), a dot where a tap touched down, a ring where a
//! contact was held, a line with a dot at its end for a swipe. With the SD card's `FONT`,
//! loaded whole into PSRAM at the start (`purestwhite_firmware::storage::load_whole`) so that
//! drawing never reads the card, each box has its name under it, the counts of taps, swipes and
//! long presses stand at the bottom after the first input, and "İyi geceler" is written under the
//! moon; without a card, the font or room in PSRAM, only the shapes. Until the C++ firmware would
//! sleep: Power held longer than 400 ms, or 60 s without input, or at the latest after 5 minutes.
//! Then it reports what it counted, draws a crescent moon with a full refresh, puts the panel to
//! sleep, and sleeps deeply until Power is pressed or for at most 60 s
//! (`test_image::report_and_sleep`); the wake boots the C++ firmware.
//!
//! The refreshes block: input during one is not read until it ends. Without esp-rtos, so it runs
//! in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

mod counts;
mod draw;
mod font;
mod screen;

use alloc::string::String;
use alloc::vec::Vec;
use alloc::{format, vec};
use core::fmt::Write as _;
use counts::Counts;
use draw::{FRAME_BYTES, canvas, draw_keys, draw_moon, draw_status};
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};
use esp_hal::time::{Duration, Instant};
use font::{Font, add_psram, open_font};
use purestwhite_firmware::board::{Board, CardParts, PanelParts};
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::input::{self, ButtonPins, Input};
use purestwhite_firmware::ready;
use purestwhite_firmware::sleep::Rails;
use purestwhite_firmware::test_image::{self, TestSleep};
use screen::{Screen, open_screen};

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
const MOST: Duration = Duration::from_secs(300);
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

/// The screen and what is drawn on it: the frame, the card's font, and the counts.
struct Feedback<'t> {
    screen: Screen,
    frame: Vec<u8>,
    font: Option<Font<'t>>,
    counts: Counts,
}

impl<'t> Feedback<'t> {
    /// Draws the key boxes and shows them with a full refresh.
    fn new(screen: Screen, font: Option<Font<'t>>) -> Self {
        let mut run = Feedback {
            screen,
            frame: vec![0xFFu8; FRAME_BYTES],
            font,
            counts: Counts::default(),
        };
        draw_keys(&mut canvas(&mut run.frame), &mut run.font);
        run.screen.show(&run.frame, &mut run.counts, true);
        run
    }

    /// Polls the input every `POLL_MS` and draws what it gives, until the C++ firmware would
    /// sleep: why, in a word or two.
    fn watch<B: embedded_hal_async::i2c::I2c>(
        &mut self,
        input: &mut Input<'_, B>,
        rtc: &mut Rtc<'_>,
    ) -> &'static str {
        let delay = Delay::new();
        let start = Instant::now();
        let mut last_activity = start;
        loop {
            let Some(polled) = ready(input.poll(millis())) else {
                return "not ready";
            };
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
            if self.counts.draw(&polled, &mut canvas(&mut self.frame)) {
                let counts = &self.counts;
                draw_status(&mut canvas(&mut self.frame), &mut self.font, || {
                    counts.status()
                });
                self.screen.show(&self.frame, &mut self.counts, false);
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

    /// The sleep screen with a full refresh, then the panel asleep, as the C++ firmware leaves
    /// it; whether it is.
    fn good_night(&mut self, rtc: &mut Rtc<'_>) -> bool {
        draw_moon(&mut canvas(&mut self.frame), &mut self.font);
        rtc.rwdt.feed();
        self.screen.show(&self.frame, &mut self.counts, true);
        self.screen.sleep()
    }
}

/// The card's font (`font::open_font`, its interval table in `table`), and the card's supply,
/// which the sleep holds; `feed` as `open_font`'s.
fn card_font(
    card: CardParts,
    table: &mut [u8],
    feed: impl FnMut(),
) -> (Option<Font<'_>>, Output<'static>) {
    // Off until its first try (`sd::mount`).
    let mut supply = Output::new(card.supply, Level::High, OutputConfig::default());
    let controller = SdHostController::new(card.sdhost, SdConfig::default()).ok();
    let pins = (card.clk, card.cmd, card.dat0);
    let font = controller
        .as_ref()
        .and_then(|c| open_font(c, pins, &mut supply, table, feed));
    (font, supply)
}

/// The panel (`screen::open_screen`); when it cannot be used, a reset after noting it.
fn open_display(report: &mut Report, parts: PanelParts) -> Screen {
    open_screen(parts).unwrap_or_else(|error| {
        let name = error
            .probe()
            .map_or("none", |found| found.controller().name());
        let _ = writeln!(report, "panel {name}: unusable");
        report.save();
        esp_hal::system::software_reset()
    })
}

/// The report's line on what started: the panel's controller and whether it works, the touch
/// controller's address (0 when not found), and the font's size and load time.
fn note_start(report: &mut Report, run: &Feedback<'_>, touch: Option<u8>) {
    let font = run.font.as_ref().map_or(String::from("none"), |f| {
        format!("{} KB {} ms", f.bytes.len() / 1024, f.load_ms)
    });
    let state = if run.screen.working { "ok" } else { "failed" };
    let touch = touch.unwrap_or(0);
    let _ = writeln!(
        report,
        "panel {} {state}, touch {touch:#04x}, font {font}",
        run.screen.name
    );
    report.save();
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    // Short, as the other lines: the report has 248 bytes.
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite feedback", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    let _psram = add_psram(board.psram);

    let screen = open_display(&mut report, board.panel);
    rtc.rwdt.feed();
    let mut table = [0u8; 2048];
    let (font, sd_power) = card_font(board.card, &mut table, || rtc.rwdt.feed());
    let mut run = Feedback::new(screen, font);
    rtc.rwdt.feed();

    let touch = test_image::open_touch(board.touch, board.i2c);
    note_start(&mut report, &run, touch.found.as_ref().map(|&(_, a)| a));

    let mut input = Input::new(
        ButtonPins::new(board.buttons),
        touch.found,
        input::POWER_HOLD_MS,
    );
    esp_println::println!("feedback: ready");
    let why = run.watch(&mut input, &mut rtc);
    run.counts.write(&mut report);
    report.save();
    if !run.good_night(&mut rtc) {
        let _ = writeln!(report, "panel did not sleep");
    }
    esp_println::println!("feedback: moon");

    let rails = Rails {
        touch_power: touch.supply,
        sd_power,
        panel_reset: run.screen.lines.rst,
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
