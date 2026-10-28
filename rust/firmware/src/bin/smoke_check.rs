//! Smoke test of the bare-metal firmware, for the second OTA slot: starts esp-rtos and Embassy,
//! runs our clock, fuel gauge and touch drivers (purestwhite-rtc, purestwhite-gauge,
//! purestwhite-gt911) on esp-hal's I2C, reads touches for 15 s with the panel black meanwhile and
//! white after (on a UC8279 or UC8179, with purestwhite-epd), then leaves a text report in RTC slow
//! memory and resets.
//!
//! First it does what every test image does first (`boot::start`): arms the RTC watchdog,
//! holds the power latch and hands the next boot back to the C++ firmware, which then saves the
//! report to the SD card. The report is saved after every stage, and a panic adds a line to it and
//! resets.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use alloc::vec::Vec;
use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_time::{Delay, Instant, Timer};
use esp_backtrace as _;
use esp_hal::Async;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Flex, Level, Output, OutputConfig};
use esp_hal::i2c::master::{Config as I2cConfig, I2c, SoftwareTimeout};
use esp_hal::time::{Duration, Rate};
use esp_hal::timer::systimer::SystemTimer;
use log::info;
use purestwhite_epd::uc8279x4::Refresh;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::board::{I2cParts, PanelParts, TouchParts};
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::input::{TOUCH, TouchPins};
use purestwhite_firmware::panel::{self, Lines, OpenError, Opened, Panel};
use purestwhite_firmware::test_image::note_touch;
use purestwhite_gt911::Reading;

extern crate alloc;

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

const RTC_ADDR: u8 = 0x51;
const GAUGE_ADDR: u8 = 0x63;

/// What 15 s of touches showed.
#[derive(Default)]
struct Touches {
    frames: u32,
    most: u8,
    first: Option<(u16, u16)>,
    last: Option<(u16, u16)>,
    home_presses: u32,
    home_down: bool,
    failed: u32,
}

impl Touches {
    fn home(&mut self, down: bool) {
        if down && !self.home_down {
            self.home_presses += 1;
        }
        self.home_down = down;
    }
}

/// The clock chip's raw seconds, minutes and hours registers, which `purestwhite_rtc::now` does
/// not give when the oscillator-stopped flag is set; `None` when the read fails.
fn clock_raw<B: embedded_hal::i2c::I2c>(bus: &mut B) -> Option<[u8; 3]> {
    let mut raw = [0; 3];
    bus.write_read(RTC_ADDR, &[0x02], &mut raw)
        .ok()
        .map(|()| raw)
}

/// A frame of the X4 Pro's panel: 480 rows of 100 bytes.
const FRAME_BYTES: usize = 100 * 480;

/// The panel set up for drawing, with a frame for it. The driver runs blocking, as in
/// `display_check`: nothing else runs meanwhile, and it is the path the emulator checks.
struct Screen {
    panel: Panel,
    lines: Lines<'static>,
    frame: Vec<u8>,
}

impl Screen {
    /// The whole panel `fill` (0x00 black, 0xFF white) in a full refresh, powered off after.
    fn paint(&mut self, fill: u8) -> bool {
        self.frame.fill(fill);
        self.panel
            .display(&mut self.lines, &self.frame, Refresh::Full, true)
    }
}

/// Probes the panel and sets it up when a driver here can draw on it; notes the controller, or
/// why not, in the report.
fn open_screen(parts: PanelParts, report: &mut Report) -> Option<Screen> {
    let Opened {
        probe,
        mut panel,
        mut lines,
    } = match panel::open(parts, |_| None) {
        Ok(opened) => opened,
        Err(error) => {
            note_unusable(report, &error);
            return None;
        }
    };
    let _ = write!(report, "panel {}: ", probe.controller().name());
    let mut frame = Vec::new();
    if frame.try_reserve_exact(FRAME_BYTES).is_err() {
        let _ = writeln!(report, "no memory");
        return None;
    }
    frame.resize(FRAME_BYTES, 0xFF);
    if !panel.begin(&mut lines) {
        let _ = writeln!(report, "begin failed");
        return None;
    }
    Some(Screen {
        panel,
        lines,
        frame,
    })
}

