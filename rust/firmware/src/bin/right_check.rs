//! The Right key as an image starts (rust/hardware-tests/bare-metal.md, the recovery mode): the
//! `purestwhite` image enters its recovery mode when Right (GPIO7) is low in five samples 10 ms
//! apart as it starts. On the device it never read low there, nor in right-check's first version
//! for 30 s, while the archived tasks-check image (f71f09f1's) still counts Right on the same
//! device. This image starts as `purestwhite` does (the CPU at its fastest, `start`), and looks at
//! GPIO7's pad three times: as the image is entered, after `start`, and with Right set up as an
//! input, pulled up (`Pad`). Then it samples Right in three phases of 4 s, on the digital input
//! and on the RTC input: as set up; with SPI1's MISC register back at its value on entry (`start`
//! selects the flash alone there, which the older images did not); and with the pad's touch
//! power, touch start and RTC pull-down off. Then it resets into the C++ firmware. It writes
//! nothing but the hand-back's otadata entry, as every image.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::{self, Display, Formatter};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::{GPIO, IO_MUX, RTC_IO, SPI1};
use esp_hal::rtc_cntl::Rtc;
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

/// Right's pin, and the bit of its pad in the RTC IO registers' pad fields (from bit 10).
const PIN: usize = 7;
const RTC_BIT: usize = 10 + PIN;
/// Samples in each phase, 10 ms apart: 4 s.
const SAMPLES: u32 = 400;
const APART_MS: u32 = 10;

/// One look at GPIO7's pad: its IO MUX register, its RTC IO register, what its digital input and
/// its RTC input read, whether a digital or an RTC output is enabled on it, and SPI1's MISC
/// register.
#[derive(Clone, Copy)]
struct Pad {
    mux: u32,
    rtc: u32,
    digital_in: u32,
    rtc_in: u32,
    out: u32,
    rtc_out: u32,
    spi: u32,
}

impl Pad {
    fn read() -> Self {
        let rtc_io = RTC_IO::regs();
        Pad {
            mux: IO_MUX::regs().gpio(PIN).read().bits(),
            rtc: rtc_io.touch_pad(PIN).read().bits(),
            digital_in: GPIO::regs().in_().read().bits() >> PIN & 1,
            rtc_in: rtc_io.rtc_gpio_in().read().bits() >> RTC_BIT & 1,
            out: GPIO::regs().enable().read().bits() >> PIN & 1,
            rtc_out: rtc_io.rtc_gpio_enable().read().bits() >> RTC_BIT & 1,
            spi: SPI1::regs().misc().read().bits(),
        }
    }
}

impl Display for Pad {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let Pad {
            mux,
            rtc,
            digital_in,
            rtc_in,
            out: oe,
            rtc_out,
            spi,
        } = *self;
        write!(
            out,
            "{mux:x} {rtc:x} i{digital_in} r{rtc_in} e{oe}{rtc_out} s{spi:x}"
        )
    }
}

/// Samples Right `SAMPLES` times: how many read low on the digital input, and on the RTC input.
fn sample(right: &Input<'_>, rtc: &mut Rtc<'_>) -> (u32, u32) {
    let delay = Delay::new();
    let (mut digital, mut rtc_low) = (0, 0);
    for _ in 0..SAMPLES {
        digital += u32::from(right.is_low());
        rtc_low += u32::from(Pad::read().rtc_in == 0);
        delay.delay_millis(APART_MS);
        rtc.rwdt.feed();
    }
    (digital, rtc_low)
}

/// The pad's touch power, touch start and RTC pull-down off.
fn touch_off() {
    RTC_IO::regs()
        .touch_pad(PIN)
        .modify(|_, w| w.xpd().clear_bit().start().clear_bit().rde().clear_bit());
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    ));
    let entered = Pad::read();
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite right check 2", board.boot);
    // `note` prints each line it adds; these are `start`'s.
    esp_println::println!("{}", report.as_str().trim_end());
    report.note(format_args!("a {entered}"));
    report.note(format_args!("b {}", Pad::read()));
    let config = InputConfig::default().with_pull(Pull::Up);
    let right = Input::new(board.buttons.right, config);
    report.note(format_args!("c {}", Pad::read()));
    let (d1, r1) = sample(&right, &mut rtc);
    // SAFETY: the value SPI1's MISC register had as the image was entered; only esp-storage's
    // flash calls use SPI1, and none runs meanwhile.
    SPI1::regs()
        .misc()
        .write(|w| unsafe { w.bits(entered.spi) });
    let (d2, r2) = sample(&right, &mut rtc);
    touch_off();
    let (d3, r3) = sample(&right, &mut rtc);
    report.note(format_args!(
        "low {d1}/{r1} {d2}/{r2} {d3}/{r3} of {SAMPLES}"
    ));
    report.note(format_args!("d {}", Pad::read()));
    report.note(format_args!("done"));
    esp_hal::system::software_reset()
}
