//! Feedback on the X4 Pro's panel from the bare-metal firmware, the first image that answers
//! input on screen: hands the next boot back to the C++ firmware, starts the panel and the touch
//! panel, draws four key boxes (Left, Right, Power, Home), then polls the input every 15 ms
//! (`purestwhite_firmware::input`) and draws what it gives with a fast refresh: a tick under a
//! key's box for each press (for Home, each tap), a dot where a tap touched down, a ring where a
//! contact was held, a line with a dot at its end for a swipe. Until the C++ firmware would sleep:
//! Power held longer than 400 ms, or 60 s without input, or at the latest after 5 minutes. Then it
//! reports what it counted, draws a crescent moon with a full refresh, puts the panel to sleep,
//! and sleeps deeply until Power is pressed or for at most 60 s (`test_image::report_and_sleep`); the
//! wake boots the C++ firmware.
//!
//! The refreshes block: input during one is not read until it ends. Without esp-rtos, so it runs
//! in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::vec;
use bembeyaz_gfx::{Canvas, Clip, Corners, Fill, Orientation};
use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input as Pin, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::time::{Duration, Instant};
use purestwhite_epd::uc8279x4::Refresh;
use purestwhite_firmware::input::{self, ButtonPins, Input, Polled};
use purestwhite_firmware::panel::{Lines, Panel, PanelPins};
use purestwhite_firmware::sleep::{self, Rails};
use purestwhite_firmware::test_image::{self, TestSleep};
use purestwhite_firmware::test_image::{Report, arm_watchdog, hand_back};
use purestwhite_firmware::{hold_power_latch, ready};
use purestwhite_touch::Point;

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

/// How long without input puts the device to sleep, and how long it runs at most.
const IDLE: Duration = Duration::from_secs(60);
const MOST: Duration = Duration::from_secs(300);
/// How long Power may stay down before the sleep starts anyway.
const RELEASE_WAIT: Duration = Duration::from_secs(10);
/// The sleep: until Power, or for at most 60 s; the watchdog resets the chip if it has not
/// woken after 90 s, and fully 30 s later.
const TEST_SLEEP: TestSleep = TestSleep {
    timer: Duration::from_secs(60),
    core: Duration::from_secs(90),
    system: Duration::from_secs(30),
};

/// The panel in its own layout: 800 by 480 pixels, a bit each, 1 white, in 100-byte rows. It is
/// drawn upright as the device is held: 480 wide, 800 high.
const PANEL_WIDTH: i32 = 800;
const PANEL_HEIGHT: i32 = 480;
const STRIDE: usize = 100;
const FRAME_BYTES: usize = STRIDE * PANEL_HEIGHT as usize;
const WIDTH: i32 = PANEL_HEIGHT;
const HEIGHT: i32 = PANEL_WIDTH;
/// After this many fast refreshes, a half refresh clears what they left behind.
const FAST_RUN: u32 = 15;

/// The key boxes along the top, in the order Left, Right, Power, Home.
const KEYS: usize = 4;
const KEY_TOP: i32 = 20;
const KEY_WIDTH: i32 = 100;
const KEY_HEIGHT: i32 = 80;
/// A box's left edge.
fn key_left(key: usize) -> i32 {
    // At most 4, so the cast is exact.
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
    let key = key as i32;
    15 + key * 115
}
/// Ticks under a box: in rows of `TICKS_PER_ROW`, `TICK_ROWS` of them, then they wrap.
const TICK: i32 = 10;
const TICK_STEP: i32 = 14;
const TICKS_PER_ROW: u32 = 7;
const TICK_ROWS: u32 = 3;

/// Milliseconds since boot, wrapping as `millis()` does.
#[allow(clippy::cast_possible_truncation, reason = "wraps on purpose")]
fn millis() -> u32 {
    Instant::now().duration_since_epoch().as_millis() as u32
}

