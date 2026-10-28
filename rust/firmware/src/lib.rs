//! The bare-metal firmware's device layers: the buttons and touch panel (`input`), deep sleep
//! (`sleep`), the PSRAM and the power latch, and for now the panel (`panel`) and the SD card
//! (`sd`) through blocking calls. What only the test images for the second OTA slot use (handing
//! the next boot back to the C++ firmware, their report, their watchdog) is in `test_image`, out
//! of the real firmware's way.
#![no_std]
#![warn(clippy::pedantic)]

pub mod input;
pub mod panel;
pub mod sd;
pub mod sleep;
pub mod test_image;

use core::pin::pin;
use core::task::{Context, Poll, Waker};

use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::GPIO1;
use esp_hal::psram::{FlashFreq, PsramConfig, PsramMode, PsramSize, SpiRamFreq};

/// Runs a future whose awaits are all ready at once; `None` if one is not.
pub fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

/// The PSRAM's clock, in MHz: 40, not the C++ firmware's 80. ESP-IDF tunes the sampling timing
/// at every boot for octal PSRAM above 40 MHz, and esp-hal 1.2.2 does not yet.
pub const PSRAM_MHZ: u32 = 40;

/// The X4 Pro's PSRAM: 8 MB octal, beside flash at 80 MHz, as the C++ firmware has it (the Arduino
/// core's `dio_opi` variant), at `PSRAM_MHZ`; its size is read from the chip.
#[must_use]
pub fn psram_config() -> PsramConfig {
    PsramConfig {
        mode: PsramMode::OctalSpi,
        size: PsramSize::AutoDetect,
        core_clock: None,
        flash_frequency: FlashFreq::FlashFreq80m,
        ram_frequency: SpiRamFreq::Freq40m,
    }
}

/// Raises the X4 Pro's power latch (GPIO1) and holds its pad: on battery the device turns off as
/// soon as the latch floats, and a system reset releases every pin it does not hold. The C++
/// firmware releases the hold at boot (`BoardConfig::holdPowerRails`).
#[must_use = "keep the pin for the rest of the run"]
pub fn hold_power_latch(pin: GPIO1<'_>) -> Output<'_> {
    let mut latch = Output::new(pin, Level::High, OutputConfig::default());
    latch.set_pad_hold(true);
    latch
}
