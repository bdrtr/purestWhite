//! What the test images that use TLS share: the clock read and given to TLS (the time it checks
//! certificates' dates against), and random bytes for it.

use crate::board::I2cParts;
use crate::boot::Report;
use crate::{clock, i2c};
use core::fmt::Write as _;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::rng::Rng;

/// Random bytes for TLS from the chip's generator, true random while the radio is on.
pub fn random(bytes: &mut [u8]) {
    Rng::new().read(bytes);
}

/// The clock's line (`clock: ... UTC`, or `clock: not set`), and its time given to TLS; the touch
/// panel's supply, on for the bus, kept.
pub fn note_clock(report: &mut Report, parts: I2cParts) -> Output<'static> {
    let supply = Output::new(parts.supply, Level::Low, OutputConfig::default());
    esp_hal::delay::Delay::new().delay_millis(u32::try_from(i2c::SUPPLY_MS).unwrap_or(50));
    let read = i2c::open_bus(parts.i2c, parts.sda, parts.scl)
        .as_mut()
        .and_then(clock::read);
    let _ = match &read {
        Some(read) => writeln!(report, "{read}"),
        None => writeln!(report, "clock: not set"),
    };
    if let Some(seconds) = read.and_then(|r| r.unix_seconds()) {
        purestwhite_tls::set_time(seconds);
    }
    report.save();
    supply
}
