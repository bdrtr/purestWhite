//! The bare-metal firmware's device layers: the X4 Pro's wiring (`board`), the buttons and touch
//! panel (`input`), deep sleep (`sleep`), and for now the panel (`panel`) and the SD card (`sd`)
//! through blocking calls. For the Embassy main: the buttons on their interrupts (`keys`), the
//! PSRAM as a heap of its own (`memory`) and the panel as the services' `Screen` (`screen`). What
//! every image shares while the C++ firmware is the known-good one (the watchdog, the next boot
//! handed back to it, the report, the bounded deep sleep), and the boot counter's record, is in
//! `boot`; what only the test images use is in `test_image`, out of the real firmware's way.
#![no_std]
#![warn(clippy::pedantic)]

// esp-alloc's global allocator, in every image: `storage` builds on `alloc` (through
// allocator-api2), which needs one even where nothing allocates.
use esp_alloc as _;

pub mod board;
pub mod boot;
pub mod images;
pub mod input;
pub mod keys;
pub mod memory;
pub mod panel;
pub mod releases;
pub mod screen;
pub mod sd;
pub mod sha;
pub mod sleep;
pub mod slots;
pub mod storage;
pub mod test_image;
pub mod updater;

use core::pin::pin;
use core::task::{Context, Poll, Waker};

/// Runs a future whose awaits are all ready at once; `None` if one is not.
pub fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}
