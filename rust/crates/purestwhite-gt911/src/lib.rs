//! The X4 Pro's touch controller: a Goodix GT911 on the caller's I2C bus, shared with the
//! real-time clock and the fuel gauge, with a capacitive Home key. The reset and address-select
//! sequence, the registers and the point records are freeink-sdk's `InputManager` GT911 path,
//! which this replaces in the bare-metal firmware. What the frames mean (presses, taps, gestures,
//! the Home key's long press) stays the caller's.
//!
//! The bus and the delay are embedded-hal-async's: the reset sequence waits 120 ms. RST and INT
//! go through [`ResetPins`], because the sequence drives INT to select the address and then makes
//! it an input again, which no embedded-hal trait covers. A future dropped part way leaves what
//! it did: `begin` dropped during a reset can leave INT driven, and `read` dropped before its
//! clear gives the same frame again.
//!
//! Nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::i2c::I2c;

#[cfg(test)]
mod tests;

/// Bit 7: a new frame; bit 4: the Home key is down; bits 0-3: the contacts. Written 0 to let the
/// controller fill the next frame.
const STATUS: [u8; 2] = [0x81, 0x4E];
/// The first contact's record; the others follow, 8 bytes each.
const POINTS: [u8; 2] = [0x81, 0x50];
const BUFFER_READY: u8 = 0x80;
const HOME_KEY: u8 = 0x10;
const CONTACTS: u8 = 0x0F;
const RECORD: usize = 8;

/// The contacts a frame keeps; the controller may report more.
pub const MAX_CONTACTS: usize = 4;

/// The controller's RST and INT lines, both wired.
pub trait ResetPins {
    /// Makes RST an output at `high`; low holds the controller in reset.
    fn set_reset(&mut self, high: bool);
    /// Makes INT an output at `high`, setting the level before it starts driving.
    fn drive_int(&mut self, high: bool);
    /// Makes INT an input again, without a pull.
    fn release_int(&mut self);
}

/// For boards without both RST and INT wired, as the SDK skips the reset then:
/// `None::<&mut NoPins>`.
pub struct NoPins;

impl ResetPins for NoPins {
    fn set_reset(&mut self, _high: bool) {}
    fn drive_int(&mut self, _high: bool) {}
    fn release_int(&mut self) {}
}

/// How the board wires and mounts the controller (freeink-sdk's `TouchConfig`).
#[allow(
    clippy::struct_excessive_bools,
    reason = "the SDK's board profile fields"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Config {
    /// The address tried first, then the alternate one; 0 is not tried.
    pub addr: u8,
    pub alt_addr: u8,
    /// The raw range of each panel axis, after `swap_xy`.
    pub raw_min_x: u16,
    pub raw_max_x: u16,
    pub raw_min_y: u16,
    pub raw_max_y: u16,
    /// The digitizer's axes are the panel's swapped.
    pub swap_xy: bool,
    pub flip_x: bool,
    pub flip_y: bool,
    /// The records hold coordinates from byte 0 and no contact id.
    pub coords_at_byte0: bool,
}

impl Config {
    /// Whether a contact keeps its id from frame to frame.
    #[must_use]
    pub fn ids_stable(&self) -> bool {
        !self.coords_at_byte0
    }
}

/// A contact in panel coordinates: 0 to `raw_max - raw_min` on each axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Contact {
    pub id: u8,
    pub x: u16,
    pub y: u16,
}

/// One frame of the controller.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub home_key: bool,
    /// The contacts the controller reports, which can be more than `count`.
    pub reported: u8,
    /// The contacts in `contacts`.
    pub count: u8,
    pub contacts: [Contact; MAX_CONTACTS],
}

/// What a poll found. Every kind but `Failed` read the status: the SDK times the Home key's long
/// press then, before it looks at the frame, so a held key times out without new frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    /// The status could not be read, or `addr` is 0: keep what is known.
    Failed,
    /// No new frame since the last.
    NotReady,
    /// A new frame; one without contacts means every contact lifted.
    Frame(Frame),
    /// A new frame whose contacts could not be read: keep the contacts known. The Home key's
    /// state is new.
    ContactsFailed { home_key: bool },
}

/// The reset sequence: the level of INT as RST rises selects the controller's address.
async fn reset(pins: &mut impl ResetPins, delay: &mut impl DelayNs, int_high: bool) {
    pins.set_reset(false);
    pins.drive_int(int_high);
    delay.delay_ms(10).await;
    pins.set_reset(true);
    delay.delay_ms(10).await;
    pins.drive_int(int_high);
    delay.delay_ms(50).await;
    pins.release_int();
    delay.delay_ms(50).await;
}

