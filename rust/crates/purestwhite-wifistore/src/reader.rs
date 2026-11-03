//! The file's JSON events gathered into its networks: `lastConnectedSsid` at the top, and each
//! object of the top's `credentials` array an `Entry`.

use crate::entry::{Entry, Field};
use crate::text::Text;
use crate::{NETWORKS_MOST, Network, SSID_MOST};
use bembeyaz_json::{Events, Token};

/// A field of the file the reader keeps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Key {
    LastConnected,
    Credentials,
    Ssid,
    Obfuscated,
    Plain,
    Length,
    Crc,
    Other,
}

impl Key {
    fn of(name: &[u8]) -> Key {
        match name {
            b"lastConnectedSsid" => Key::LastConnected,
            b"credentials" => Key::Credentials,
            b"ssid" => Key::Ssid,
            b"password_obf" => Key::Obfuscated,
            b"password" => Key::Plain,
            b"password_len" => Key::Length,
            b"password_crc32" => Key::Crc,
            _ => Key::Other,
        }
    }
}

/// The networks gathered so far, with the device's key to decode their passwords.
pub(crate) struct Reader<'k> {
    key: &'k [u8],
    depth: usize,
    /// The key of the value to come, at the depth it was read.
    key_at: Option<(Key, usize)>,
    /// The depth of the `credentials` array, while it is read.
    credentials: Option<usize>,
    entry: Option<Entry>,
    pub last: Field<Text<SSID_MOST>>,
    pub networks: [Option<Network>; NETWORKS_MOST],
    pub count: usize,
    /// The top object ended, and nothing came after it: a whole document, as `ArduinoJson` takes
    /// it.
    pub whole: bool,
    /// Something came after the top object.
    trailing: bool,
}

impl<'k> Reader<'k> {
    pub(crate) fn new(key: &'k [u8]) -> Self {
        Reader {
            key,
            depth: 0,
            key_at: None,
            credentials: None,
            entry: None,
            last: Field::Missing,
            networks: [None; NETWORKS_MOST],
            count: 0,
            whole: false,
            trailing: false,
        }
    }

    /// Notes a value or container starting where the document has already ended.
    fn after_end(&mut self) {
        if self.depth == 0 && self.whole {
            self.trailing = true;
            self.whole = false;
        }
    }

    /// The key the value just read belongs to, at this depth (taken: a value has one key).
    fn value_key(&mut self) -> Key {
        match self.key_at.take() {
            Some((key, depth)) if depth == self.depth => key,
            _ => Key::Other,
        }
    }

    fn in_entry(&self) -> bool {
        self.entry.is_some() && self.credentials.is_some_and(|d| self.depth == d + 1)
    }

    /// A string value for `key`.
    fn text(&mut self, key: Key, bytes: &[u8]) {
        let top = self.depth == 1;
        let entry = if self.in_entry() {
            self.entry.as_mut()
        } else {
            None
        };
        match (key, top, entry) {
            (Key::LastConnected, true, _) => self.last = field(bytes),
            (Key::Ssid, _, Some(entry)) => entry.ssid = field(bytes),
            (Key::Obfuscated, _, Some(entry)) => entry.obfuscated = field(bytes),
            (Key::Plain, _, Some(entry)) => entry.plain = field(bytes),
            (Key::Length, _, Some(entry)) => entry.length = Field::Wrong,
            (Key::Crc, _, Some(entry)) => entry.crc = Field::Wrong,
            _ => {}
        }
    }

    /// A number value for `key`: a length or a checksum, a whole number that fits 32 bits.
    fn number(&mut self, key: Key, bytes: &[u8]) {
        // `ArduinoJson` stores `-0` as the integer 0.
        let digits = bytes
            .strip_prefix(b"-")
            .filter(|d| d.iter().all(|&c| c == b'0'));
        let value = core::str::from_utf8(digits.unwrap_or(bytes))
            .ok()
            .and_then(|text| text.parse::<u32>().ok())
            .map_or(Field::Wrong, Field::Given);
        if key == Key::LastConnected && self.depth == 1 {
            self.last = Field::Missing;
        }
        if !self.in_entry() {
            return;
        }
        if let Some(entry) = self.entry.as_mut() {
            match key {
                Key::Length => entry.length = value,
                Key::Crc => entry.crc = value,
                Key::Ssid | Key::Obfuscated | Key::Plain => self.wrong(key),
                _ => {}
            }
        }
    }

