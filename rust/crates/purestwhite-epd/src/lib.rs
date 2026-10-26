//! The X4 Pro's e-paper panel: the controller's bus (write-only SPI with chip select, a
//! data/command line, reset and BUSY) and, in [`uc8279x4`], the UC8279 controller the panel ships
//! with. The framing, the reset pulse and the BUSY waits are freeink-sdk's `EpdBus`, which this
//! replaces in the bare-metal firmware. The caller sets up what `EpdBus::begin` did: SPI mode 0,
//! MSB first, at the board's clock (10 MHz on the X4 Pro); chip select and data/command as
//! outputs, high; reset as an output released from any pad hold (esp-hal releases it when it
//! takes the pin); BUSY as an input without a pull.
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

impl<S, Cs, Dc, Rst, Busy, D> Bus<S, Cs, Dc, Rst, Busy, D>
where
    S: SpiBus,
    Cs: OutputPin,
    Dc: OutputPin,
    Rst: OutputPin,
    Busy: InputPin,
    D: DelayNs,
{
    /// The reset pulse: high, low, high, 10 ms each, then `settle_ms` more.
    ///
    /// # Errors
    /// When the reset line cannot be driven.
    pub async fn reset(&mut self, settle_ms: u32) -> Result<(), BusError> {
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

    async fn write(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        self.spi.write(bytes).await.map_err(|_| BusError)?;
        // The frame ends only once the bytes are out.
        self.spi.flush().await.map_err(|_| BusError)
    }

    /// A command byte in its own chip-select frame.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn cmd(&mut self, command: u8) -> Result<(), BusError> {
        pin(self.dc.set_low())?;
        pin(self.cs.set_low())?;
        let sent = self.write(&[command]).await;
        pin(self.cs.set_high())?;
        sent
    }

    /// Data bytes in their own chip-select frame.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn data(&mut self, bytes: &[u8]) -> Result<(), BusError> {
        pin(self.dc.set_high())?;
        pin(self.cs.set_low())?;
        let sent = self.write(bytes).await;
        pin(self.cs.set_high())?;
        sent
    }

    /// A command and its data bytes, each in its own frame, as freeink-sdk's drivers send them.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn cmd_data(&mut self, command: u8, bytes: &[u8]) -> Result<(), BusError> {
        self.cmd(command).await?;
        for &b in bytes {
            self.data(&[b]).await?;
        }
        Ok(())
    }

    /// A command, then a burst of data in one chip-select frame, written by `fill` row by row:
    /// `fill(n, row)` fills row `n` (of `rows`) of `row.len()` bytes.
    ///
    /// # Errors
    /// When a transfer or a pin fails.
    pub async fn burst(
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

    /// The `UC81xx` wait: one tick for the command to take effect, then until BUSY (active low)
    /// reads high. No time limit, as the SDK's: writing while the controller is busy can drop
    /// the write, so a stuck BUSY is the caller's watchdog's to end.
    ///
    /// # Errors
    /// When BUSY cannot be read.
    pub async fn wait_idle(&mut self) -> Result<(), BusError> {
        self.delay.delay_ms(1).await;
        while self.busy.is_low().map_err(|_| BusError)? {
            self.delay.delay_ms(1).await;
        }
        Ok(())
    }

    /// Up to `max_ms` for BUSY to drop, which shows a waveform started.
    ///
    /// # Errors
    /// When BUSY cannot be read.
    pub async fn wait_busy_start(&mut self, max_ms: u32) -> Result<(), BusError> {
        let mut waited = 0;
        while self.busy.is_high().map_err(|_| BusError)? && waited < max_ms {
            self.delay.delay_ms(1).await;
            waited += 1;
        }
        Ok(())
    }
}
