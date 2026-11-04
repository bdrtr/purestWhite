//! The battery-backed clock (a BM8563 on the shared I2C bus, `purestwhite-rtc`) as the time TLS
//! checks certificates' dates against.

use core::fmt::{self, Display, Formatter};
use embedded_hal::i2c::I2c;
use esp_hal::delay::Delay;
use purestwhite_rtc::{DateTime, Fault, begin, epoch_from_utc, read_time};

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

/// `2026-11-03 06:12 UTC`.
fn write_time(out: &mut Formatter<'_>, t: &DateTime) -> fmt::Result {
    write!(
        out,
        "{:04}-{:02}-{:02} {:02}:{:02} UTC",
        t.year, t.month, t.day, t.hour, t.minute
    )
}

/// `clock: 2026-11-03 06:12 UTC`.
impl Display for Read {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        write!(out, "clock: ")?;
        write_time(out, &self.0)
    }
}

/// Why the clock's time could not be had.
#[derive(Clone, Copy, Debug)]
pub enum Unread {
    /// It did not answer on the bus.
    NoAnswer,
    /// It answered, then the read failed.
    ReadFailed,
    /// Its oscillator stopped at some point (never set, or it lost power); the time it holds.
    Stopped(DateTime),
}

/// `clock: no answer`, `clock: read failed`, or `clock: stopped, holds 2026-11-03 06:12 UTC`.
impl Display for Unread {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Unread::NoAnswer => write!(out, "clock: no answer"),
            Unread::ReadFailed => write!(out, "clock: read failed"),
            Unread::Stopped(t) => {
                write!(out, "clock: stopped, holds ")?;
                write_time(out, t)
            }
        }
    }
}

/// The clock's time, or why it could not be had: it does not answer, or says its time is not to
/// be trusted (it lost power).
///
/// # Errors
/// [`Unread`].
pub fn read(bus: &mut impl I2c) -> Result<Read, Unread> {
    if !begin(bus, &mut Delay::new(), ADDR) {
        return Err(Unread::NoAnswer);
    }
    read_time(bus, ADDR).map(Read).map_err(|fault| match fault {
        Fault::Bus => Unread::ReadFailed,
        Fault::Stopped(time) => Unread::Stopped(time),
    })
}
