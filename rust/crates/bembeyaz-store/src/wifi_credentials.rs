//! `WifiCredentialStore`'s JSON (`src/WifiCredentialStore.cpp`, `/.crosspoint/wifi.json`): the
//! Wi-Fi networks saved and the one connected last. Each password is kept obfuscated with the
//! device's key, with its length and CRC-32 so a corrupted one is noticed. Deciding which
//! networks to keep (the password decoded, its length and checksum checked, at most
//! `MAX_NETWORKS` kept) and whether to save again stays in C++, so this reads each item's stored
//! fields as they are, each number with its kind, and writes what C++ gives.

use crate::field::Number;
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};

/// A credential as a file holds it, for C++ to decide on. `password` is a legacy plain password.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored {
    pub ssid: Vec<u8>,
    pub password_obf: Vec<u8>,
    pub password: Vec<u8>,
    pub password_len: Number,
    pub password_crc32: Number,
}

/// What a file holds: the network connected last, and every item of its "credentials" array.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct File {
    pub last_connected_ssid: Vec<u8>,
    pub credentials: Vec<Stored>,
}

/// A credential to save: the password obfuscated, its length and its CRC-32. `S` is the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved<S> {
    pub ssid: S,
    pub password_obf: S,
    pub password_len: u64,
    pub password_crc32: u32,
}

/// Writes `wifi.json`'s text (`toJson`, then `serializeJson`); none when memory ran out.
#[must_use]
pub fn write<S: AsRef<[u8]>>(
    last_connected_ssid: &[u8],
    credentials: &[Saved<S>],
) -> Option<Vec<u8>> {
    let size: usize = credentials
        .iter()
        .map(|c| 96 + c.ssid.as_ref().len() + c.password_obf.as_ref().len())
        .sum();
    let mut w = Writer::with_capacity(48 + last_connected_ssid.len() + size);
    w.object(|w| {
        w.key(b"lastConnectedSsid");
        w.string(last_connected_ssid);
        w.key(b"credentials");
        w.array(|w| {
            for c in credentials {
                w.object(|w| {
                    w.key(b"ssid");
                    w.string(c.ssid.as_ref());
                    w.key(b"password_obf");
                    w.string(c.password_obf.as_ref());
                    w.key(b"password_len");
                    w.unsigned(c.password_len);
                    w.key(b"password_crc32");
                    w.unsigned(c.password_crc32.into());
                });
            }
        });
    });
    w.finish()
}

/// Reads `wifi.json`'s text (`deserializeJson`): the network connected last, and every item of
/// the "credentials" array (none when it has no such array), its texts read as strings, up to a
/// NUL as C++ reads a `const char*` (an item that is not an object, or a field that is not a
/// string, gives empty text).
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; the C++ store then keeps its
/// current networks.
pub fn read(text: &[u8]) -> Result<File, Error> {
    let doc = parse(text)?;
    let field = |item: &Value, key: &[u8]| or_str(item.get(key), b"").to_vec();
    Ok(File {
        last_connected_ssid: field(&doc, b"lastConnectedSsid"),
        credentials: items(doc.get(b"credentials"))
            .iter()
            .map(|item| Stored {
                ssid: field(item, b"ssid"),
                password_obf: field(item, b"password_obf"),
                password: field(item, b"password"),
                password_len: Number::of(item.get(b"password_len")),
                password_crc32: Number::of(item.get(b"password_crc32")),
            })
            .collect(),
    })
}
