//! `WifiCredentialStore`'s JSON (`src/WifiCredentialStore.cpp`, `/.crosspoint/wifi.json`): the
//! Wi-Fi networks saved and the one connected last. Each password is kept obfuscated with the
//! device's key, with its length and CRC-32 so a corrupted one is noticed. On the C++ firmware,
//! deciding which networks to keep (the password decoded, its length and checksum checked, at most
//! `MAX_NETWORKS` kept) and whether to save again stays in C++: [`read_each`] hands over each
//! item's stored fields as they are, each number with its kind, and [`write`] writes what C++
//! gives. [`keep`] makes the same decisions in Rust for the bare-metal firmware
//! (`purestwhite-wifistore`); it is checked against the same corpus, so a change to the C++
//! checks must be made in `keep` too.

use crate::copied;
use crate::field::{Kind, Number};
use crate::password::{self, Decoded, Secret};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};
use core::ops::ControlFlow;

/// A credential as a file holds it, for the store to decide on. `password` is a legacy plain
/// password. `S` is the text: views into the parsed file, or owned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stored<S = Vec<u8>> {
    pub ssid: S,
    pub password_obf: S,
    pub password: S,
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

/// Reads `wifi.json`'s text (`deserializeJson`) and hands `last` the network connected last, then
/// `each` every item of the "credentials" array (none when it has no such array) with how many
/// there are, until `each` breaks; texts are read as strings, up to a NUL as C++ reads a
/// `const char*` (an item that is not an object, or a field that is not a string, gives empty
/// text), as views into the parsed text, valid during the call.
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; neither is called, and the
/// C++ store then keeps its current networks.
pub fn read_each(
    text: &[u8],
    last: impl FnOnce(&[u8]),
    mut each: impl FnMut(&Stored<&[u8]>, usize) -> ControlFlow<()>,
) -> Result<(), Error> {
    fn field<'a>(item: &'a Value, key: &[u8]) -> &'a [u8] {
        or_str(item.get(key), b"")
    }
    let doc = parse(text)?;
    last(field(&doc, b"lastConnectedSsid"));
    let credentials = items(doc.get(b"credentials"));
    for item in credentials {
        let stored = Stored {
            ssid: field(item, b"ssid"),
            password_obf: field(item, b"password_obf"),
            password: field(item, b"password"),
            password_len: Number::of(item.get(b"password_len")),
            password_crc32: Number::of(item.get(b"password_crc32")),
        };
        if each(&stored, credentials.len()).is_break() {
            break;
        }
    }
    Ok(())
}

/// [`read_each`]'s file, copied out.
///
/// # Errors
/// As [`read_each`].
pub fn read(text: &[u8]) -> Result<File, Error> {
    let mut file = File::default();
    let mut credentials = Vec::new();
    read_each(
        text,
        |last| file.last_connected_ssid = last.to_vec(),
        |s, _| {
            credentials.push(Stored {
                ssid: s.ssid.to_vec(),
                password_obf: s.password_obf.to_vec(),
                password: s.password.to_vec(),
                password_len: s.password_len,
                password_crc32: s.password_crc32,
            });
            ControlFlow::Continue(())
        },
    )?;
    file.credentials = credentials;
    Ok(file)
}

/// A network kept: its name and plain password.
#[derive(Clone, PartialEq, Eq)]
pub struct Kept {
    pub ssid: Vec<u8>,
    pub password: Secret,
}

/// The name's length and whether there is a password: never the password, nor the name.
impl core::fmt::Debug for Kept {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        out.debug_struct("Kept")
            .field("ssid_len", &self.ssid.len())
            .field("has_password", &!self.password.is_empty())
            .finish()
    }
}

/// The networks a file gives, as `WifiCredentialStore` keeps them, and whether it saves the file
/// again (an older shape, or a network dropped).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Networks {
    pub last_connected_ssid: Vec<u8>,
    pub kept: Vec<Kept>,
    pub resave: bool,
}

/// The limits `WifiCredentialStore` keeps to (`MAX_NETWORKS`, `MAX_PASSWORD_LENGTH`).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub networks: usize,
    pub password: usize,
}

/// The networks `wifi.json`'s text gives, decided as `WifiCredentialStore::fromJsonText` decides
/// them (`addRead` and `checkIntegrity` in `src/WifiCredentialStore.cpp`, which this must follow):
/// `decode(text, most)` decodes a stored password (at most `most` bytes). Only the networks kept
/// are copied out of the parsed text, and each copy may fail, so a file that leaves too little
/// memory after it is parsed is an error, not an abort.
///
/// # Errors
/// When the text is not JSON, as [`read_each`]; [`Error::NoMemory`] when a copy runs out of memory.
pub fn keep(
    text: &[u8],
    limits: Limits,
    decode: impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Networks, Error> {
    let mut last_connected = Ok(Vec::new());
    let mut kept = Vec::new();
    let mut resave = false;
    let mut failed = None;
    read_each(
        text,
        |last| last_connected = copied(last),
        |stored, _| {
            if kept.len() >= limits.networks {
                return ControlFlow::Break(());
            }
            let one = decide(stored, limits.password, &decode, &mut resave);
            match one.and_then(|one| {
                kept.try_reserve(1)
                    .map(|()| one)
                    .map_err(|_| Error::NoMemory)
            }) {
                Ok(Some(one)) => kept.push(one),
                Ok(None) => resave = true,
                Err(error) => {
                    failed = Some(error);
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        },
    )?;
    if let Some(error) = failed {
        return Err(error);
    }
    Ok(Networks {
        last_connected_ssid: last_connected?,
        kept,
        resave,
    })
}

/// One network kept, or `None` when the C++ store drops it (which saves the file again).
fn decide(
    stored: &Stored<&[u8]>,
    most: usize,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
    resave: &mut bool,
) -> Result<Option<Kept>, Error> {
    let length = &stored.password_len;
    if length.kind == Kind::Other || (length.kind == Kind::U32 && usize_of(length.value) > most) {
        return Ok(None);
    }
    let Some(password) =
        password::stored(stored.password_obf, stored.password, most, decode, resave)?
    else {
        return Ok(None);
    };
    if !integrity_valid(&password, stored, resave) {
        return Ok(None);
    }
    Ok(Some(Kept {
        ssid: copied(stored.ssid)?,
        password,
    }))
}

fn usize_of(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// Whether the password matches its stored length and CRC-32 (`checkIntegrity`); an older file
/// without them is kept and saved again with them.
fn integrity_valid(password: &[u8], stored: &Stored<&[u8]>, resave: &mut bool) -> bool {
    let mut valid = true;
    if stored.password_len.kind == Kind::Missing {
        *resave = true;
    } else if password.len() != usize_of(stored.password_len.value) {
        valid = false;
    }
    match stored.password_crc32.kind {
        Kind::U32 => valid &= crc32(password) == stored.password_crc32.value,
        Kind::Missing => *resave = true,
        Kind::Other => valid = false,
    }
    valid
}

/// IEEE CRC-32 (`credential_integrity::crc32`).
#[must_use]
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}
