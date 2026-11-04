//! The extra request headers a server keeps (OPDS servers, the KOReader sync server): a name and
//! the value as stored, obfuscated with the device's key, stored as an array of
//! `{"name", "value_obf"}` objects. [`Plain`] is a header with its value decoded.

use crate::copied;
use crate::password::{self, Decoded, Secret};
use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, Writer, items, or_str};

/// The headers a server keeps (`MAX_CUSTOM_HEADERS`; the C ABI's array).
pub const HEADERS: usize = 2;

/// An extra request header: its name and its value as stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Header<S = Vec<u8>> {
    pub name: S,
    pub value_obf: S,
}

/// A server's headers: the first `count` of `slots`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Headers<S = Vec<u8>> {
    pub slots: [Header<S>; HEADERS],
    pub count: usize,
}

impl<S: AsRef<[u8]>> Headers<S> {
    /// The headers' text size, for a writer's first estimate.
    pub(crate) fn text_size(&self) -> usize {
        self.slots
            .iter()
            .map(|h| h.name.as_ref().len() + h.value_obf.as_ref().len())
            .sum()
    }

    /// The array of the first `count` headers.
    pub(crate) fn write(&self, w: &mut Writer) {
        w.array(|w| {
            for header in self.slots.iter().take(self.count) {
                w.object(|w| {
                    w.key(b"name");
                    w.string(header.name.as_ref());
                    w.key(b"value_obf");
                    w.string(header.value_obf.as_ref());
                });
            }
        });
    }
}

/// The first two items of the array `list` (none when it is not one), their fields read as
/// strings (an item that is not an object, or a field that is not a string, gives empty text), as
/// views into the parsed text.
pub(crate) fn view(list: Option<&Value>) -> Headers<&[u8]> {
    let mut headers = Headers::default();
    for (slot, item) in headers.slots.iter_mut().zip(items(list)) {
        *slot = Header {
            name: or_str(item.get(b"name"), b""),
            value_obf: or_str(item.get(b"value_obf"), b""),
        };
        headers.count += 1;
    }
    headers
}

#[cfg(test)]
impl Headers<&[u8]> {
    /// These headers, copied out (for the tests).
    pub(crate) fn owned(&self) -> Headers {
        Headers {
            slots: self.slots.clone().map(|h| Header {
                name: h.name.to_vec(),
                value_obf: h.value_obf.to_vec(),
            }),
            count: self.count,
        }
    }
}

/// A header as the store holds it: its value plain, wiped when dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plain {
    pub name: Vec<u8>,
    pub value: Secret,
}

/// The first `count` of `stored`, their values decoded (empty when one does not decode).
///
/// # Errors
/// [`Error::NoMemory`] when the decoder or a copy runs out of memory.
pub(crate) fn plain<S: AsRef<[u8]>>(
    stored: &Headers<S>,
    decode: &impl Fn(&[u8], usize) -> Result<Decoded, Error>,
) -> Result<Vec<Plain>, Error> {
    let mut out = Vec::new();
    out.try_reserve_exact(HEADERS)
        .map_err(|_| Error::NoMemory)?;
    for header in stored.slots.iter().take(stored.count) {
        out.push(Plain {
            name: copied(header.name.as_ref())?,
            value: password::decoded(header.value_obf.as_ref(), decode)?,
        });
    }
    Ok(out)
}
