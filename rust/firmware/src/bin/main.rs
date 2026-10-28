//! Smoke test of the bare-metal firmware, for the second OTA slot: starts esp-rtos and Embassy,
//! runs our clock, fuel gauge and touch drivers (purestwhite-rtc, purestwhite-gauge,
//! purestwhite-gt911) on esp-hal's I2C, reads touches for 15 s with the panel black meanwhile and
//! white after (on a UC8279 or UC8179, with purestwhite-epd), then leaves a text report in RTC slow
//! memory and resets.
//!
//! First it arms the RTC watchdog and hands the next boot back to the C++ firmware (see
//! `hand_back`), which then saves the report to the SD card. The report is saved after every
//! stage, and a panic adds a line to it and resets.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use alloc::vec::Vec;
use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_time::{Delay, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Flex, Level, Output, OutputConfig};
use esp_hal::i2c::master::{Config as I2cConfig, I2c, SoftwareTimeout};
use esp_hal::time::{Duration, Rate};
use esp_hal::timer::systimer::SystemTimer;
use log::info;
use purestwhite_epd::uc8279x4::Refresh;
use purestwhite_firmware::input::{TOUCH, TouchPins};
use purestwhite_firmware::panel::{Lines, Panel, PanelPins};
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch};
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
    Report::append_saved("PANIK\n");
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
struct Screen<'d> {
    panel: Panel,
    lines: Lines<'d>,
    frame: Vec<u8>,
}

impl Screen<'_> {
    /// The whole panel `fill` (0x00 black, 0xFF white) in a full refresh, powered off after.
    fn paint(&mut self, fill: u8) -> bool {
        self.frame.fill(fill);
        self.panel
            .display(&mut self.lines, &self.frame, Refresh::Full, true)
    }
}

/// Probes the panel and sets it up when a driver here can draw on it; notes the controller, or
/// why not, in the report.
fn open_screen<'d>(mut pins: PanelPins<'d>, report: &mut Report) -> Option<Screen<'d>> {
    let Some(found) = pins.probe() else {
        let _ = writeln!(report, "ekran: yoklanamadi");
        return None;
    };
    let controller = found.controller();
    let _ = write!(report, "ekran {}: ", controller.name());
    let Some(mut panel) = Panel::for_controller(controller, None) else {
        // No driver for this controller yet: nothing more goes to it.
        let _ = writeln!(report, "atlandi");
        return None;
    };
    let mut frame = Vec::new();
    if frame.try_reserve_exact(FRAME_BYTES).is_err() {
        let _ = writeln!(report, "bellek yok");
        return None;
    }
    frame.resize(FRAME_BYTES, 0xFF);
    let Some(mut lines) = pins.open() else {
        let _ = writeln!(report, "spi yok");
        return None;
    };
    if !panel.begin(&mut lines) {
        let _ = writeln!(report, "baslatilamadi");
        return None;
    }
    Some(Screen {
        panel,
        lines,
        frame,
    })
}

/// "tamam" or "hata", for the report.
fn outcome(ok: bool) -> &'static str {
    if ok { "tamam" } else { "hata" }
}

