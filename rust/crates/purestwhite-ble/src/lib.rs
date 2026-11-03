//! The BLE page turner's logic, apart from the radio (gate 3, rust/firmware/ROADMAP.md): which
//! advertisers are HID remotes (`advert`), a scan's tally of them (`Seen`), a remote's reports
//! read as key presses and button edges (`decoder`, with `keymap`), the remotes known by name
//! (`profiles`) and what each button does (`binding`). It follows the C++ firmware: its SDK's
//! `BleKeyboardHost` (an advertiser is a remote when it lists the HID service, 0x1812, or the
//! keyboard appearance, 0x03C1) and the app's binding rules. Nothing here allocates or panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

pub mod advert;
pub mod binding;
pub mod decoder;
pub mod keymap;
pub mod profiles;
mod seen;
#[cfg(test)]
mod tests;

pub use advert::Advert;
pub use seen::{SEEN_MOST, Seen};
