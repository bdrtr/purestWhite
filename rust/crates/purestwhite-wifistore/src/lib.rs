//! The Wi-Fi networks the C++ firmware saved on the card (`/.crosspoint/wifi.json`,
//! `WifiCredentialStore`), read for the bare-metal firmware: each network's name and password,
//! and the one it joined last. The C++ firmware keeps each password masked (XOR) with the device's
//! factory MAC address (its eFuse base MAC, 6 bytes) and base64-encoded (`password_obf`), with its
//! length (`password_len`) and CRC-32 (`password_crc32`); older files hold it plain (`password`).
//! The file is read by the C++ store's own Rust reader (`bembeyaz_store::wifi_credentials`), and
//! its networks are kept by `keep`, the C++ store's decisions in Rust, checked against its corpus;
//! so a network is kept as `WifiCredentialStore` keeps it, at most eight. This decodes the
//! passwords as mbedtls does, and leaves out a network whose name is longer than Wi-Fi's 32 bytes.
//! A file larger than [`JSON_MOST`] is refused: the C++ firmware writes a few KB at most.
//!
//! A password never leaves a `Network` except through [`Network::password`]: no `Debug` or
//! `Display` shows it, and its decoded copies on the heap are wiped when dropped (the file's bytes
//! and the parsed file, which hold it masked or, in an older file, plain, are not). Reading
//! allocates the parsed file and the networks kept on the global heap, as the C++ store does;
//! running out of memory there is an error, not an abort.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod base64;
#[cfg(test)]
mod tests;
mod text;

use base64::{DecodeError, decode};
use bembeyaz_jsondoc::Error;
use bembeyaz_store::password::{Decoded, Secret};
use bembeyaz_store::wifi_credentials::{self, Limits};
use core::fmt;
pub use text::Text;

/// The longest network name Wi-Fi allows.
pub const SSID_MOST: usize = 32;
/// The longest password the C++ firmware keeps (`MAX_PASSWORD_LENGTH`).
pub const PASSWORD_MOST: usize = 64;
/// How many networks the C++ firmware keeps (`MAX_NETWORKS`).
pub const NETWORKS_MOST: usize = 8;
/// The length of the key the passwords are masked (XOR) with: a MAC address.
pub const KEY_LEN: usize = 6;
/// The largest file read: eight networks with the longest names and passwords take about 2 KB, so
/// this leaves room for hand edits while keeping what is parsed and copied small.
pub const JSON_MOST: usize = 16 * 1024;

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
    /// as a write cut short leaves it): the C++ firmware keeps no network then either.
    Json,
    /// Memory ran out while the file was read.
    Memory,
    /// The file is larger than [`JSON_MOST`].
    TooLarge,
}

/// The networks in `json` (the file's bytes), their passwords decoded with `key` (the device's
/// base MAC address).
///
/// # Errors
/// The JSON is broken, or the file too large; a network the C++ firmware would discard is only
/// left out.
pub fn read(json: &[u8], key: &[u8; KEY_LEN]) -> Result<Saved, ReadError> {
    if json.len() > JSON_MOST {
        return Err(ReadError::TooLarge);
    }
    let limits = Limits {
        networks: NETWORKS_MOST,
        password: PASSWORD_MOST,
    };
    let kept = wifi_credentials::keep(json, limits, |text, most| decoded(text, most, *key))
        .map_err(|error| match error {
            Error::NoMemory => ReadError::Memory,
            _ => ReadError::Json,
        })?;
    let mut saved = Saved {
        networks: [None; NETWORKS_MOST],
        count: 0,
        last: None,
    };
    for network in &kept.kept {
        let (Some(ssid), Some(password)) = (
            Text::from_bytes(&network.ssid),
            Text::from_bytes(&network.password),
        ) else {
            continue;
        };
        if let Some(slot) = saved.networks.get_mut(saved.count) {
            *slot = Some(Network { ssid, password });
            saved.count += 1;
        }
    }
    let last = &kept.last_connected_ssid;
    saved.last = (!last.is_empty())
        .then(|| {
            saved
                .networks()
                .position(|n| n.ssid.as_bytes() == last.as_slice())
        })
        .flatten();
    Ok(saved)
}

/// A stored password decoded as `obfuscation::deobfuscateFromBase64` decodes it: empty text does
/// not decode (text of only spaces or line breaks decodes to an empty password), text that is not
/// base64 does not, text that decodes to more than `most` bytes is too long, and the rest is
/// unmasked (XOR) with `key`.
fn decoded(text: &[u8], most: usize, key: [u8; KEY_LEN]) -> Result<Decoded, Error> {
    if text.is_empty() {
        return Ok(Decoded::default());
    }
    Ok(match decode::<PASSWORD_MOST>(text) {
        Ok(mut password) if password.len() <= most => {
            password.xor(&key);
            Decoded {
                text: Secret::copy_of(password.as_bytes())?,
                ok: true,
                too_long: false,
            }
        }
        Ok(_) | Err(DecodeError::TooLong) => Decoded {
            too_long: true,
            ..Decoded::default()
        },
        Err(DecodeError::Invalid) => Decoded::default(),
    })
}
