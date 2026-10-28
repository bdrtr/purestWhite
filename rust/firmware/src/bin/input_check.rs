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
//! ends shows its step (`sleep::Step`): `i` and the sleep interrupt flags found, `p` and the
//! digital pads' isolation and hold settings found (bit 11 is the auto-hold `sleep::deep_sleep`
//! disables), `a` arming Power (`!` if that failed), `r` holding the rails, `s` the deep sleep.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Flex, Input as Pin, InputConfig, Level, Output, OutputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, I2c, SoftwareTimeout};
use esp_hal::peripherals::LPWR;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::time::{Duration, Instant, Rate};
use purestwhite_firmware::input::{self, ButtonPins, Input, Polled, ReadyI2c, TouchPins};
use purestwhite_firmware::sleep::{self, Rails, Step};
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch, ready};

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
const MOST: Duration = Duration::from_secs(180);
/// How long Power may stay down before the sleep starts anyway.
const RELEASE_WAIT: Duration = Duration::from_secs(10);
/// The sleep's timer, in case Power does not wake it.
const SLEEP_AT_MOST: Duration = Duration::from_secs(60);
/// The watchdog's stages for the sleep, past its timer (see `sleep::watch_sleep`).
const WATCHDOG_CORE: Duration = Duration::from_secs(90);
const WATCHDOG_SYSTEM: Duration = Duration::from_secs(30);

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
        let _ = writeln!(report, "dugme: sol {left} sag {right} guc {power}");
        let _ = writeln!(
            report,
            "dokunus {} dokun {} kaydir {} uzun {} home {}/{}",
            self.touches,
            self.taps,
            self.swipes,
            self.long_presses,
            self.home_taps,
            self.home_long_presses
        );
        let _ = writeln!(
            report,
            "birak {} yavas {} diger {} hata {}",
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
            return "hazir degil";
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
            return "guc";
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
    let _ = writeln!(report, "purestWhite bare-metal giris denemesi");
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

    // The touch controller's supply, active low, on before the shared bus is driven, as the C++
    // firmware does.
    let delay = Delay::new();
    let touch_power = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    delay.delay_millis(50);
    let bus = I2c::new(
        peripherals.I2C0,
        // A stuck bus fails a transfer instead of waiting for the watchdog.
        I2cConfig::default()
            .with_frequency(Rate::from_khz(400))
            .with_software_timeout(SoftwareTimeout::Transaction(Duration::from_millis(50))),
    )
    .ok()
    .map(|i2c| {
        ReadyI2c(
            i2c.with_sda(peripherals.GPIO39)
                .with_scl(peripherals.GPIO38),
        )
    });
    let mut touch_pins = TouchPins {
        // In reset until the address is selected, as the SDK's pinMode(OUTPUT) leaves it.
        reset: Output::new(peripherals.GPIO4, Level::Low, OutputConfig::default()),
        int: Flex::new(peripherals.GPIO10),
    };
    touch_pins.int.apply_output_config(&OutputConfig::default());
    let touch = bus.and_then(|mut bus| {
        let found = ready(purestwhite_gt911::begin(
            &mut bus,
            &mut purestwhite_firmware::sd::SpinDelay(Delay::new()),
            Some(&mut touch_pins),
            &input::TOUCH,
        ))
        .flatten();
        found.map(|addr| (bus, addr))
    });
    match &touch {
        Some((_, addr)) => {
            let _ = writeln!(report, "dokunma: {addr:#04x}");
        }
        None => {
            let _ = writeln!(report, "dokunma: yok");
        }
    }
    report.save();

    let pulled_up = InputConfig::default().with_pull(Pull::Up);
    let pins = ButtonPins {
        left: Pin::new(peripherals.GPIO0, pulled_up),
        right: Pin::new(peripherals.GPIO7, pulled_up),
        power: Pin::new(peripherals.GPIO3, pulled_up),
    };
    let mut input = Input::new(pins, touch, input::POWER_HOLD_MS);
    let mut counts = Counts::default();
    esp_println::println!("giris: hazir");
    let why = watch(&mut input, &mut rtc, &mut counts);
    counts.write(&mut report);
    report.save();

    let mut pins = input.into_pins();
    if !sleep::wait_power_up(&pins.power, RELEASE_WAIT, || rtc.rwdt.feed()) {
        let _ = writeln!(report, "guc birakilmadi");
    }
    let rails = Rails {
        touch_power,
        sd_power: Output::new(peripherals.GPIO5, Level::High, OutputConfig::default()),
        panel_reset: Output::new(peripherals.GPIO14, Level::High, OutputConfig::default()),
    };
    sleep_reporting(
        &mut report,
        why,
        &mut rtc,
        &mut pins.power,
        rails,
        LowPower::new(peripherals.LPWR),
    )
}

/// Ends the report with the sleep's line, which the C++ firmware looks for to add what woke and
/// reset the chip, and sleeps deeply, marking each step on that line.
fn sleep_reporting(
    report: &mut Report,
    why: &str,
    rtc: &mut Rtc<'_>,
    power: &mut Pin<'_>,
    rails: Rails<'_>,
    low_power: LowPower<'_>,
) -> ! {
    // It starts with the sleep's interrupt flags and the pads' settings as the run found them.
    let _ = write!(report, "uyku: derin, {why}");
    let mut line = SleepLine::new(report);
    let regs = LPWR::regs();
    line.mark(format_args!(
        " i{:x} p{:x}",
        regs.int_raw().read().bits(),
        regs.dig_iso().read().bits()
    ));
    esp_println::println!("{}", line.report.as_str());
    sleep::watch_sleep(rtc, WATCHDOG_CORE, WATCHDOG_SYSTEM);
    // With a timer it does not return.
    let _ = sleep::deep_sleep(low_power, power, rails, Some(SLEEP_AT_MOST), |step| {
        line.mark(match step {
            Step::Arm => format_args!(" a"),
            Step::ArmFailed => format_args!("!"),
            Step::Rails => format_args!(" r"),
            Step::Sleep => format_args!(" s"),
        });
    });
    esp_hal::system::software_reset()
}

/// The report's last line, the sleep's, saved again with a mark before each step of the sleep: a
/// sleep that never ends shows the step it stopped at.
struct SleepLine<'r> {
    report: &'r mut Report,
    /// Where the line's text ends, before its newline.
    end: usize,
}

impl<'r> SleepLine<'r> {
    /// The line written last, without its newline yet.
    fn new(report: &'r mut Report) -> Self {
        let end = report.len();
        SleepLine { report, end }
    }

    fn mark(&mut self, text: core::fmt::Arguments<'_>) {
        self.report.truncate(self.end);
        let _ = self.report.write_fmt(text);
        self.end = self.report.len();
        let _ = writeln!(self.report);
        self.report.save();
    }
}
