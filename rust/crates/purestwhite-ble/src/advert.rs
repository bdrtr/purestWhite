//! An advertisement's data (a list of AD structures: length, type, value) read for what tells a
//! HID remote: the service UUIDs it lists and its appearance.

/// The HID service's 16-bit UUID.
pub const HID_SERVICE: u16 = 0x1812;
/// The keyboard appearance (Bluetooth Assigned Numbers, the HID category's keyboard).
pub const KEYBOARD_APPEARANCE: u16 = 0x03C1;

/// AD types read here.
const UUID16_INCOMPLETE: u8 = 0x02;
const UUID16_COMPLETE: u8 = 0x03;
const UUID128_INCOMPLETE: u8 = 0x06;
const UUID128_COMPLETE: u8 = 0x07;
const NAME_SHORT: u8 = 0x08;
const NAME_COMPLETE: u8 = 0x09;
const APPEARANCE: u8 = 0x19;

/// The Bluetooth base UUID with the HID service in it, little-endian as advertised.
const HID_SERVICE_128: [u8; 16] = [
    0xFB, 0x34, 0x9B, 0x5F, 0x80, 0x00, 0x00, 0x80, 0x00, 0x10, 0x00, 0x00, 0x12, 0x18, 0x00, 0x00,
];

/// What one advertisement (or scan response) says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Advert {
    /// It lists the HID service (as a 16-bit UUID, or its 128-bit form).
    pub hid_service: bool,
    /// The appearance it gives (Bluetooth Assigned Numbers).
    pub appearance: Option<u16>,
    /// It gives a name (never kept: only whether there is one).
    pub named: bool,
}

impl Advert {
    /// Whether this is a HID remote, as the C++ firmware's SDK judges it.
    #[must_use]
    pub fn is_hid(&self) -> bool {
        self.hid_service || self.appearance == Some(KEYBOARD_APPEARANCE)
    }

    /// `data` read; a structure that runs past the end, or one of length 0 (which the spec makes
    /// the end of the data, where NimBLE steps over it), ends the reading.
    #[must_use]
    pub fn read(data: &[u8]) -> Advert {
        let mut advert = Advert::default();
        let mut rest = data;
        while let Some((&len, tail)) = rest.split_first() {
            let len = usize::from(len);
            let Some((structure, after)) = (len > 0).then(|| tail.split_at_checked(len)).flatten()
            else {
                break;
            };
            if let Some((&kind, value)) = structure.split_first() {
                advert.note(kind, value);
            }
            rest = after;
        }
        advert
    }

    fn note(&mut self, kind: u8, value: &[u8]) {
        match kind {
            UUID16_INCOMPLETE | UUID16_COMPLETE => {
                let (uuids, _) = value.as_chunks::<2>();
                self.hid_service |= uuids
                    .iter()
                    .any(|&uuid| u16::from_le_bytes(uuid) == HID_SERVICE);
            }
            UUID128_INCOMPLETE | UUID128_COMPLETE => {
                let (uuids, _) = value.as_chunks::<16>();
                self.hid_service |= uuids.contains(&HID_SERVICE_128);
            }
            // As NimBLE reads it: the first field, of exactly two bytes.
            APPEARANCE => {
                if let (&[low, high], None) = (value, self.appearance) {
                    self.appearance = Some(u16::from_le_bytes([low, high]));
                }
            }
            NAME_SHORT | NAME_COMPLETE => self.named |= !value.is_empty(),
            _ => {}
        }
    }

    /// This and `other` (an advertisement and its scan response) taken together.
    #[must_use]
    pub fn merged(self, other: Advert) -> Advert {
        Advert {
            hid_service: self.hid_service || other.hid_service,
            appearance: self.appearance.or(other.appearance),
            named: self.named || other.named,
        }
    }
}