/// The frame as `bembeyaz_gfx` draws into it, upright.
fn canvas(frame: &mut [u8]) -> Canvas<'_> {
    Canvas {
        buf: frame,
        stride: STRIDE,
        origin_y: 0,
        rows: PANEL_HEIGHT,
        panel_width: PANEL_WIDTH,
        panel_height: PANEL_HEIGHT,
        orientation: Orientation::Portrait,
        clip: Clip {
            left: 0,
            top: 0,
            right: WIDTH,
            bottom: HEIGHT,
        },
    }
}

/// A disc of radius `r` around (`x`, `y`).
fn disc(c: &mut Canvas<'_>, x: i32, y: i32, r: i32, fill: Fill) {
    c.fill_rounded_rect(x - r, y - r, 2 * r, 2 * r, r, Corners::ALL, fill);
}

/// The four boxes, each with its sign: an arrow left, an arrow right, a ring, a house.
fn draw_keys(c: &mut Canvas<'_>) {
    for key in 0..KEYS {
        let left = key_left(key);
        c.rect(left, KEY_TOP, KEY_WIDTH, KEY_HEIGHT, 3, true);
        let (cx, cy) = (left + KEY_WIDTH / 2, KEY_TOP + KEY_HEIGHT / 2);
        match key {
            0 => c.fill_polygon(
                &[cx - 25, cx + 20, cx + 20],
                &[cy, cy - 25, cy + 25],
                Fill::Black,
            ),
            1 => c.fill_polygon(
                &[cx + 25, cx - 20, cx - 20],
                &[cy, cy - 25, cy + 25],
                Fill::Black,
            ),
            2 => {
                disc(c, cx, cy, 25, Fill::Black);
                disc(c, cx, cy, 18, Fill::White);
            }
            _ => {
                c.fill_polygon(
                    &[cx - 28, cx, cx + 28],
                    &[cy - 2, cy - 28, cy - 2],
                    Fill::Black,
                );
                c.fill_rect(cx - 20, cy - 2, 40, 28, Fill::Black);
            }
        }
    }
}

/// The `n`th tick (from 0) under `key`'s box; the ticks wrap after `TICKS_PER_ROW * TICK_ROWS`,
/// clearing the space first.
fn draw_tick(c: &mut Canvas<'_>, key: usize, n: u32) {
    let n = n % (TICKS_PER_ROW * TICK_ROWS);
    let left = key_left(key);
    let top = KEY_TOP + KEY_HEIGHT + 8;
    if n == 0 {
        #[allow(clippy::cast_possible_wrap, reason = "3")]
        c.fill_rect(
            left,
            top,
            KEY_WIDTH,
            TICK_STEP * TICK_ROWS as i32,
            Fill::White,
        );
    }
    #[allow(clippy::cast_possible_wrap, reason = "below 21")]
    let (column, row) = ((n % TICKS_PER_ROW) as i32, (n / TICKS_PER_ROW) as i32);
    c.fill_rect(
        left + column * TICK_STEP,
        top + row * TICK_STEP,
        TICK,
        TICK,
        Fill::Black,
    );
}

/// The crescent moon of the sleep screen.
fn draw_moon(c: &mut Canvas<'_>) {
    c.fill_rect(0, 0, WIDTH, HEIGHT, Fill::White);
    disc(c, WIDTH / 2, HEIGHT / 2, 90, Fill::Black);
    disc(c, WIDTH / 2 + 40, HEIGHT / 2 - 25, 80, Fill::White);
}

/// What the run did, for the report.
#[derive(Default)]
struct Counts {
    /// Presses of Left, Right, Power and taps of Home.
    keys: [u32; KEYS],
    taps: u32,
    swipes: u32,
    long_presses: u32,
    fast: u32,
    half: u32,
    failed: u32,
    /// The slowest refresh, in milliseconds.
    slowest_ms: u64,
    /// Fast refreshes since the last half or full one.
    fast_run: u32,
}