    /// A value of the wrong kind for a string field: the C++ firmware reads it as empty.
    fn wrong(&mut self, key: Key) {
        if let Some(entry) = self.entry.as_mut() {
            match key {
                Key::Ssid => entry.ssid = Field::Wrong,
                Key::Obfuscated => entry.obfuscated = Field::Wrong,
                Key::Plain => entry.plain = Field::Wrong,
                Key::Length => entry.length = Field::Wrong,
                Key::Crc => entry.crc = Field::Wrong,
                _ => {}
            }
        }
    }

    /// `key`'s field as if it were not given.
    fn missing(&mut self, key: Key) {
        if key == Key::LastConnected && self.depth == 1 {
            self.last = Field::Missing;
        }
        if !self.in_entry() {
            return;
        }
        if let Some(entry) = self.entry.as_mut() {
            match key {
                Key::Ssid => entry.ssid = Field::Missing,
                Key::Obfuscated => entry.obfuscated = Field::Missing,
                Key::Plain => entry.plain = Field::Missing,
                Key::Length => entry.length = Field::Missing,
                Key::Crc => entry.crc = Field::Missing,
                _ => {}
            }
        }
    }

    /// The entry ended: kept when the C++ firmware would keep it, up to `NETWORKS_MOST`.
    fn finish(&mut self) {
        let Some(entry) = self.entry.take() else {
            return;
        };
        if let Some(slot) = self.networks.get_mut(self.count)
            && let Some(network) = entry.network(self.key)
        {
            *slot = Some(network);
            self.count += 1;
        }
    }
}

/// A string field, cut at its first NUL as the C++ firmware reads it (a `const char*`).
fn field<const N: usize>(bytes: &[u8]) -> Field<Text<N>> {
    let text = bytes.split(|&b| b == 0).next().unwrap_or_default();
    Text::from_bytes(text).map_or(Field::TooLong, Field::Given)
}

impl Events for Reader<'_> {
    fn key(&mut self, key: Token<'_>) {
        self.key_at = Some((Key::of(key.bytes()), self.depth));
    }

    fn string(&mut self, value: Token<'_>) {
        self.after_end();
        let key = self.value_key();
        self.text(key, value.bytes());
    }

    fn number(&mut self, value: Token<'_>) {
        self.after_end();
        let key = self.value_key();
        Reader::number(self, key, value.bytes());
    }

    fn boolean(&mut self, _value: bool) {
        self.after_end();
        let key = self.value_key();
        if self.in_entry() {
            self.wrong(key);
        }
    }

    fn null(&mut self) {
        // A null reads as missing in `ArduinoJson` (`| ""`, `isNull()`), over an earlier value of
        // the same key.
        self.after_end();
        let key = self.value_key();
        self.missing(key);
    }

    fn object_start(&mut self) {
        self.after_end();
        let key = self.value_key();
        if self.in_entry() {
            self.wrong(key);
        }
        self.depth += 1;
        if self.credentials.is_some_and(|d| self.depth == d + 1) {
            self.entry = Some(Entry::default());
        }
    }

    fn object_end(&mut self) {
        if self.credentials.is_some_and(|d| self.depth == d + 1) {
            self.finish();
        }
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0 && !self.trailing {
            self.whole = true;
        }
    }

    fn array_start(&mut self) {
        self.after_end();
        let key = self.value_key();
        if self.in_entry() {
            self.wrong(key);
        }
        self.depth += 1;
        if key == Key::Credentials && self.depth == 2 && self.credentials.is_none() {
            // A later `credentials` replaces an earlier one, as `ArduinoJson` keeps the last.
            self.credentials = Some(self.depth);
            self.networks = [None; NETWORKS_MOST];
            self.count = 0;
        }
    }

    fn array_end(&mut self) {
        if self.credentials == Some(self.depth) {
            self.credentials = None;
        }
        self.depth = self.depth.saturating_sub(1);
    }
}