/// Why the panel cannot be used, in the report.
fn note_unusable(report: &mut Report, error: &OpenError) {
    let Some(found) = error.probe() else {
        let _ = writeln!(report, "panel: probe failed");
        return;
    };
    let why = match error {
        OpenError::Spi(_) => "no spi",
        // No driver for this controller yet: nothing more goes to it.
        OpenError::NoDriver(_) | OpenError::Probe => "skipped",
    };
    let _ = writeln!(report, "panel {}: {why}", found.controller().name());
}

/// "ok" or "failed", for the report.
fn outcome(ok: bool) -> &'static str {
    if ok { "ok" } else { "failed" }
}

/// Whole seconds and tenths since `start`, for the report.
fn took(start: Instant) -> (u64, u64) {
    let ms = start.elapsed().as_millis();
    (ms / 1000, ms % 1000 / 100)
}

impl Touches {
    /// The 15 s of touches, read every 20 ms from the controller at `addr`.
    async fn read<B: embedded_hal_async::i2c::I2c>(bus: &mut B, addr: u8) -> Self {
        let start = Instant::now();
        let mut touches = Touches::default();
        while Instant::now() - start < embassy_time::Duration::from_secs(15) {
            touches.add(purestwhite_gt911::read(bus, addr, &TOUCH).await);
            Timer::after_millis(20).await;
        }
        touches
    }

    fn add(&mut self, reading: Reading) {
        match reading {
            Reading::Frame(frame) => {
                self.frames += 1;
                self.most = self.most.max(frame.count);
                if let Some(c) = frame
                    .contacts
                    .get(..usize::from(frame.count))
                    .and_then(<[_]>::first)
                {
                    self.first.get_or_insert((c.x, c.y));
                    self.last = Some((c.x, c.y));
                }
                self.home(frame.home_key);
            }
            Reading::ContactsFailed { home_key } => {
                self.failed += 1;
                self.home(home_key);
            }
            Reading::Failed => self.failed += 1,
            Reading::NotReady => {}
        }
    }

    fn write(&self, report: &mut Report) {
        let point =
            |p: Option<(u16, u16)>| p.map_or((-1, -1), |(x, y)| (i32::from(x), i32::from(y)));
        let ((fx, fy), (lx, ly)) = (point(self.first), point(self.last));
        let _ = writeln!(
            report,
            "frames {} most {} first {fx},{fy} last {lx},{ly} home {} failed {}",
            self.frames, self.most, self.home_presses, self.failed
        );
    }
}

/// The shared I2C bus of the touch panel, the clock and the fuel gauge, at 400 kHz, after the
/// touch panel's supply (active low) has been on for 50 ms, as the C++ firmware has it before it
/// drives the bus; a stuck bus fails a transfer instead of waiting for the watchdog. `None`, noted
/// in the report, when it cannot be set up; and the supply, to keep on for the run.
async fn open_i2c(
    report: &mut Report,
    parts: I2cParts,
) -> (Option<I2c<'static, Async>>, Output<'static>) {
    let supply = Output::new(parts.supply, Level::Low, OutputConfig::default());
    Timer::after_millis(50).await;
    let config = I2cConfig::default()
        .with_frequency(Rate::from_khz(400))
        .with_software_timeout(SoftwareTimeout::Transaction(Duration::from_millis(50)));
    let bus = I2c::new(parts.i2c, config)
        .map(|i2c| i2c.with_sda(parts.sda).with_scl(parts.scl).into_async())
        .ok();
    if bus.is_none() {
        let _ = writeln!(report, "i2c: not set up");
    }
    (bus, supply)
}

/// The touch controller reset and found as the SDK does (`purestwhite_gt911::begin`): its address,
/// or `None` without a bus or an answer; and its reset pins, to keep for the run.
async fn begin_touch<B: embedded_hal_async::i2c::I2c>(
    bus: Option<&mut B>,
    parts: TouchParts,
) -> (Option<u8>, TouchPins<'static>) {
    let mut pins = TouchPins {
        // In reset until the address is selected, as the SDK's pinMode(OUTPUT) leaves it.
        reset: Output::new(parts.reset, Level::Low, OutputConfig::default()),
        int: Flex::new(parts.int),
    };
    pins.int.apply_output_config(&OutputConfig::default());
    let addr = match bus {
        Some(bus) => purestwhite_gt911::begin(bus, &mut Delay, Some(&mut pins), &TOUCH).await,
        None => None,
    };
    (addr, pins)
}