impl Counts {
    /// Draws what `polled` gave; whether anything was drawn.
    fn draw(&mut self, polled: &Polled, c: &mut Canvas<'_>) -> bool {
        let mut drawn = false;
        let mut tick = |key: usize, keys: &mut [u32; KEYS], c: &mut Canvas<'_>| {
            if let Some(count) = keys.get_mut(key) {
                draw_tick(c, key, *count);
                *count += 1;
                drawn = true;
            }
        };
        for key in 0..3 {
            if polled.buttons.pressed & (1 << key) != 0 {
                tick(key, &mut self.keys, c);
            }
        }
        let t = &polled.touch;
        if t.home_tapped {
            tick(3, &mut self.keys, c);
        }
        // Touch points are panel pixels: the touch panel's range is the panel's 800 by 480. The
        // C++ firmware maps them through fractions (`tapToLogical`); without floats here, which
        // eink-qemu 0.9.2 fails on.
        let at = |c: &Canvas<'_>, p: Point| c.panel_to_logical(i32::from(p.x), i32::from(p.y));
        if let Some(p) = t.tap {
            let (x, y) = at(c, p);
            disc(c, x, y, 10, Fill::Black);
            self.taps += 1;
            drawn = true;
        }
        if let Some(p) = t.long_press {
            let (x, y) = at(c, p);
            disc(c, x, y, 24, Fill::Black);
            disc(c, x, y, 19, Fill::White);
            self.long_presses += 1;
            drawn = true;
        }
        if let Some((start, end)) = t.swipe {
            let ((x1, y1), (x2, y2)) = (at(c, start), at(c, end));
            c.thick_line(x1, y1, x2, y2, 5, true);
            disc(c, x2, y2, 9, Fill::Black);
            self.swipes += 1;
            drawn = true;
        }
        drawn
    }

    fn write(&self, report: &mut Report) {
        // Short, so that the sleep line's marks keep their room in the report's 248 bytes.
        let _ = writeln!(
            report,
            "hizli {} yarim {} hata {} {} ms",
            self.fast, self.half, self.failed, self.slowest_ms
        );
        let [left, right, power, home] = self.keys;
        let _ = writeln!(
            report,
            "tus {left}/{right}/{power}/{home} dokun {} kaydir {} uzun {}",
            self.taps, self.swipes, self.long_presses
        );
    }
}

/// The panel, while it answers: nothing more goes to it after a call that failed.
struct Screen<'d> {
    panel: Panel,
    lines: Lines<'d>,
    working: bool,
}

impl Screen<'_> {
    /// Shows `frame`, a half refresh after `FAST_RUN` fast ones; notes it in `counts`.
    fn show(&mut self, frame: &[u8], counts: &mut Counts, full: bool) {
        if !self.working {
            return;
        }
        let refresh = if full {
            counts.fast_run = 0;
            Refresh::Full
        } else if counts.fast_run >= FAST_RUN {
            counts.fast_run = 0;
            counts.half += 1;
            Refresh::Half
        } else {
            counts.fast_run += 1;
            Refresh::Fast
        };
        let start = Instant::now();
        self.working = self.panel.display(&mut self.lines, frame, refresh, false);
        counts.slowest_ms = counts.slowest_ms.max(start.elapsed().as_millis());
        if refresh == Refresh::Fast {
            counts.fast += 1;
        }
        if !self.working {
            counts.failed += 1;
        }
    }
}

/// The panel: which controller (the lines as GPIOs for a moment, as the C++ firmware probes at
/// every boot), then its driver over SPI, started. `None` when it cannot be used; the
/// controller's name either way.
fn open_screen(mut pins: PanelPins<'_>) -> (Option<Screen<'_>>, &'static str) {
    let found = pins.probe();
    let name = found.map_or("yok", |f| f.controller().name());
    let panel = found.and_then(|f| Panel::for_controller(f.controller(), None));
    let (Some(mut panel), Some(mut lines)) = (panel, pins.open()) else {
        return (None, name);
    };
    let working = panel.begin(&mut lines);
    (
        Some(Screen {
            panel,
            lines,
            working,
        }),
        name,
    )
}

