//! The small stores whose files are a list of records of texts, or one such record: the OPDS
//! tokens (`opds_tokens.json`), the LCP passphrases (`lcp_passphrases.json`) and the LCP device
//! identity (`lcp_device.json`); each one's list and fields are its [`Store`]'s, and so is what it
//! makes of a record it reads (its [`Rule`]: the secrets decoded, a record without its URL or
//! provider skipped, a limit). [`load`] gives the records a store keeps (the C++ firmware calls
//! it, and the bare-metal one can); [`write`] writes the ones it gives.

use crate::copied;
use crate::password::{Decoded, Secret};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};
use core::ops::ControlFlow;

/// Where a store's records are: under the array `list` of the root object, or, when `list` is
/// `None`, the root object itself, one record; each with the fields `keys`, in that order.
#[derive(Clone, Copy, Debug)]
pub struct Shape<'a, K> {
    pub list: Option<&'a [u8]>,
    pub keys: &'a [K],
}

/// The fields of an OPDS token entry (`OpdsTokenStore`).
pub const OPDS_TOKEN_KEYS: [&[u8]; 4] = [b"url", b"access_obf", b"refresh_obf", b"refresh_url"];
/// The fields of an LCP passphrase entry (`LcpPassphraseStore`).
pub const LCP_PASSPHRASE_KEYS: [&[u8]; 2] = [b"provider", b"key_obf"];
/// The fields of the LCP device identity (`LcpDeviceStore`).
pub const LCP_DEVICE_KEYS: [&[u8]; 2] = [b"device_id", b"priv_obf"];

/// The stores of records of texts; each value is `store_records.h`'s `BB_STORE_RECORDS_*` for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Store {
    /// `opds_tokens.json`: the list "tokens".
    OpdsTokens = 0,
    /// `lcp_passphrases.json`: the list "passphrases".
    LcpPassphrases = 1,
    /// `lcp_device.json`: one record, the root object.
    LcpDevice = 2,
}

/// A secret field of a record, and the most bytes it may decode to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SecretField {
    pub field: usize,
    pub most: usize,
}

/// What a store makes of a record it reads (`parseEntry` in `src/OpdsTokenStore.cpp` and
/// `src/LcpPassphraseStore.cpp`, `fromJsonText` in `src/network/LcpDeviceStore.cpp`, at 2f0c7c97).
/// Its `secrets` are decoded, each empty when it does not decode or is longer than its limit. A
/// record is kept only when each of its `required` fields is not empty: the plain ones are checked
/// first, so a record skipped for them is never decoded, and the secret ones after decoding. At
/// most `most` records are kept; the ones after are not looked at.
#[derive(Clone, Copy, Debug)]
pub struct Rule {
    pub secrets: &'static [SecretField],
    pub required: &'static [usize],
    pub most: usize,
}

/// An OPDS token entry's fields, in `OPDS_TOKEN_KEYS`' order.
pub const OPDS_TOKEN_URL: usize = 0;
pub const OPDS_TOKEN_ACCESS: usize = 1;
pub const OPDS_TOKEN_REFRESH: usize = 2;
pub const OPDS_TOKEN_REFRESH_URL: usize = 3;
/// An LCP passphrase's fields, in `LCP_PASSPHRASE_KEYS`' order.
pub const LCP_PASSPHRASE_PROVIDER: usize = 0;
pub const LCP_PASSPHRASE_KEY: usize = 1;
/// The LCP device identity's fields, in `LCP_DEVICE_KEYS`' order.
pub const LCP_DEVICE_ID: usize = 0;
pub const LCP_DEVICE_PRIV: usize = 1;
/// The most fields a record of any of the stores has.
pub const MOST_FIELDS: usize = 4;
const _: () = assert!(
    OPDS_TOKEN_KEYS.len() <= MOST_FIELDS
        && LCP_PASSPHRASE_KEYS.len() <= MOST_FIELDS
        && LCP_DEVICE_KEYS.len() <= MOST_FIELDS
);