/// Whole seconds and tenths since `start`, for the report.
fn took(start: Instant) -> (u64, u64) {
    let ms = start.elapsed().as_millis();
    (ms / 1000, ms % 1000 / 100)
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // Nothing to spawn yet; everything runs in main.
    let _ = spawner;
    let peripherals = esp_hal::init(esp_hal::Config::default().with_cpu_clock(CpuClock::max()));
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    // A report at once, so a panic from here on is not added to an older one.
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal deneme");
    report.save();
    let handed_back = hand_back(peripherals.FLASH, peripherals.CPU_CTRL);
    esp_println::logger::init_logger_from_env();

    match handed_back {
        // The bootloader's state is left out: the report has no room for it, and the rollback
        // check reports it.
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
    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(systimer.alarm0, peripherals.FROM_CPU_INTR0);
    // No report line: the report has no room for it, and the lines after it show it.
    info!("esp-rtos started");

    // The touch controller's active-low supply: the C++ firmware powers it before using the
    // shared bus, so the bus is driven the same way here.
    let _touch = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    Timer::after_millis(50).await;

    let i2c = I2c::new(
        peripherals.I2C0,
        // A stuck bus fails a transfer instead of waiting for the watchdog.
        I2cConfig::default()
            .with_frequency(Rate::from_khz(400))
            .with_software_timeout(SoftwareTimeout::Transaction(Duration::from_millis(50))),
    )
    .map(|i2c| {
        i2c.with_sda(peripherals.GPIO39)
            .with_scl(peripherals.GPIO38)
            .into_async()
    });
    let mut bus = i2c.ok();
    if bus.is_none() {
        let _ = writeln!(report, "i2c: kurulamadi");
    }

    if let Some(bus) = bus.as_mut() {
        let mut delay = Delay;
        if purestwhite_rtc::begin(bus, &mut delay, RTC_ADDR) {
            match purestwhite_rtc::now(bus, RTC_ADDR) {
                Some(t) => {
                    let _ = writeln!(
                        report,
                        "saat: {:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
                        t.year, t.month, t.day, t.hour, t.minute, t.second
                    );
                }
                None => {
                    // Why: the read failed, or the chip says its time is not reliable.
                    match clock_raw(bus) {
                        Some([seconds, ..]) if seconds & 0x80 != 0 => {
                            let _ = writeln!(report, "saat: ayarlanmamis (VL biti)");
                        }
                        Some(_) => {
                            let _ = writeln!(report, "saat: okundu, gecersiz");
                        }
                        None => {
                            let _ = writeln!(report, "saat: okuma hatasi");
                        }
                    }
                }
            }
        } else {
            let _ = writeln!(report, "saat: bulunamadi");
        }
        report.save();

        let mut gauge = purestwhite_gauge::Cw2017::new();
        let mut soc = None;
        // The first reading may need a restart of the gauge; a failed one retries after 1 s.
        for attempt in 0..3 {
            if attempt > 0 {
                Timer::after_millis(1100).await;
            }
            #[allow(clippy::cast_possible_truncation)]
            let now = Instant::now().as_millis() as u32;
            soc = gauge.soc(bus, &mut delay, GAUGE_ADDR, now).await;
            if soc.is_some() {
                break;
            }
        }
        match soc {
            Some(soc) => {
                let _ = writeln!(report, "pil: %{soc}");
            }
            None => {
                let _ = writeln!(report, "pil: okunamadi");
            }
        }
        report.save();
    }

    let mut touch_pins = TouchPins {
        // In reset until the address is selected, as the SDK's pinMode(OUTPUT) leaves it.
        reset: Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default()),
        int: Flex::new(peripherals.GPIO10),
    };
    touch_pins.int.apply_output_config(&OutputConfig::default());
    let touch = match bus.as_mut() {
        Some(bus) => purestwhite_gt911::begin(bus, &mut Delay, Some(&mut touch_pins), &TOUCH).await,
        None => None,
    };
    match touch {
        Some(addr) => {
            let _ = writeln!(report, "dokunma: {addr:#04x}");
        }
        None => {
            let _ = writeln!(report, "dokunma: yok");
        }
    }
    report.save();

    // The window on screen: black while touches are read, so they are made inside it. Only with
    // the touch controller found, which also tells an X4 Pro.
    let pins = PanelPins {
        spi: peripherals.SPI2,
        sclk: peripherals.GPIO12,
        mosi: peripherals.GPIO11,
        cs: peripherals.GPIO13,
        dc: peripherals.GPIO18,
        rst: peripherals.GPIO14,
        busy: peripherals.GPIO6,
    };
    let mut screen = touch.and_then(|_| open_screen(pins, &mut report));
    // Saved before each refresh: one that never ends leaves its line unfinished.
    report.save();
    if let Some(s) = screen.as_mut() {
        let start = Instant::now();
        let black = s.paint(0x00);
        let (secs, tenths) = took(start);
        let _ = writeln!(report, "siyah {} {secs}.{tenths} s", outcome(black));
        if !black {
            // The controller's state is unknown: nothing more goes to it.
            screen = None;
        }
    }
    report.save();

    // Touches for 15 s, read every 20 ms.
    let start = Instant::now();
    let mut touches = Touches::default();
    while Instant::now() - start < embassy_time::Duration::from_secs(15) {
        if let (Some(bus), Some(addr)) = (bus.as_mut(), touch) {
            match purestwhite_gt911::read(bus, addr, &TOUCH).await {
                Reading::Frame(frame) => {
                    touches.frames += 1;
                    touches.most = touches.most.max(frame.count);
                    if let Some(c) = frame
                        .contacts
                        .get(..usize::from(frame.count))
                        .and_then(<[_]>::first)
                    {
                        touches.first.get_or_insert((c.x, c.y));
                        touches.last = Some((c.x, c.y));
                    }
                    touches.home(frame.home_key);
                }
                Reading::ContactsFailed { home_key } => {
                    touches.failed += 1;
                    touches.home(home_key);
                }
                Reading::Failed => touches.failed += 1,
                Reading::NotReady => {}
            }
        }
        Timer::after_millis(20).await;
    }
    if touch.is_some() {
        let point =
            |p: Option<(u16, u16)>| p.map_or((-1, -1), |(x, y)| (i32::from(x), i32::from(y)));
        let ((fx, fy), (lx, ly)) = (point(touches.first), point(touches.last));
        let _ = writeln!(
            report,
            "kare {} cok {} ilk {fx},{fy} son {lx},{ly} home {} hata {}",
            touches.frames, touches.most, touches.home_presses, touches.failed
        );
    }
    report.save();
    if let Some(s) = screen.as_mut() {
        let _ = write!(report, "beyaz ");
        report.save();
        let start = Instant::now();
        let white = s.paint(0xFF);
        let (secs, tenths) = took(start);
        let slept = white && s.panel.deep_sleep(&mut s.lines);
        let _ = match (white, slept) {
            (true, true) => writeln!(report, "tamam {secs}.{tenths} s"),
            (true, false) => writeln!(report, "tamam {secs}.{tenths} s, uyku hata"),
            (false, _) => writeln!(report, "hata {secs}.{tenths} s"),
        };
    }
    let _ = writeln!(report, "bitti");

    info!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