/// Polls the input every `POLL_MS` and draws what it gives, until the C++ firmware would sleep:
/// why, in a word or two.
fn watch<B: embedded_hal_async::i2c::I2c>(
    input: &mut Input<'_, B>,
    screen: &mut Screen<'_>,
    frame: &mut [u8],
    counts: &mut Counts,
    rtc: &mut Rtc<'_>,
) -> &'static str {
    let delay = Delay::new();
    let start = Instant::now();
    let mut last_activity = start;
    loop {
        let Some(polled) = ready(input.poll(millis())) else {
            return "hazir degil";
        };
        if polled.touch.long_press.is_some() {
            // As the C++ firmware's callers: the lift of a long press does not also tap.
            input.suppress_touch();
        }
        if polled.activity() {
            last_activity = Instant::now();
        }
        if polled.sleep {
            return "guc";
        }
        if counts.draw(&polled, &mut canvas(frame)) {
            screen.show(frame, counts, false);
        }
        if last_activity.elapsed() >= IDLE {
            return "bos 60 s";
        }
        if start.elapsed() >= MOST {
            return "sure doldu";
        }
        rtc.rwdt.feed();
        delay.delay_millis(input::POLL_MS);
    }
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let mut rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal geri bildirim");
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
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    let pins = PanelPins {
        spi: peripherals.SPI2,
        sclk: peripherals.GPIO12,
        mosi: peripherals.GPIO11,
        cs: peripherals.GPIO13,
        dc: peripherals.GPIO18,
        rst: peripherals.GPIO14,
        busy: peripherals.GPIO6,
    };
    let (screen, name) = open_screen(pins);
    let Some(mut screen) = screen else {
        let _ = writeln!(report, "ekran {name}: kullanilamadi");
        report.save();
        esp_hal::system::software_reset()
    };
    let mut frame = vec![0xFFu8; FRAME_BYTES];
    let mut counts = Counts::default();
    draw_keys(&mut canvas(&mut frame));
    screen.show(&frame, &mut counts, true);
    rtc.rwdt.feed();

    // The touch controller's supply, active low, on before the shared bus is driven.
    let delay = Delay::new();
    let touch_power = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    delay.delay_millis(50);
    let (touch, _touch_pins) = test_image::open_touch(
        peripherals.I2C0,
        peripherals.GPIO39,
        peripherals.GPIO38,
        peripherals.GPIO4,
        peripherals.GPIO10,
    );
    let touch_note = touch.as_ref().map_or(0, |(_, addr)| *addr);
    let _ = writeln!(
        report,
        "ekran {name} {}, dokunma {touch_note:#04x}",
        if screen.working { "tamam" } else { "hata" }
    );
    report.save();

    let pulled_up = InputConfig::default().with_pull(Pull::Up);
    let mut input = Input::new(
        ButtonPins {
            left: Pin::new(peripherals.GPIO0, pulled_up),
            right: Pin::new(peripherals.GPIO7, pulled_up),
            power: Pin::new(peripherals.GPIO3, pulled_up),
        },
        touch,
        input::POWER_HOLD_MS,
    );
    esp_println::println!("geri bildirim: hazir");
    let why = watch(&mut input, &mut screen, &mut frame, &mut counts, &mut rtc);
    counts.write(&mut report);
    report.save();

    // The sleep screen, then the panel asleep, as the C++ firmware leaves it.
    draw_moon(&mut canvas(&mut frame));
    rtc.rwdt.feed();
    screen.show(&frame, &mut counts, true);
    let asleep = screen.working && screen.panel.deep_sleep(&mut screen.lines);
    if !asleep {
        let _ = writeln!(report, "ekran uyumadi");
    }
    esp_println::println!("geri bildirim: ay");

    let mut pins = input.into_pins();
    if !sleep::wait_power_up(&pins.power, RELEASE_WAIT, || rtc.rwdt.feed()) {
        let _ = writeln!(report, "guc birakilmadi");
    }
    let rails = Rails {
        touch_power,
        sd_power: Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        panel_reset: screen.lines.rst,
    };
    test_image::report_and_sleep(
        &mut report,
        why,
        &mut rtc,
        &mut pins.power,
        rails,
        LowPower::new(peripherals.LPWR),
        TEST_SLEEP,
    )
}
