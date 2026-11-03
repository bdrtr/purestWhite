//! The BLE page turner's logic, apart from the radio (gate 3, rust/firmware/ROADMAP.md): which
//! advertisers are HID remotes (`advert`), and a scan's tally of them (`Seen`). It follows the C++
//! firmware's SDK (`BleKeyboardHost`): an advertiser is a remote when it lists the HID service
//! (0x1812) or the keyboard appearance (0x03C1). Nothing here allocates or panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod advert;
mod seen;
#[cfg(test)]
mod tests;

pub use advert::Advert;
pub use seen::{SEEN_MOST, Seen};
