//! The network the C++ firmware saved (`/.crosspoint/wifi.json`, decoded by
//! `purestwhite-wifistore` with the device's key), read from the card (`crate::card`) before the
//! scheduler, and the device's key for the C++ firmware's stored secrets.

use crate::board::CardParts;
use crate::boot::Report;
use crate::card::{FileError, read_files};
use core::fmt::Write as _;
use purestwhite_wifistore::{JSON_MOST, KEY_LEN, Network, ReadError, Saved};

/// The C++ firmware's saved networks.
pub const SAVED: &str = "/.crosspoint/wifi.json";

/// The device's key for the saved passwords: its eFuse base (factory) MAC address, as the C++
/// firmware's `esp_efuse_mac_get_default` reads it.
#[must_use]
pub fn device_key() -> [u8; KEY_LEN] {
    let mut key = [0; KEY_LEN];
    let mac = esp_hal::efuse::base_mac_address();
    for (to, from) in key.iter_mut().zip(mac.as_bytes()) {
        *to = *from;
    }
    key
}

/// The network the C++ firmware joined last (or its first saved one), read from the card;
/// `saved: N networks`, or why there is none, in `report`. `feed` runs before each try at the card
/// (a watchdog).
pub fn saved_network(report: &mut Report, card: CardParts, feed: impl FnMut()) -> Option<Network> {
    let Ok([file]) = read_files(card, [(SAVED, JSON_MOST)], feed) else {
        note_saved(report, Err("no card"));
        return None;
    };
    note_network(report, file.as_deref().map_err(|e| *e))
}

/// The network to join from `file` (the saved networks' bytes), and `saved: N networks` or why
/// there is none in `report`.
pub fn note_network(report: &mut Report, file: Result<&[u8], FileError>) -> Option<Network> {
    let saved = file.map_err(FileError::note).and_then(networks);
    note_saved(report, saved.as_ref().map(Saved::len).map_err(|why| *why));
    saved.ok()?.first_choice().copied()
}

/// `saved: N networks`, or why there are none, in `report`.
pub fn note_saved(report: &mut Report, saved: Result<usize, &'static str>) {
    let _ = match saved {
        Ok(count) => writeln!(report, "saved: {count} networks"),
        Err(why) => writeln!(report, "saved: {why}"),
    };
    report.save();
}

/// The saved networks in `bytes`, or why they could not be had.
fn networks(bytes: &[u8]) -> Result<Saved, &'static str> {
    purestwhite_wifistore::read(bytes, &device_key()).map_err(|why| match why {
        ReadError::Json => "broken file",
        ReadError::TooLarge => "file too large",
        ReadError::Memory => "no memory",
    })
}
