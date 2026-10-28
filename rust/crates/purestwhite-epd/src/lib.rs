//! The X4 Pro's e-paper panel: the controller's bus (write-only SPI with chip select, a
//! data/command line, reset and BUSY), the probe that tells which controller drives it ([`probe`]),
//! and two of the controllers it ships with: [`uc8279x4`] and [`uc8179`]. The framing, the reset
//! pulse and the BUSY waits are freeink-sdk's `EpdBus`, which this replaces in the bare-metal
//! firmware. The caller sets up what `EpdBus::begin` did: SPI mode 0, MSB first, at the board's
//! clock (10 MHz on the X4 Pro); chip select and data/command as outputs, high; reset as an output
//! released from any pad hold (esp-hal releases it when it takes the pin); BUSY as an input without
//! a pull.
//!
//! The drivers take any [`Interface`], the bus operations they use; [`Bus`] implements it over
//! embedded-hal.
//!
//! The SPI bus and the delay are embedded-hal-async's: a refresh waits about a second on BUSY,
//! which an executor must be free to spend elsewhere. The pins are embedded-hal's.
//!
//! Nothing here panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

use embedded_hal::digital::{InputPin, OutputPin};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::SpiBus;

pub mod probe;
#[cfg(test)]
mod tests;
pub mod uc8179;
pub mod uc8279x4;

/// A transfer or a pin failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BusError;

/// The panel controller's lines. Chip select and data/command are driven here, not by the SPI
/// peripheral, because one chip-select frame carries a command byte and then its data.
pub struct Bus<S, Cs, Dc, Rst, Busy, D> {
    pub spi: S,
    pub cs: Cs,
    pub dc: Dc,
    pub rst: Rst,
    pub busy: Busy,
    pub delay: D,
}

fn pin<E>(result: Result<(), E>) -> Result<(), BusError> {
    result.map_err(|_| BusError)
}

/// What a controller driver needs of the panel's lines; [`Bus`] is the one the firmware has.
/// The drivers bound on this, not on the bus's six types.
#[allow(
    async_fn_in_trait,
    reason = "the drivers run on a single-threaded executor; no `Send` bound is wanted"
)]
pub trait Interface {
    /// The reset pulse: high, low, high, 10 ms each, then `settle_ms` more.
    ///
    /// # Errors
    /// When the reset line cannot be driven.
    async fn reset(&mut self, settle_ms: u32) -> Result<(), BusError>;

    /// A command byte in its own chip-select frame.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    async fn cmd(&mut self, command: u8) -> Result<(), BusError>;

    /// Data bytes in their own chip-select frame.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    async fn data(&mut self, bytes: &[u8]) -> Result<(), BusError>;

    /// A command and its data bytes, each in its own frame: freeink-sdk's `cmd` and `data` calls
    /// one after another, not its `cmdData` (that is `frame`).
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    async fn cmd_data(&mut self, command: u8, bytes: &[u8]) -> Result<(), BusError> {
        self.cmd(command).await?;
        for &b in bytes {
            self.data(&[b]).await?;
        }
        Ok(())
    }

    /// A command and its data bytes in one chip-select frame (freeink-sdk's `cmdData`).
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    async fn frame(&mut self, command: u8, bytes: &[u8]) -> Result<(), BusError>;

    /// A command, then a burst of data in one chip-select frame, written by `fill` row by row:
    /// `fill(n, row)` fills row `n` (of `rows`) of `row.len()` bytes.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    async fn burst(
        &mut self,
        command: u8,
        row: &mut [u8],
        rows: u16,
        fill: impl FnMut(u16, &mut [u8]),
    ) -> Result<(), BusError>;

    /// The `UC81xx` wait: one tick for the command to take effect, then until BUSY (active low)
    /// reads high. No time limit, as the SDK's: writing while the controller is busy can drop
    /// the write, so a stuck BUSY is the caller's watchdog's to end.
    ///
    /// # Errors
    /// When BUSY cannot be read.
    async fn wait_idle(&mut self) -> Result<(), BusError>;

    /// Up to `max_ms` for BUSY to drop, which shows a waveform started.
    ///
    /// # Errors
    /// When BUSY cannot be read.
    async fn wait_busy_start(&mut self, max_ms: u32) -> Result<(), BusError>;
}

impl<S, Cs, Dc, Rst, Busy, D> Bus<S, Cs, Dc, Rst, Busy, D>
where
    S: SpiBus,
{
    async fn write(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        self.spi.write(bytes).await.map_err(|_| BusError)?;
        // The frame ends only once the bytes are out.
        self.spi.flush().await.map_err(|_| BusError)
    }
}

impl<S, Cs, Dc, Rst, Busy, D> Interface for Bus<S, Cs, Dc, Rst, Busy, D>
where
    S: SpiBus,
    Cs: OutputPin,
    Dc: OutputPin,
    Rst: OutputPin,
    Busy: InputPin,
    D: DelayNs,
{
    async fn reset(&mut self, settle_ms: u32) -> Result<(), BusError> {
        for high in [true, false, true] {
            pin(if high {
                self.rst.set_high()
            } else {
                self.rst.set_low()
            })?;
            self.delay.delay_ms(10).await;
        }
        if settle_ms > 0 {
            self.delay.delay_ms(settle_ms).await;
        }
        Ok(())
    }

    async fn cmd(&mut self, command: u8) -> Result<(), BusError> {
        pin(self.dc.set_low())?;
        pin(self.cs.set_low())?;
        let sent = self.write(&[command]).await;
        pin(self.cs.set_high())?;
        sent
    }

    async fn data(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        pin(self.dc.set_high())?;
        pin(self.cs.set_low())?;
        let sent = self.write(bytes).await;
        pin(self.cs.set_high())?;
        sent
    }

    async fn frame(&mut self, command: u8, bytes: &[u8]) -> Result<(), BusError> {
        pin(self.dc.set_low())?;
        pin(self.cs.set_low())?;
        let mut sent = self.write(&[command]).await;
        if sent.is_ok() && !bytes.is_empty() {
            sent = match self.dc.set_high() {
                Ok(()) => self.write(bytes).await,
                Err(_) => Err(BusError),
            };
        }
        pin(self.cs.set_high())?;
        sent
    }

    async fn burst(
        &mut self,
        command: u8,
        row: &mut [u8],
        rows: u16,
        mut fill: impl FnMut(u16, &mut [u8]),
    ) -> Result<(), BusError> {
        self.cmd(command).await?;
        pin(self.dc.set_high())?;
        pin(self.cs.set_low())?;
        let mut sent = Ok(());
        for n in 0..rows {
            fill(n, row);
            sent = self.spi.write(row).await.map_err(|_| BusError);
            if sent.is_err() {
                break;
            }
        }
        if sent.is_ok() {
            sent = self.spi.flush().await.map_err(|_| BusError);
        }
        pin(self.cs.set_high())?;
        sent
    }

    async fn wait_idle(&mut self) -> Result<(), BusError> {
        self.delay.delay_ms(1).await;
        while self.busy.is_low().map_err(|_| BusError)? {
            self.delay.delay_ms(1).await;
        }
        Ok(())
    }

    async fn wait_busy_start(&mut self, max_ms: u32) -> Result<(), BusError> {
        let mut waited = 0;
        while self.busy.is_high().map_err(|_| BusError)? && waited < max_ms {
            self.delay.delay_ms(1).await;
            waited += 1;
        }
        Ok(())
    }
}
