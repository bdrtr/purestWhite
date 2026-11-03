//! `WifiCredentialStore`'s JSON (`src/WifiCredentialStore.cpp`, `/.crosspoint/wifi.json`): the
//! Wi-Fi networks saved and the one connected last. Each password is kept obfuscated with the
//! device's key, with its length and CRC-32 so a corrupted one is noticed. [`keep`] decides which
//! networks to keep (the password decoded, its length and checksum checked, at most
//! [`MOST_NETWORKS`] kept), why each other one is dropped, and whether to save again, for both
//! firmwares: the C++ store calls it with its decoder (`bb_store_wifi_credentials_keep`), the
//! bare-metal firmware with its own (`purestwhite-wifistore`); it is checked against the store's
//! corpus, its log lines included. [`write`] writes what the store gives, the length and CRC-32
//! worked out here.

use crate::copied;
use crate::field::{Kind, Number};
use crate::password::{self, Decoded, Secret};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};
use core::ops::ControlFlow;

/// How many networks the store keeps (`WifiCredentialStore::MAX_NETWORKS`).
pub const MOST_NETWORKS: usize = 8;
/// The longest password it keeps (`WifiCredentialStore::MAX_PASSWORD_LENGTH`).
pub const MOST_PASSWORD: usize = 64;

/// A credential as a file holds it, for the store to decide on. `password` is a legacy plain
/// password. `S` is the text: views into the parsed file, or owned.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Stored<S = Vec<u8>> {
    pub ssid: S,
    pub password_obf: S,
    pub password: S,
    pub password_len: Number,
    pub password_crc32: Number,
}

/// What a file holds: the network connected last, and every item of its "credentials" array (for
/// the tests).
#[cfg(test)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct File {
    pub last_connected_ssid: Vec<u8>,
    pub credentials: Vec<Stored>,
}

/// A credential to save: the password obfuscated, and plain, for the length and CRC-32 saved with
/// it (so a corrupted one is noticed when it is read back). `S` is the text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved<S> {
    pub ssid: S,
    pub password_obf: S,
    pub password: S,
}

/// Writes `wifi.json`'s text (`toJson`, then `serializeJson`), each password's length and CRC-32
/// worked out here; none when memory ran out.
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
                    let password = c.password.as_ref();
                    w.key(b"password_len");
                    w.unsigned(u64::try_from(password.len()).unwrap_or(u64::MAX));
                    w.key(b"password_crc32");
                    w.unsigned(crc32(password).into());
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
pub(crate) fn read_each(
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

/// [`read_each`]'s file, copied out (for the tests).
///
/// # Errors
/// As [`read_each`].
#[cfg(test)]
pub(crate) fn read(text: &[u8]) -> Result<File, Error> {
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

/// What made a password's checksum check fail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checksum {
    /// The CRC-32 does not match the password (`checksum mismatch`).
    Mismatch,
    /// The stored CRC-32 is not a `u32` (`invalid checksum`).
    Invalid,
}

/// Why the store dropped a network, for its log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// `password_len` is not an integer within `u32` (`invalid length`).
    InvalidLength,
    /// `password_len` is over the limit: the value.
    OversizedLength(u32),
    /// The password is longer than the limit (`readStoredPassword`'s `valid` false).
    OversizedPassword,
    /// The password does not match its stored length (the length stored and the one decoded), its
    /// checksum, or both.
    Corrupted {
        length: Option<(u32, usize)>,
        checksum: Option<Checksum>,
    },
}

/// A network dropped: its name (a view into the parsed file), and why.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dropped<'a> {
    pub ssid: &'a [u8],
    pub why: Why,
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

/// The networks `wifi.json`'s text gives, decided as the C++ store decided them before it called
/// this (`addRead` and `checkIntegrity`, last in `src/WifiCredentialStore.cpp` at 071be57a):
/// `decode(text, most)` decodes a stored password (at most `most` bytes), and `dropped` hears of
/// each network dropped, as it is (for the C++ store's log), in the file's order. Only the
/// networks kept are copied out of the parsed text, and each copy may fail, so a file that leaves
/// too little memory after it is parsed is an error, not an abort.
///
/// # Errors
/// When the text is not JSON, as [`read_each`]; [`Error::NoMemory`] when the decoder or a copy
/// runs out of memory.
pub fn keep(
    text: &[u8],
    limits: Limits,
    decode: impl Fn(&[u8], usize) -> Result<Decoded, Error>,
    mut dropped: impl FnMut(&Dropped<'_>),
) -> Result<Networks, Error> {
    let mut last_connected = Ok(Vec::new());
    let mut networks = Networks::default();
    let mut failed = None;
    read_each(
        text,
        |last| last_connected = copied(last),
        |stored, _| {
            if networks.kept.len() >= limits.networks {
                return ControlFlow::Break(());
            }
            match decide(stored, limits.password, &decode, &mut networks) {
                Ok(None) => ControlFlow::Continue(()),
                Ok(Some(why)) => {
                    networks.resave = true;
                    dropped(&Dropped {
                        ssid: stored.ssid,
                        why,
                    });
                    ControlFlow::Continue(())
                }
                Err(error) => {
                    failed = Some(error);
                    ControlFlow::Break(())
                }
            }
        },
    )?;
    if let Some(error) = failed {
        return Err(error);
    }
    networks.last_connected_ssid = last_connected?;
    Ok(networks)
}

/// One network kept into `networks`, or why it is dropped.
fn decide(
    stored: &Stored<&[u8]>,
    most: usize,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
    networks: &mut Networks,
) -> Result<Option<Why>, Error> {
    let password = match judge(stored, most, decode, &mut networks.resave)? {
        Ok(password) => password,
        Err(why) => return Ok(Some(why)),
    };
    networks.kept.try_reserve(1).map_err(|_| Error::NoMemory)?;
    networks.kept.push(Kept {
        ssid: copied(stored.ssid)?,
        password,
    });
    Ok(None)
}

/// The password of a network kept, or why it is dropped.
fn judge(
    stored: &Stored<&[u8]>,
    most: usize,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
    resave: &mut bool,
) -> Result<Result<Secret, Why>, Error> {
    let length = &stored.password_len;
    match length.kind {
        Kind::Other => return Ok(Err(Why::InvalidLength)),
        Kind::U32 if usize_of(length.value) > most => {
            return Ok(Err(Why::OversizedLength(length.value)));
        }
        _ => {}
    }
    let Some(password) =
        password::stored(stored.password_obf, stored.password, most, decode, resave)?
    else {
        return Ok(Err(Why::OversizedPassword));
    };
    Ok(match integrity(&password, stored, resave) {
        None => Ok(password),
        Some(why) => Err(why),
    })
}

fn usize_of(n: u32) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// What is wrong with the password against its stored length and CRC-32, if anything; an older
/// file without them is kept and saved again with them.
fn integrity(password: &[u8], stored: &Stored<&[u8]>, resave: &mut bool) -> Option<Why> {
    let mut length = None;
    if stored.password_len.kind == Kind::Missing {
        *resave = true;
    } else if password.len() != usize_of(stored.password_len.value) {
        length = Some((stored.password_len.value, password.len()));
    }
    let checksum = match stored.password_crc32.kind {
        Kind::U32 if crc32(password) != stored.password_crc32.value => Some(Checksum::Mismatch),
        Kind::U32 => None,
        Kind::Missing => {
            *resave = true;
            None
        }
        Kind::Other => Some(Checksum::Invalid),
    };
    (length.is_some() || checksum.is_some()).then_some(Why::Corrupted { length, checksum })
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