/// The configured address that answers, if any.
async fn probe(bus: &mut impl I2c, config: &Config) -> Option<u8> {
    for addr in [config.addr, config.alt_addr] {
        if addr != 0 && bus.write(addr, &[]).await.is_ok() {
            return Some(addr);
        }
    }
    None
}

/// Resets the controller with INT low, then high, until one of its addresses answers; the
/// address, or `None` when the controller is absent. The board's touch supply must be on.
pub async fn begin(
    bus: &mut impl I2c,
    delay: &mut impl DelayNs,
    mut pins: Option<&mut impl ResetPins>,
    config: &Config,
) -> Option<u8> {
    for int_high in [false, true] {
        if let Some(pins) = pins.as_deref_mut() {
            reset(pins, delay, int_high).await;
        }
        if let Some(addr) = probe(bus, config).await {
            return Some(addr);
        }
    }
    None
}

/// `raw` on an axis of `raw_min` to `raw_max`, scaled to 0 to `out_max`.
#[allow(
    clippy::cast_possible_truncation,
    reason = "below out_max when raw is inside the range"
)]
fn map_axis(raw: u16, raw_min: u16, raw_max: u16, out_max: u16) -> u16 {
    if raw <= raw_min {
        return 0;
    }
    if raw >= raw_max {
        return out_max;
    }
    (u32::from(raw - raw_min) * u32::from(out_max) / u32::from(raw_max - raw_min)) as u16
}

fn contact(index: u8, record: [u8; RECORD], config: &Config) -> Contact {
    let [b0, b1, b2, b3, b4, ..] = record;
    let (id, [x0, x1, y0, y1]) = if config.coords_at_byte0 {
        (index, [b0, b1, b2, b3])
    } else {
        (b0 & 0x0F, [b1, b2, b3, b4])
    };
    let (raw_x, raw_y) = (u16::from_le_bytes([x0, x1]), u16::from_le_bytes([y0, y1]));
    let (x, y) = if config.swap_xy {
        (raw_y, raw_x)
    } else {
        (raw_x, raw_y)
    };
    // The spans wrap as the SDK's 16-bit arithmetic does.
    let span_x = config.raw_max_x.wrapping_sub(config.raw_min_x);
    let span_y = config.raw_max_y.wrapping_sub(config.raw_min_y);
    let mut x = map_axis(x, config.raw_min_x, config.raw_max_x, span_x);
    let mut y = map_axis(y, config.raw_min_y, config.raw_max_y, span_y);
    if config.flip_x {
        x = span_x.wrapping_sub(x);
    }
    if config.flip_y {
        y = span_y.wrapping_sub(y);
    }
    Contact { id, x, y }
}

/// Reads the frame at `addr`, and lets the controller fill the next when there was a new one.
/// `addr` is what `begin` found; 0, no controller, reads nothing.
pub async fn read(bus: &mut impl I2c, addr: u8, config: &Config) -> Reading {
    if addr == 0 {
        return Reading::Failed;
    }
    let mut status = [0];
    if bus.write_read(addr, &STATUS, &mut status).await.is_err() {
        return Reading::Failed;
    }
    let [status] = status;
    if status & BUFFER_READY == 0 {
        return Reading::NotReady;
    }
    let home_key = status & HOME_KEY != 0;
    let reported = status & CONTACTS;
    let mut frame = Frame {
        home_key,
        reported,
        ..Frame::default()
    };
    let reading = if reported == 0 {
        Reading::Frame(frame)
    } else {
        // All the records in one transfer, so they come from one frame.
        let count = usize::from(reported).min(MAX_CONTACTS);
        let mut records = [0u8; MAX_CONTACTS * RECORD];
        let records = records.get_mut(..count * RECORD).unwrap_or_default();
        if bus.write_read(addr, &POINTS, records).await.is_ok() {
            for ((index, record), slot) in (0u8..)
                .zip(records.as_chunks::<RECORD>().0)
                .zip(&mut frame.contacts)
            {
                *slot = contact(index, *record, config);
                frame.count = index + 1;
            }
            Reading::Frame(frame)
        } else {
            Reading::ContactsFailed { home_key }
        }
    };
    // Best effort, as the SDK: a failed clear shows as the same frame again.
    let _ = bus.write(addr, &[STATUS[0], STATUS[1], 0]).await;
    reading
}
