//! The X4 Pro's and X4 Classic's real-time clock: a BM8563, register-compatible with the NXP
//! PCF8563, on the I2C bus it shares with the touch controller and the fuel gauge. It keeps UTC
//! in BCD. The bus and the delay are embedded-hal's, the caller's: other drivers share the bus.
//!
//! Nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c;

#[cfg(test)]
mod tests;

/// Control/Status 1, read to find the chip.
const CONTROL_STATUS1: u8 = 0x00;
/// Seconds, minutes, hours, days, weekdays, months (with the century bit) and years.
const TIME: u8 = 0x02;
const CLKOUT: u8 = 0x0D;
/// The 32 kHz CLKOUT output off: nothing uses it.
const CLKOUT_DISABLED: u8 = 0x00;
/// In the seconds register: the oscillator stopped (low voltage, or never set).
const VOLTAGE_LOW: u8 = 0x80;
/// In the months register: the year is 19xx.
const CENTURY: u8 = 0x80;

/// Tries at finding the chip: on a shared bus, the first transfer after another device's NACK
/// can fail.
const PROBE_TRIES: usize = 3;
const PROBE_PAUSE_MS: u32 = 10;

/// A wall-clock time, in UTC. `year` is the full year; `weekday` is 0 for Sunday.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub weekday: u8,
}

fn from_bcd(v: u8) -> u8 {
    (v >> 4) * 10 + (v & 0x0F)
}

#[allow(clippy::cast_possible_truncation)]
fn to_bcd(v: u8) -> u8 {
    // Out-of-range values wrap as the SDK driver's decToBcd did.
    (u32::from(v / 10) << 4 | u32::from(v % 10)) as u8
}

/// Whether the clock at `addr` answers; turns its CLKOUT output off.
pub fn begin(bus: &mut impl I2c, delay: &mut impl DelayNs, addr: u8) -> bool {
    for _ in 0..PROBE_TRIES {
        let mut status = [0];
        if bus
            .write_read(addr, &[CONTROL_STATUS1], &mut status)
            .is_ok()
        {
            // Best effort: the clock works with CLKOUT on.
            let _ = bus.write(addr, &[CLKOUT, CLKOUT_DISABLED]);
            return true;
        }
        delay.delay_ms(PROBE_PAUSE_MS);
    }
    false
}

/// The time; `None` when the read fails or the oscillator stopped (the time is then unreliable).
pub fn now(bus: &mut impl I2c, addr: u8) -> Option<DateTime> {
    let mut raw = [0; 7];
    bus.write_read(addr, &[TIME], &mut raw).ok()?;
    let [second, minute, hour, day, weekday, month, year] = raw;
    if second & VOLTAGE_LOW != 0 {
        return None;
    }
    let century = if month & CENTURY != 0 { 1900 } else { 2000 };
    Some(DateTime {
        year: century + u16::from(from_bcd(year)),
        month: from_bcd(month & 0x1F),
        day: from_bcd(day & 0x3F),
        hour: from_bcd(hour & 0x3F),
        minute: from_bcd(minute & 0x7F),
        second: from_bcd(second & 0x7F),
        weekday: from_bcd(weekday & 0x07),
    })
}

/// Sets the time, which also clears the voltage-low flag; false when the write fails.
#[allow(clippy::cast_possible_truncation)]
pub fn set(bus: &mut impl I2c, addr: u8, time: &DateTime) -> bool {
    let century = if time.year < 2000 { CENTURY } else { 0 };
    bus.write(
        addr,
        &[
            TIME,
            to_bcd(time.second),
            to_bcd(time.minute),
            to_bcd(time.hour),
            to_bcd(time.day),
            to_bcd(time.weekday),
            to_bcd(time.month) | century,
            to_bcd((time.year % 100) as u8),
        ],
    )
    .is_ok()
}

/// The Unix time of a UTC calendar time (Howard Hinnant's days from civil). Fields out of range
/// give what the firmware's 32-bit arithmetic gives.
#[must_use]
#[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
pub fn epoch_from_utc(time: &DateTime) -> i64 {
    let month = i32::from(time.month);
    let year = i32::from(time.year) - i32::from(month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = (year - era * 400) as u32;
    let shifted = (month + if month > 2 { -3 } else { 9 }) as u32;
    let day_of_year = ((153 * shifted + 2) / 5)
        .wrapping_add(u32::from(time.day))
        .wrapping_sub(1);
    let day_of_era =
        (year_of_era * 365 + year_of_era / 4 - year_of_era / 100).wrapping_add(day_of_year);
    let days = era
        .wrapping_mul(146_097)
        .wrapping_add(day_of_era as i32)
        .wrapping_sub(719_468);
    i64::from(days) * 86_400
        + i64::from(time.hour) * 3_600
        + i64::from(time.minute) * 60
        + i64::from(time.second)
}
