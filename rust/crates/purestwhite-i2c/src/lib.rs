//! The I2C bus the purestwhite drivers talk to. The bus is the caller's: on the X4 Pro the
//! real-time clock, the touch controller and the fuel gauge share one, and the SDK's touch and
//! gauge drivers use it through the same Arduino `TwoWire`.
#![no_std]
#![warn(clippy::pedantic)]
#![forbid(unsafe_code)]

/// An I2C bus.
pub trait Bus {
    /// Writes `bytes` to the device at `addr` in one transfer; false when it fails.
    fn write(&mut self, addr: u8, bytes: &[u8]) -> bool;
    /// Writes `out` (a register address, of any width; 1 to 255 bytes) and, after a repeated
    /// start, reads `buf.len()` bytes (up to 255); false when either part fails.
    fn write_read(&mut self, addr: u8, out: &[u8], buf: &mut [u8]) -> bool;
    fn pause_ms(&mut self, ms: u32);
}
