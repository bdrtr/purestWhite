//! The `KOReader` sync account the C++ firmware saved (`/.crosspoint/koreader.json`,
//! `KOReaderCredentialStore`), read for the bare-metal firmware, and the request that checks it
//! (`KOReaderSyncClient::authenticate`). The file is read and its settings made by the C++ store's
//! own Rust code (`bembeyaz_store::koreader_settings`), the secrets decoded as the C++ firmware
//! decodes them (`purestwhite-obfuscation`). [`auth`] writes a request signed in as the C++ client
//! signs in, the account check among them, and reads its answer. Nothing here panics; running out
//! of memory is an error.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod auth;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use bembeyaz_jsondoc::Error;
use bembeyaz_store::koreader_settings::{Settings, base_url, load};
use purestwhite_obfuscation::{KEY_LEN, decoded};

/// The C++ firmware's `KOReader` sync account, on the card.
pub const SETTINGS_PATH: &str = "/.crosspoint/koreader.json";
/// The largest file read: the C++ firmware writes well under 2 KB.
pub const JSON_MOST: usize = 16 * 1024;

/// Why the file could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The JSON is broken: the C++ firmware keeps its settings then.
    Json,
    /// The file is larger than [`JSON_MOST`].
    TooLarge,
    /// Memory ran out while the file was read.
    Memory,
}

/// The sync account, as the C++ store holds it after loading the file. Its password and header
/// values are wiped when it is dropped.
pub struct Account {
    settings: Settings,
}

/// Whether there is an account, and how many extra headers: never a name, password or server.
impl core::fmt::Debug for Account {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        out.debug_struct("Account")
            .field("has_credentials", &self.has_credentials())
            .field("headers", &self.settings.headers.len())
            .finish()
    }
}

impl Account {
    /// The settings, as the C++ store holds them.
    #[must_use]
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Whether there is an account to sign in with (`hasCredentials`).
    #[must_use]
    pub fn has_credentials(&self) -> bool {
        self.settings.has_credentials()
    }

    /// The server's base URL (`getBaseUrl`).
    ///
    /// # Errors
    /// [`Error::NoMemory`] when memory runs out.
    pub fn base_url(&self) -> Result<Vec<u8>, Error> {
        base_url(&self.settings.server_url)
    }
}

/// The account in `json` (the file's bytes), its secrets decoded with `key` (the device's base MAC
/// address).
///
/// # Errors
/// The JSON is broken, the file too large, or memory ran out.
pub fn read(json: &[u8], key: &[u8; KEY_LEN]) -> Result<Account, ReadError> {
    if json.len() > JSON_MOST {
        return Err(ReadError::TooLarge);
    }
    let settings =
        load(json, |text, most| decoded(text, most, key)).map_err(|error| match error {
            Error::NoMemory => ReadError::Memory,
            _ => ReadError::Json,
        })?;
    Ok(Account { settings })
}
