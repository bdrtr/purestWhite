//! Smoke test of the bare-metal firmware, for the second OTA slot: starts esp-rtos and Embassy,
//! runs our clock, fuel gauge and touch drivers (purestwhite-rtc, purestwhite-gauge,
//! purestwhite-gt911) on esp-hal's I2C, reads touches for 15 s while timing Embassy's timer
//! against the clock chip, then leaves a text report in RTC slow memory and resets.
//!
//! First it arms the RTC watchdog and hands the next boot back to the C++ firmware (see
//! `hand_back`), which then saves the report to the SD card. The report is saved after every
//! stage, and a panic adds a line to it and resets.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

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
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch};
use purestwhite_gt911::{Reading, ResetPins};

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

/// The X4 Pro's touch controller, as freeink-sdk's board profile has it.
const TOUCH: purestwhite_gt911::Config = purestwhite_gt911::Config {
    addr: 0x5D,
    alt_addr: 0x14,
    raw_min_x: 0,
    raw_max_x: 799,
    raw_min_y: 0,
    raw_max_y: 479,
    swap_xy: true,
    flip_x: false,
    flip_y: true,
    coords_at_byte0: true,
};

/// The touch controller's RST (GPIO4) and INT (GPIO10).
struct TouchPins<'d> {
    reset: Output<'d>,
    int: Flex<'d>,
}

impl ResetPins for TouchPins<'_> {
    fn set_reset(&mut self, high: bool) {
        self.reset.set_level(Level::from(high));
    }
    fn drive_int(&mut self, high: bool) {
        // The level first, so the pin starts driving it.
        self.int.set_level(Level::from(high));
        self.int.set_output_enable(true);
    }
    fn release_int(&mut self) {
        self.int.set_output_enable(false);
        self.int.set_input_enable(true);
    }
}

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

/// Seconds since midnight from the clock chip's registers, whether or not its time was ever set:
/// the oscillator runs either way once powered.
fn clock_seconds<B: embedded_hal::i2c::I2c>(bus: &mut B) -> Option<u32> {
    let bcd = |v: u8| u32::from(v >> 4) * 10 + u32::from(v & 0x0F);
    clock_raw(bus).map(|[s, m, h]| bcd(h & 0x3F) * 3600 + bcd(m & 0x7F) * 60 + bcd(s & 0x7F))
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
        Ok(found) => {
            let _ = writeln!(report, "sonraki acilis: C++ yazilimi");
            if let Some(state) = found.booted_state {
                let _ = writeln!(report, "onyukleyici durumu: {state}");
            }
        }
        Err(e) => {
            let _ = writeln!(report, "sonraki acilis: SECILEMEDI ({e})");
        }
    }
    report.save();

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    let systimer = SystemTimer::new(peripherals.SYSTIMER);
    esp_rtos::start(systimer.alarm0, peripherals.FROM_CPU_INTR0);
    let _ = writeln!(report, "esp-rtos: basladi");
    report.save();
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

    // Touches for 15 s, read every 20 ms; meanwhile Embassy's timer against the clock chip, which
    // should agree give or take its one-second resolution.
    let clock_start = bus.as_mut().and_then(clock_seconds);
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
    let clock_end = bus.as_mut().and_then(clock_seconds);
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
    match (clock_start, clock_end) {
        (Some(a), Some(b)) => {
            let _ = writeln!(
                report,
                "zaman: 15 s = saat {} s",
                b.wrapping_sub(a).wrapping_add(86_400) % 86_400
            );
        }
        _ => {
            let _ = writeln!(report, "zaman: saat yok");
        }
    }
    let _ = writeln!(report, "bitti");

    info!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
