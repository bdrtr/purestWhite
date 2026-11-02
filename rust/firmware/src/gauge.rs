//! The battery's fuel gauge as the services' `Gauge`: the CW2017 (`purestwhite-gauge`) on the
//! shared I2C bus (`crate::i2c`), read for the battery measurement (DESIGN.md, gate 1).

use embassy_time::{Delay, Instant};
use embedded_hal_async::i2c::I2c;
use purestwhite_gauge::Cw2017;
use purestwhite_services::{Gauge, Reading};

/// The CW2017's address on the bus.
pub const ADDR: u8 = 0x63;

/// The CW2017 on `B`: a reading is its state of charge and its cell's voltage, each read on its
/// own, as the voltage needs no battery profile.
pub struct DeviceGauge<B> {
    bus: B,
    gauge: Cw2017,
}

impl<B: I2c> DeviceGauge<B> {
    #[must_use]
    pub const fn new(bus: B) -> Self {
        DeviceGauge {
            bus,
            gauge: Cw2017::new(),
        }
    }
}

impl<B: I2c> Gauge for DeviceGauge<B> {
    async fn read(&mut self) -> Reading {
        // A millisecond clock that may wrap, as `Cw2017::soc` takes it.
        #[allow(clippy::cast_possible_truncation, reason = "the clock may wrap")]
        let now = Instant::now().as_millis() as u32;
        let percent = self.gauge.soc(&mut self.bus, &mut Delay, ADDR, now).await;
        let millivolts = Cw2017::millivolts(&mut self.bus, ADDR).await;
        Reading {
            percent,
            millivolts,
        }
    }
}