/// How many OPDS token entries the store keeps (`OpdsTokenStore::MAX_SERVERS`).
pub const MOST_OPDS_TOKENS: usize = 8;
/// How many LCP passphrases it keeps (`LcpPassphraseStore::MAX_PROVIDERS`).
pub const MOST_LCP_PASSPHRASES: usize = 8;
/// The longest OPDS token decoded (was `MAX_TOKEN_CHARS` in C++: a JWT runs long, but a malformed
/// file must not fill the heap).
pub const MOST_TOKEN: usize = 2048;
/// The longest LCP hash decoded: a hex SHA-256 (was `MAX_HASH_CHARS`), as the device key.
pub const MOST_HASH: usize = 64;

impl Store {
    /// What the store makes of a record it reads.
    #[must_use]
    pub fn rule(self) -> Rule {
        match self {
            Store::OpdsTokens => Rule {
                secrets: &[
                    SecretField {
                        field: OPDS_TOKEN_ACCESS,
                        most: MOST_TOKEN,
                    },
                    SecretField {
                        field: OPDS_TOKEN_REFRESH,
                        most: MOST_TOKEN,
                    },
                ],
                required: &[OPDS_TOKEN_URL],
                most: MOST_OPDS_TOKENS,
            },
            Store::LcpPassphrases => Rule {
                secrets: &[SecretField {
                    field: LCP_PASSPHRASE_KEY,
                    most: MOST_HASH,
                }],
                required: &[LCP_PASSPHRASE_PROVIDER, LCP_PASSPHRASE_KEY],
                most: MOST_LCP_PASSPHRASES,
            },
            Store::LcpDevice => Rule {
                secrets: &[SecretField {
                    field: LCP_DEVICE_PRIV,
                    most: MOST_HASH,
                }],
                required: &[],
                most: 1,
            },
        }
    }

    /// Where the store's records are, and their fields.
    #[must_use]
    pub fn shape(self) -> Shape<'static, &'static [u8]> {
        match self {
            Store::OpdsTokens => Shape {
                list: Some(b"tokens"),
                keys: &OPDS_TOKEN_KEYS,
            },
            Store::LcpPassphrases => Shape {
                list: Some(b"passphrases"),
                keys: &LCP_PASSPHRASE_KEYS,
            },
            Store::LcpDevice => Shape {
                list: None,
                keys: &LCP_DEVICE_KEYS,
            },
        }
    }
}

