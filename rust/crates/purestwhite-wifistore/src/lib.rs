//! The Wi-Fi networks the C++ firmware saved on the card (`/.crosspoint/wifi.json`,
//! `WifiCredentialStore`), read for the bare-metal firmware: each network's name and password,
//! and the one it joined last. The C++ firmware keeps each password masked (XOR) with the device's
//! factory MAC address (its eFuse base MAC, 6 bytes) and base64-encoded (`password_obf`), with its
//! length (`password_len`) and CRC-32 (`password_crc32`); older files hold it plain (`password`).
//! A network is kept only as `WifiCredentialStore::fromJson` would keep it, at most eight, for
//! every file the C++ firmware writes. Hand-edited files may differ in corners the C++ firmware
//! never writes: a non-object among the `credentials`, a lone UTF-16 surrogate, or line breaks
//! in a password's base64 (mbedtls skips them, this does not).
//!
//! A password never leaves a `Network` except through [`Network::password`]: no `Debug` or
//! `Display` shows it. Nothing here allocates or panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod base64;
mod crc32;
mod entry;
mod reader;
#[cfg(test)]
mod tests;
mod text;

use bembeyaz_json::Parser;
use core::fmt;
use core::mem::MaybeUninit;
use entry::Field;
use reader::Reader;
pub use text::Text;

/// The longest network name Wi-Fi allows.
pub const SSID_MOST: usize = 32;
/// The longest password the C++ firmware keeps (`MAX_PASSWORD_LENGTH`).
pub const PASSWORD_MOST: usize = 64;
/// How many networks the C++ firmware keeps (`MAX_NETWORKS`).
pub const NETWORKS_MOST: usize = 8;
/// The length of the key the passwords are masked (XOR) with: a MAC address.
pub const KEY_LEN: usize = 6;

/// A saved network.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Network {
    ssid: Text<SSID_MOST>,
    password: Text<PASSWORD_MOST>,
}

impl Network {
    /// The network's name.
    #[must_use]
    pub fn ssid(&self) -> &Text<SSID_MOST> {
        &self.ssid
    }

    /// The plain password, empty for an open network.
    #[must_use]
    pub fn password(&self) -> &Text<PASSWORD_MOST> {
        &self.password
    }
}

/// The name's length and whether there is a password: never the password, nor the name.
impl fmt::Debug for Network {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        out.debug_struct("Network")
            .field("ssid_len", &self.ssid.len())
            .field("has_password", &!self.password.is_empty())
            .finish()
    }
}

/// The networks of one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Saved {
    networks: [Option<Network>; NETWORKS_MOST],
    count: usize,
    last: Option<usize>,
}

impl Saved {
    /// The networks in the file's order.
    pub fn networks(&self) -> impl Iterator<Item = &Network> {
        self.networks.iter().take(self.count).flatten()
    }

    /// How many networks were kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether none was kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// The network to join first: the one joined last, when it is saved, else the first (the C++
    /// firmware joins none on its own then; a test image tries the first).
    #[must_use]
    pub fn first_choice(&self) -> Option<&Network> {
        let index = self.last.unwrap_or(0);
        self.networks.get(index).and_then(Option::as_ref)
    }
}

/// Why the file could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The JSON is broken (a bad escape or literal, nesting too deep, an object that never ends,
    /// as a write cut short leaves it, or something after it): the C++ firmware keeps no network
    /// then either.
    Json,
}

/// The networks in `json` (the file's bytes), their passwords decoded with `key` (the device's
/// base MAC address).
///
/// # Errors
/// The JSON is broken; a network the C++ firmware would discard is only left out.
pub fn read(json: &[u8], key: &[u8; KEY_LEN]) -> Result<Saved, ReadError> {
    let mut slot = MaybeUninit::uninit();
    let parser = Parser::init(&mut slot);
    // A string too long for the buffer is cut, and so too long for its field, not dropped.
    parser.truncate_long_strings(true);
    let mut reader = Reader::new(key);
    parser.feed(json, &mut reader);
    if parser.has_error() || !reader.whole {
        return Err(ReadError::Json);
    }
    let last = match reader.last {
        Field::Given(ssid) if !ssid.is_empty() => reader
            .networks
            .iter()
            .take(reader.count)
            .position(|n| n.is_some_and(|n| n.ssid == ssid)),
        _ => None,
    };
    Ok(Saved {
        networks: reader.networks,
        count: reader.count,
        last,
    })
}
