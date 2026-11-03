//! The network the C++ firmware saved, read from the card (`/.crosspoint/wifi.json`, decoded by
//! `purestwhite-wifistore` with the device's key) over the blocking bus, before the scheduler.

use crate::board::CardParts;
use crate::boot::Report;
use crate::ready;
use crate::sd;
use crate::storage::load_whole;
use allocator_api2::alloc::Global;
use core::fmt::Write as _;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{GPIO40, GPIO41, GPIO42, SDHOST};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};
use purestwhite_wifistore::{KEY_LEN, Network};

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

/// The network the C++ firmware joined last (or its first saved one), the card off again after;
/// `saved: N networks`, or why there is none (`saved: no card`, `no file`, `broken file`), in
/// `report`. `feed` runs before each try at the card (a watchdog).
pub fn saved_network(report: &mut Report, card: CardParts, feed: impl FnMut()) -> Option<Network> {
    let mut supply = Output::new(card.supply, Level::High, OutputConfig::default());
    let pins = (card.clk, card.cmd, card.dat0);
    let saved = read_saved(&mut supply, card.sdhost, pins, feed);
    supply.set_high();
    let _ = match &saved {
        Ok(saved) => writeln!(report, "saved: {} networks", saved.len()),
        Err(why) => writeln!(report, "saved: {why}"),
    };
    report.save();
    saved.ok()?.first_choice().copied()
}

/// The saved networks, or why they could not be had.
fn read_saved(
    supply: &mut Output<'_>,
    sdhost: SDHOST<'static>,
    pins: (GPIO41<'static>, GPIO42<'static>, GPIO40<'static>),
    mut feed: impl FnMut(),
) -> Result<purestwhite_wifistore::Saved, &'static str> {
    let controller = SdHostController::new(sdhost, SdConfig::default()).map_err(|_| "no card")?;
    let mut volume =
        sd::open_volume(&controller, pins, supply, |_| feed()).map_err(|_| "no card")?;
    let bytes = ready(load_whole(&mut volume, SAVED, Global))
        .and_then(Result::ok)
        .ok_or("no file")?;
    purestwhite_wifistore::read(&bytes, &device_key()).map_err(|why| match why {
        purestwhite_wifistore::ReadError::Json => "broken file",
        purestwhite_wifistore::ReadError::TooLarge => "file too large",
        purestwhite_wifistore::ReadError::Memory => "no memory",
    })
}
