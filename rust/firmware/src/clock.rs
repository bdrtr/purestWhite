//! The battery-backed clock (a BM8563 on the shared I2C bus, `purestwhite-rtc`) as the time TLS
//! checks certificates' dates against.

use core::fmt::{self, Display, Formatter};
use embedded_hal::i2c::I2c;
use esp_hal::delay::Delay;
use purestwhite_rtc::{DateTime, begin, epoch_from_utc, now};

/// The BM8563's address on the bus.
pub const ADDR: u8 = 0x51;

/// The clock's time, UTC, as it read.
#[derive(Clone, Copy)]
pub struct Read(pub DateTime);

impl Read {
    /// Seconds since 1970; `None` before 1970 or after 2106.
    #[must_use]
    pub fn unix_seconds(&self) -> Option<u32> {
        u32::try_from(epoch_from_utc(&self.0)).ok()
    }
}

/// `clock: 2026-11-03 06:12 UTC`.
impl Display for Read {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let t = &self.0;
        write!(
            out,
            "clock: {:04}-{:02}-{:02} {:02}:{:02} UTC",
            t.year, t.month, t.day, t.hour, t.minute
        )
    }
}

/// The clock's time; `None` when it does not answer, or says its time is not to be trusted (it
/// lost power).
pub fn read(bus: &mut impl I2c) -> Option<Read> {
    if !begin(bus, &mut Delay::new(), ADDR) {
        return None;
    }
    now(bus, ADDR).map(Read)
}