/// The clock's time and the battery's charge, each saved in the report as it is read.
async fn note_clock_and_battery<B>(report: &mut Report, bus: &mut B)
where
    B: embedded_hal::i2c::I2c + embedded_hal_async::i2c::I2c,
{
    note_clock(report, bus);
    report.save();
    note_battery(report, bus).await;
    report.save();
}

/// The clock's time, or why there is none, in the report.
fn note_clock<B: embedded_hal::i2c::I2c>(report: &mut Report, bus: &mut B) {
    if !purestwhite_rtc::begin(bus, &mut Delay, RTC_ADDR) {
        let _ = writeln!(report, "clock: not found");
        return;
    }
    let _ = match purestwhite_rtc::now(bus, RTC_ADDR) {
        Some(t) => writeln!(
            report,
            "clock: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        ),
        // Why: the read failed, or the chip says its time is not reliable.
        None => match clock_raw(bus) {
            Some([seconds, ..]) if seconds & 0x80 != 0 => {
                writeln!(report, "clock: not set (VL bit)")
            }
            Some(_) => writeln!(report, "clock: read, invalid"),
            None => writeln!(report, "clock: read failed"),
        },
    };
}

/// The battery's charge from the fuel gauge, in the report. The first reading may need a restart
/// of the gauge; a failed one retries after 1 s.
async fn note_battery<B: embedded_hal_async::i2c::I2c>(report: &mut Report, bus: &mut B) {
    let mut gauge = purestwhite_gauge::Cw2017::new();
    let mut soc = None;
    for attempt in 0..3 {
        if attempt > 0 {
            Timer::after_millis(1100).await;
        }
        #[allow(clippy::cast_possible_truncation)]
        let now = Instant::now().as_millis() as u32;
        soc = gauge.soc(bus, &mut Delay, GAUGE_ADDR, now).await;
        if soc.is_some() {
            break;
        }
    }
    let _ = match soc {
        Some(soc) => writeln!(report, "battery: {soc}%"),
        None => writeln!(report, "battery: read failed"),
    };
}

/// The screen black, the window touches are made in; `None` from then on when it fails, as the
/// controller's state is unknown.
fn paint_black(report: &mut Report, screen: Option<Screen>) -> Option<Screen> {
    let mut s = screen?;
    let start = Instant::now();
    let black = s.paint(0x00);
    let (secs, tenths) = took(start);
    let _ = writeln!(report, "black {} {secs}.{tenths} s", outcome(black));
    black.then_some(s)
}

/// The screen white again, then the panel asleep.
fn paint_white(report: &mut Report, s: &mut Screen) {
    let _ = write!(report, "white ");
    report.save();
    let start = Instant::now();
    let white = s.paint(0xFF);
    let (secs, tenths) = took(start);
    let slept = white && s.panel.deep_sleep(&mut s.lines);
    let _ = match (white, slept) {
        (true, true) => writeln!(report, "ok {secs}.{tenths} s"),
        (true, false) => writeln!(report, "ok {secs}.{tenths} s, sleep failed"),
        (false, _) => writeln!(report, "failed {secs}.{tenths} s"),
    };
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // Nothing to spawn yet; everything runs in main.
    let _ = spawner;
    let board = Board::take(esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    ));
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal smoke check", board.boot);
    esp_println::logger::init_logger_from_env();

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    let systimer = SystemTimer::new(board.rtos.systimer);
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);
    // No report line: the report has no room for it, and the lines after it show it.
    info!("esp-rtos started");

    let (mut bus, _touch_supply) = open_i2c(&mut report, board.i2c).await;
    if let Some(bus) = bus.as_mut() {
        note_clock_and_battery(&mut report, bus).await;
    }

    let (touch, _touch_pins) = begin_touch(bus.as_mut(), board.touch).await;
    note_touch(&mut report, touch);

    // The window on screen: black while touches are read, so they are made inside it. Only with
    // the touch controller found, which also tells an X4 Pro.
    let screen = touch.and_then(|_| open_screen(board.panel, &mut report));
    // Saved before each refresh: one that never ends leaves its line unfinished.
    report.save();
    let mut screen = paint_black(&mut report, screen);
    report.save();
    if let (Some(bus), Some(addr)) = (bus.as_mut(), touch) {
        Touches::read(bus, addr).await.write(&mut report);
    } else {
        Timer::after_secs(15).await;
    }
    report.save();
    if let Some(s) = screen.as_mut() {
        paint_white(&mut report, s);
    }
    let _ = writeln!(report, "done");

    info!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
