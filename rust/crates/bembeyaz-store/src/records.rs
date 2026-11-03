//! The small stores whose files are a list of records of texts, or one such record: the OPDS
//! tokens (`opds_tokens.json`), the LCP passphrases (`lcp_passphrases.json`) and the LCP device
//! identity (`lcp_device.json`); each one's list and fields are its [`Store`]'s. Deciding which
//! records to keep (an empty URL or provider skipped, a limit, the secrets decoded) stays in C++,
//! so this reads every record's fields as they are stored and writes the ones C++ gives.

use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str, parse};

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

impl Store {
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

fn record<'a, K: AsRef<[u8]>>(item: &'a Value, keys: &[K]) -> Vec<&'a [u8]> {
    keys.iter()
        .map(|key| or_str(item.get(key.as_ref()), b""))
        .collect()
}

/// Reads a store's text (`deserializeJson`) and hands `each` every record's fields, each read as a
/// string, up to a NUL as C++ reads a `const char*` (an item that is not an object, or a field that
/// is not a string, gives empty text), with how many records there are; the fields are views into
/// the parsed text, valid during the call. A list that is missing or not an array holds no record.
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; `each` is not called, and the
/// C++ store keeps its current records.
pub fn read_each<K: AsRef<[u8]>>(
    text: &[u8],
    shape: &Shape<'_, K>,
    mut each: impl FnMut(&[&[u8]], usize),
) -> Result<(), Error> {
    let doc = parse(text)?;
    match shape.list {
        Some(list) => {
            let list = items(doc.get(list));
            for item in list {
                each(&record(item, shape.keys), list.len());
            }
        }
        None => each(&record(&doc, shape.keys), 1),
    }
    Ok(())
}

/// [`read_each`]'s records, copied out.
///
/// # Errors
/// As [`read_each`].
pub fn read<K: AsRef<[u8]>>(text: &[u8], shape: &Shape<'_, K>) -> Result<Vec<Vec<Vec<u8>>>, Error> {
    let mut records = Vec::new();
    read_each(text, shape, |fields, _| {
        records.push(fields.iter().map(|f| f.to_vec()).collect());
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
