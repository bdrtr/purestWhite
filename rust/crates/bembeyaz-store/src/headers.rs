//! The extra request headers a server keeps (OPDS servers, the KOReader sync server): a name and
//! the value as stored, obfuscated (its obfuscation stays in C++). Stored as an array of
//! `{"name", "value_obf"}` objects.

use alloc::vec::Vec;
use bembeyaz_jsondoc::{Value, Writer, items, or_str};

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
/// strings (an item that is not an object, or a field that is not a string, gives empty text).
pub(crate) fn read(list: Option<&Value>) -> Headers {
    let mut headers = Headers::default();
    for (slot, item) in headers.slots.iter_mut().zip(items(list)) {
        let field = |key: &[u8]| or_str(item.get(key), b"").to_vec();
        *slot = Header {
            name: field(b"name"),
            value_obf: field(b"value_obf"),
        };
        headers.count += 1;
    }
    headers
}