/// The fields of `item` (at most `MOST_FIELDS`), on the stack.
fn record<'a, K: AsRef<[u8]>>(item: &'a Value, keys: &[K]) -> ([&'a [u8]; MOST_FIELDS], usize) {
    let mut fields: [&[u8]; MOST_FIELDS] = [&[]; MOST_FIELDS];
    for (field, key) in fields.iter_mut().zip(keys) {
        *field = or_str(item.get(key.as_ref()), b"");
    }
    (fields, keys.len().min(MOST_FIELDS))
}

/// Reads a store's text (`deserializeJson`) and hands `each` the records' fields, until it breaks
/// (a root-object store has one record, handed over whatever `each` says), each read as a string,
/// up to a NUL as C++ reads a `const char*` (an item that is not an object, or a field that is not
/// a string, gives empty text), with how many records there are; the fields are views into the
/// parsed text, valid during the call. A list that is missing or not an array holds no record.
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; `each` is not called, and the
/// C++ store keeps its current records.
pub(crate) fn read_each<K: AsRef<[u8]>>(
    text: &[u8],
    shape: &Shape<'_, K>,
    mut each: impl FnMut(&[&[u8]], usize) -> ControlFlow<()>,
) -> Result<(), Error> {
    let doc = parse(text)?;
    let Some(list) = shape.list else {
        let (fields, n) = record(&doc, shape.keys);
        let _ = each(fields.get(..n).unwrap_or_default(), 1);
        return Ok(());
    };
    let list = items(doc.get(list));
    for item in list {
        let (fields, n) = record(item, shape.keys);
        if each(fields.get(..n).unwrap_or_default(), list.len()).is_break() {
            break;
        }
    }
    Ok(())
}

/// [`read_each`]'s records, copied out (for the tests).
///
/// # Errors
/// As [`read_each`].
#[cfg(test)]
pub(crate) fn read<K: AsRef<[u8]>>(
    text: &[u8],
    shape: &Shape<'_, K>,
) -> Result<Vec<Vec<Vec<u8>>>, Error> {
    let mut records = Vec::new();
    read_each(text, shape, |fields, _| {
        records.push(fields.iter().map(|f| f.to_vec()).collect());
        ControlFlow::Continue(())
    })?;
    Ok(records)
}

fn write_record<K: AsRef<[u8]>, S: AsRef<[u8]>>(w: &mut Writer, keys: &[K], fields: &[S]) {
    w.object(|w| {
        for (key, field) in keys.iter().zip(fields) {
            w.key(key.as_ref());
            w.string(field.as_ref());
        }
    });
}

/// Writes a store's text (`toJson`, then `serializeJson`) for `fields`, the records' fields one
/// record after another (`shape.keys.len()` each; a root-object store writes the first record);
/// none when memory ran out.
#[must_use]
pub fn write<K: AsRef<[u8]>, S: AsRef<[u8]>>(
    shape: &Shape<'_, K>,
    fields: &[S],
) -> Option<Vec<u8>> {
    let width = shape.keys.len().max(1);
    let size: usize = fields.iter().map(|f| 16 + f.as_ref().len()).sum();
    let mut w = Writer::with_capacity(32 + size);
    match shape.list {
        Some(list) => w.object(|w| {
            w.key(list);
            w.array(|w| {
                for one in fields.chunks(width) {
                    write_record(w, shape.keys, one);
                }
            });
        }),
        None => write_record(&mut w, shape.keys, fields.get(..width).unwrap_or(fields)),
    }
    w.finish()
}

/// The records `store` keeps of its text, each its fields in the store's order, a secret one
/// decoded with `decode(text, most)`; every field is held as a [`Secret`], wiped when dropped.
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives (the store then keeps its
/// records); [`Error::NoMemory`] when the decoder or a copy runs out of memory.
pub fn load(
    store: Store,
    text: &[u8],
    decode: impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Vec<Vec<Secret>>, Error> {
    let rule = store.rule();
    let mut kept: Vec<Vec<Secret>> = Vec::new();
    let mut failed = None;
    read_each(text, &store.shape(), |fields, _| {
        if kept.len() >= rule.most {
            return ControlFlow::Break(());
        }
        match keep(fields, &rule, &decode) {
            Ok(Some(record)) => match kept.try_reserve(1) {
                Ok(()) => kept.push(record),
                Err(_) => failed = Some(Error::NoMemory),
            },
            Ok(None) => {}
            Err(error) => failed = Some(error),
        }
        if failed.is_some() {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    })?;
    match failed {
        Some(error) => Err(error),
        None => Ok(kept),
    }
}

/// One record as the store keeps it, or none when it skips it.
fn keep(
    fields: &[&[u8]],
    rule: &Rule,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Option<Vec<Secret>>, Error> {
    let secret = |i: usize| rule.secrets.iter().find(|s| s.field == i).map(|s| s.most);
    let empty = |i: &usize| fields.get(*i).is_none_or(|f| f.is_empty());
    if rule
        .required
        .iter()
        .filter(|i| secret(**i).is_none())
        .any(empty)
    {
        return Ok(None);
    }
    let mut record = Vec::new();
    record
        .try_reserve_exact(fields.len())
        .map_err(|_| Error::NoMemory)?;
    for (i, field) in fields.iter().enumerate() {
        record.push(match secret(i) {
            Some(most) => decoded_at_most(field, most, decode)?,
            None => Secret::new(copied(field)?),
        });
    }
    let kept = rule
        .required
        .iter()
        .all(|i| record.get(*i).is_some_and(|f| !f.is_empty()));
    Ok(kept.then_some(record))
}

/// `obfuscated` decoded (`deobfuscateFromBase64(text, most, ...)`): empty when it does not decode
/// or is longer than `most`.
fn decoded_at_most(
    obfuscated: &[u8],
    most: usize,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Secret, Error> {
    let decoded = decode(obfuscated, most)?;
    Ok(if decoded.ok && !decoded.too_long {
        decoded.text
    } else {
        Secret::default()
    })
}
