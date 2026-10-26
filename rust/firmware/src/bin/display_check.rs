//! The X4 Pro's panel from the bare-metal firmware: hands the next boot back to the C++
//! firmware, then draws a test pattern with purestwhite-epd's UC8279 driver (a full refresh,
//! then a fast one with the block moved), holds it 3 s, puts the controller to sleep and resets.
//! Without esp-rtos, so it runs in the emulator too: the driver's async calls go over blocking
//! SPI and delays, ready at once.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::vec;
use core::fmt::Write as _;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Level, Output, OutputConfig};
use esp_hal::spi::master::{Config as SpiConfig, Spi};
use esp_hal::time::Rate;
use purestwhite_epd::Bus;
use purestwhite_epd::uc8279x4::{Refresh, Uc8279X4, X4_PRO};
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch};

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

/// Blocking SPI as embedded-hal-async's: each future is ready at once.
struct BlockingSpi<S>(S);

impl<S: embedded_hal::spi::SpiBus> embedded_hal_async::spi::ErrorType for BlockingSpi<S> {
    type Error = S::Error;
}

#[allow(clippy::unused_async, reason = "blocking calls, ready at once")]
impl<S: embedded_hal::spi::SpiBus> embedded_hal_async::spi::SpiBus for BlockingSpi<S> {
    async fn read(&mut self, words: &mut [u8]) -> Result<(), S::Error> {
        self.0.read(words)
    }
    async fn write(&mut self, words: &[u8]) -> Result<(), S::Error> {
        self.0.write(words)
    }
    async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), S::Error> {
        self.0.transfer(read, write)
    }
    async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), S::Error> {
        self.0.transfer_in_place(words)
    }
    async fn flush(&mut self) -> Result<(), S::Error> {
        self.0.flush()
    }
}

/// esp-hal's busy-wait delay as embedded-hal-async's.
struct BlockingDelay(Delay);

#[allow(clippy::unused_async, reason = "blocking calls, ready at once")]
impl embedded_hal_async::delay::DelayNs for BlockingDelay {
    async fn delay_ns(&mut self, ns: u32) {
        embedded_hal::delay::DelayNs::delay_ns(&mut self.0, ns);
    }
}

/// Runs a future whose awaits are all ready at once; `None` if one is not.
fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// White with a 16-pixel black border, a black block whose left edge is `block` bytes in, and
/// bars across the middle; a bit per pixel, 1 white.
fn pattern(frame: &mut [u8], block: usize) {
    for (y, row) in frame.chunks_exact_mut(100).enumerate() {
        row.fill(0xFF);
        if !(16..464).contains(&y) {
            row.fill(0x00);
            continue;
        }
        for edge in [0, 1, 98, 99] {
            if let Some(b) = row.get_mut(edge) {
                *b = 0x00;
            }
        }
        if (100..380).contains(&y) {
            for b in row.iter_mut().skip(block).take(20) {
                *b = 0x00;
            }
        }
        if (200..280).contains(&y) {
            for (i, b) in row.iter_mut().enumerate().take(92).skip(50) {
                if (i / 5) % 2 == 0 {
                    *b = 0x00;
                }
            }
        }
    }
}

#[esp_hal::main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal ekran denemesi");
    report.save();
    match hand_back(peripherals.FLASH, peripherals.CPU_CTRL) {
        Ok(_) => {
            let _ = writeln!(report, "sonraki acilis: C++ yazilimi");
        }
        Err(e) => {
            let _ = writeln!(report, "sonraki acilis: SECILEMEDI ({e})");
        }
    }
    report.save();
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // The X4 Pro's panel lines (freeink-sdk's board profile): SCLK 12, MOSI 11, CS 13, DC 18,
    // RST 14, BUSY 6; 10 MHz, mode 0.
    let spi = Spi::new(
        peripherals.SPI2,
        SpiConfig::default().with_frequency(Rate::from_mhz(10)),
    )
    .map(|spi| {
        spi.with_sck(peripherals.GPIO12)
            .with_mosi(peripherals.GPIO11)
    });
    let Ok(spi) = spi else {
        let _ = writeln!(report, "ekran: spi kurulamadi");
        report.save();
        esp_hal::system::software_reset()
    };
    let mut bus = Bus {
        spi: BlockingSpi(spi),
        cs: Output::new(peripherals.GPIO13, Level::High, OutputConfig::default()),
        dc: Output::new(peripherals.GPIO18, Level::High, OutputConfig::default()),
        rst: Output::new(peripherals.GPIO14, Level::High, OutputConfig::default()),
        busy: Input::new(peripherals.GPIO6, InputConfig::default()),
        delay: BlockingDelay(Delay::new()),
    };

    let mut panel = Uc8279X4::new(X4_PRO);
    let mut frame = vec![0xFFu8; panel.row_bytes() * 480];
    let delay = Delay::new();
    let begun = matches!(ready(panel.begin(&mut bus)), Some(Ok(())));
    pattern(&mut frame, 10);
    let full = begun
        && matches!(
            ready(panel.display(&mut bus, &frame, Refresh::Full, false)),
            Some(Ok(()))
        );
    let _ = writeln!(report, "ekran tam: {}", if full { "tamam" } else { "hata" });
    report.save();
    esp_println::println!("ekran tam: {full}");
    delay.delay_millis(2000);

    pattern(&mut frame, 60);
    let fast = full
        && matches!(
            ready(panel.display(&mut bus, &frame, Refresh::Fast, true)),
            Some(Ok(()))
        );
    let _ = writeln!(
        report,
        "ekran hizli: {}",
        if fast { "tamam" } else { "hata" }
    );
    report.save();
    esp_println::println!("ekran hizli: {fast}");
    delay.delay_millis(3000);

    let slept = matches!(ready(panel.deep_sleep(&mut bus)), Some(Ok(())));
    let _ = writeln!(
        report,
        "ekran uyku: {}",
        if slept { "tamam" } else { "hata" }
    );
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
