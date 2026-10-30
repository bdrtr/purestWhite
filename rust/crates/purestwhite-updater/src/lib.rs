//! The SD updater of the bare-metal firmware (rust/firmware/DESIGN.md, the SD updater): the C++
//! firmware's release on the card (`releases::RECOVERY_FILE`) written into the C++ firmware's
//! slot, within phase 2's limits (`purestwhite_ota::plan`). It identifies the slot and the card's
//! release (the first pass, nothing written), writes the release sector by sector, each read back
//! (the second pass, whose digest must be the first's), and identifies the slot once more
//! ([`Updater::update`]). Whether the firmware then resets into the slot or sleeps is
//! [`Done::resets`].
//!
//! Nothing here touches the chip: the slot is an `embedded-storage` NOR flash, the card a
//! `purestwhite-fat` volume opened through [`Card`], the SHA-256 unit a [`Hashes`], and the lines
//! go to [`Notes`] (the device's report). The firmware runs it on the X4 Pro's parts, and the
//! tests over a FAT image and a flash in memory, the scenarios of the recovery mode included.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod images;
pub mod releases;
mod run;
#[cfg(test)]
mod tests;

use core::fmt::{Arguments, Debug};
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use purestwhite_fat::{BlockDevice, Volume};
use purestwhite_ota::digest::Sha256;
pub use run::{Done, Updater};

/// Where the updater's lines go, one at a time: the device's report.
pub trait Notes {
    fn note(&mut self, line: Arguments<'_>);
}

/// The chip's SHA-256 unit: one digest at a time, each borrowing the unit until it is done.
pub trait Hashes {
    type Sha<'s>: Sha256
    where
        Self: 's;

    /// A new digest.
    fn start(&mut self) -> Self::Sha<'_>;
}

/// The card the release is read from, opened once the slot is identified. Its reads must finish
/// when first polled (a blocking bus): the updater polls each step once, and a step whose read
/// waits ends there as `not ready`, a write cut short.
pub trait Card {
    type Blocks<'c>: BlockDevice
    where
        Self: 'c;
    type Error: Debug;

    /// The card's FAT volume. `feed` feeds the watchdog while a card that stalls is tried again.
    ///
    /// # Errors
    /// The card or its volume cannot be had.
    fn open(&mut self, feed: &mut dyn FnMut()) -> Result<Volume<Self::Blocks<'_>>, Self::Error>;
}

/// Runs a future whose awaits are all ready at once (the card over a blocking bus); `None` if one
/// is not.
fn ready<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}
