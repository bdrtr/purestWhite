//! The I2C bus the touch panel, the clock and the fuel gauge share (I2C0, SDA 39, SCL 38), as the
//! C++ firmware drives it: at 400 kHz, and only once the touch panel's supply (GPIO2, active low)
//! has been on for `SUPPLY_MS`. The caller turns the supply on, waits, and keeps it on while the
//! bus is used.

use esp_hal::Async;
use esp_hal::i2c::master::{Config, I2c, SoftwareTimeout};
use esp_hal::peripherals::{GPIO38, GPIO39, I2C0};
use esp_hal::time::{Duration, Rate};

/// How long the touch panel's supply is on before the bus is driven.
pub const SUPPLY_MS: u64 = 50;

/// The shared bus over the interrupt-driven driver; a stuck bus fails a transfer after 50 ms
/// instead of waiting for the watchdog. `None` when it cannot be set up.
#[must_use]
pub fn open_bus(
    i2c: I2C0<'static>,
    sda: GPIO39<'static>,
    scl: GPIO38<'static>,
) -> Option<I2c<'static, Async>> {
    let config = Config::default()
        .with_frequency(Rate::from_khz(400))
        .with_software_timeout(SoftwareTimeout::Transaction(Duration::from_millis(50)));
    I2c::new(i2c, config)
        .map(|i2c| i2c.with_sda(sda).with_scl(scl).into_async())
        .ok()
}
