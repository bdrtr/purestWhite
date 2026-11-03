//! The C++ firmware's obfuscation of stored secrets (`lib/Serialization/ObfuscationUtils.cpp`),
//! undone for the bare-metal firmware: a secret is masked (XOR) with the device's factory MAC
//! address (its eFuse base MAC, 6 bytes) and base64-encoded. [`decoded`] is
//! `obfuscation::deobfuscateFromBase64`, as `bembeyaz_store::password`'s decoder; [`base64`] also
//! encodes, for an HTTP Basic sign-in. Nothing here panics; running out of memory is an error.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod base64;
#[cfg(test)]
mod tests;

use alloc::vec::Vec;
use bembeyaz_jsondoc::Error;
use bembeyaz_store::password::{Decoded, Secret};

/// The length of the key secrets are masked with: a MAC address.
pub const KEY_LEN: usize = 6;

/// A stored secret decoded as `obfuscation::deobfuscateFromBase64` decodes it: empty text does
/// not decode (text of only spaces or line breaks decodes to an empty secret), text that is not
/// base64 (as mbedtls reads it) does not, text that decodes to more than `most` bytes is too long,
/// and the rest is unmasked (XOR) with `key`.
///
/// # Errors
/// [`Error::NoMemory`] when the plain text cannot be held.
pub fn decoded(text: &[u8], most: usize, key: &[u8; KEY_LEN]) -> Result<Decoded, Error> {
    if text.is_empty() {
        return Ok(Decoded::default());
    }
    let Ok(len) = base64::decoded_len(text) else {
        return Ok(Decoded::default());
    };
    if len > most {
        return Ok(Decoded {
            too_long: true,
            ..Decoded::default()
        });
    }
    let mut plain = Vec::new();
    plain.try_reserve_exact(len).map_err(|_| Error::NoMemory)?;
    base64::decode_into(text, &mut plain);
    for (byte, k) in plain.iter_mut().zip(key.iter().cycle()) {
        *byte ^= k;
    }
    Ok(Decoded {
        text: Secret::new(plain),
        ok: true,
        too_long: false,
    })
}
