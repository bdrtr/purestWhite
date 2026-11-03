//! One network of the file's `credentials`, as read, and the checks `WifiCredentialStore::fromJson`
//! makes before it keeps it.

use crate::base64::{DecodeError, decode};
use crate::crc32::crc32;
use crate::text::Text;
use crate::{Network, PASSWORD_MOST, SSID_MOST};

/// A field's value as the file gave it.
#[derive(Clone, Copy, Default)]
pub(crate) enum Field<T> {
    #[default]
    Missing,
    Given(T),
    /// A string longer than its room.
    TooLong,
    /// There, but not of the kind the C++ firmware takes (a string for a number, say).
    Wrong,
}

/// Room for the base64 of `PASSWORD_MOST` bytes, and one more quad to tell a longer one.
pub(crate) const OBFUSCATED_MOST: usize = PASSWORD_MOST.div_ceil(3) * 4 + 4;

/// What the file gave for one network.
#[derive(Default)]
pub(crate) struct Entry {
    pub ssid: Field<Text<SSID_MOST>>,
    /// `password_obf`: the password masked (XOR) with the key, base64.
    pub obfuscated: Field<Text<OBFUSCATED_MOST>>,
    /// `password`: the legacy plain password.
    pub plain: Field<Text<PASSWORD_MOST>>,
    /// `password_len`.
    pub length: Field<u32>,
    /// `password_crc32`.
    pub crc: Field<u32>,
}

impl Entry {
    /// The network, when the C++ firmware would keep it (and its name fits Wi-Fi's 32 bytes): its
    /// password decoded with `key` (or the legacy plain one), of the length and CRC-32 the file
    /// gives when it gives them.
    pub(crate) fn network(&self, key: &[u8]) -> Option<Network> {
        let ssid = match self.ssid {
            Field::Given(ssid) => ssid,
            Field::Missing | Field::Wrong => Text::new(),
            Field::TooLong => return None,
        };
        let length = match self.length {
            Field::Missing => None,
            Field::Given(length) => Some(usize::try_from(length).ok()?),
            Field::TooLong | Field::Wrong => return None,
        };
        let password = self.password(key)?;
        let length_ok = length.is_none_or(|length| length == password.len());
        let crc_ok = match self.crc {
            Field::Missing => true,
            Field::Given(crc) => crc == crc32(password.as_bytes()),
            Field::TooLong | Field::Wrong => false,
        };
        (length_ok && crc_ok).then_some(Network { ssid, password })
    }

    /// The password: `password_obf` decoded and masked (XOR) with `key`; when that is missing, empty or
    /// not base64, the legacy `password`. `None` when the one taken is too long.
    fn password(&self, key: &[u8]) -> Option<Text<PASSWORD_MOST>> {
        let decoded = match self.obfuscated {
            Field::Given(text) if !text.is_empty() => decode::<PASSWORD_MOST>(text.as_bytes()),
            Field::TooLong => Err(DecodeError::TooLong),
            Field::Given(_) | Field::Missing | Field::Wrong => Err(DecodeError::Invalid),
        };
        match decoded {
            Ok(mut password) => {
                password.xor(key);
                Some(password)
            }
            Err(DecodeError::TooLong) => None,
            Err(DecodeError::Invalid) => match self.plain {
                Field::Given(plain) => Some(plain),
                Field::Missing | Field::Wrong => Some(Text::new()),
                Field::TooLong => None,
            },
        }
    }
}
