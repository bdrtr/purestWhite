//! The touch panel for images without an executor.

use crate::board::{I2cParts, TouchParts};
use crate::boot::Report;
use crate::input::{ReadyI2c, TOUCH, TouchPins};
use core::fmt::Write as _;
use esp_hal::Blocking;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Flex, Level, Output, OutputConfig};
use esp_hal::i2c::master::{Config as I2cConfig, I2c as I2cBus, SoftwareTimeout};
use esp_hal::time::{Duration, Rate};

/// The touch panel's bus through blocking calls.
pub type BlockingTouchBus<'d> = ReadyI2c<I2cBus<'d, Blocking>>;

/// The touch panel as `open_touch` leaves it.
pub struct TouchStart<'d> {
    /// The bus and the controller's address, when it answered.
    pub found: Option<(BlockingTouchBus<'d>, u8)>,
    /// Its reset and interrupt pins: keep them for the rest of the run.
    pub pins: TouchPins<'d>,
    /// Its supply (GPIO2), on: keep it for the rest of the run.
    pub supply: Output<'d>,
}

/// The touch panel through blocking calls, for images without an executor: its supply (GPIO2,
/// active low) on for 50 ms before the shared bus is driven, as the C++ firmware does; I2C0 (SDA
/// GPIO39, SCL GPIO38) at 400 kHz, a transfer failing after 50 ms rather than hanging; and the
/// controller reset and found as the SDK does (`purestwhite_gt911::begin`, RST GPIO4, INT GPIO10).
#[must_use = "keep the pins and the supply for the rest of the run"]
pub fn open_touch(touch: TouchParts, bus: I2cParts) -> TouchStart<'static> {
    let TouchParts { reset, int } = touch;
    let I2cParts {
        i2c,
        sda,
        scl,
        supply,
    } = bus;
    let supply = Output::new(supply, Level::Low, OutputConfig::default());
    Delay::new().delay_millis(50);
    let mut pins = TouchPins {
        // In reset until the address is selected, as the SDK's pinMode(OUTPUT) leaves it.
        reset: Output::new(reset, Level::Low, OutputConfig::default()),
        int: Flex::new(int),
    };
    pins.int.apply_output_config(&OutputConfig::default());
    let bus = I2cBus::new(
        i2c,
        I2cConfig::default()
            .with_frequency(Rate::from_khz(400))
            .with_software_timeout(SoftwareTimeout::Transaction(Duration::from_millis(50))),
    )
    .ok()
    .map(|i2c| ReadyI2c(i2c.with_sda(sda).with_scl(scl)));
    let found = bus.and_then(|mut bus| {
        crate::ready(purestwhite_gt911::begin(
            &mut bus,
            &mut crate::sd::SpinDelay(Delay::new()),
            Some(&mut pins),
            &TOUCH,
        ))
        .flatten()
        .map(|addr| (bus, addr))
    });
    TouchStart {
        found,
        pins,
        supply,
    }
}

/// The report's line on the touch controller: its address, or that none answered.
pub fn note_touch(report: &mut Report, addr: Option<u8>) {
    let _ = match addr {
        Some(addr) => writeln!(report, "touch: {addr:#04x}"),
        None => writeln!(report, "touch: none"),
    };
    report.save();
}
