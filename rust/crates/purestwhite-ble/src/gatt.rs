//! The HID service as the SDK's `setupHid` reads it (BleKeyboardHost.cpp): the UUIDs it looks
//! for, a Report Reference read for its report id, and which input report each notification came
//! from.

/// The HID service.
pub const HID_SERVICE: u16 = crate::advert::HID_SERVICE;
/// Report: one per report the remote sends (or takes).
pub const REPORT: u16 = 0x2A4D;
/// Protocol Mode: 1 is Report Protocol (full reports, with their ids), 0 Boot Protocol.
pub const PROTOCOL_MODE: u16 = 0x2A4E;
/// Boot Keyboard Input Report: the input when no Report can be subscribed to.
pub const BOOT_KEYBOARD_INPUT: u16 = 0x2A22;
/// Report Map: the HID descriptor, read for `MapHints`.
pub const REPORT_MAP: u16 = 0x2A4B;
/// Report Reference descriptor: a Report's id and type.
pub const REPORT_REFERENCE: u16 = 0x2908;
/// The GAP Device Name, read for a remote's built-in table.
pub const DEVICE_NAME: u16 = 0x2A00;
/// The longest device name kept (the C++ firmware keeps 31 bytes).
pub const NAME_MOST: usize = 31;
/// A Client Characteristic Configuration value turning notifications on.
pub const NOTIFY_ON: [u8; 2] = [1, 0];
/// Report Protocol, written to Protocol Mode.
pub const REPORT_PROTOCOL: u8 = 1;
/// The longest Report Map read (an attribute's longest value).
pub const REPORT_MAP_MOST: usize = 512;
/// Input reports whose id is kept; one past these reads as id 0, as in the SDK.
pub const INPUTS_MOST: usize = 4;

/// A Report's Report Reference value (none when it has no such descriptor) read as the SDK reads
/// it: its id (byte 0, 0 when absent), or none when its type (byte 1) is not Input (1).
#[must_use]
pub fn input_report_id(reference: Option<&[u8]>) -> Option<u8> {
    match reference {
        None | Some([]) => Some(0),
        Some([id] | [id, 1, ..]) => Some(*id),
        Some(_) => None,
    }
}

/// The input reports subscribed to: each one's value handle and report id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Inputs {
    reports: [(u16, u8); INPUTS_MOST],
    count: usize,
}

impl Inputs {
    /// Notes a subscribed input report; past `INPUTS_MOST` it is not kept (its frames read as
    /// id 0).
    pub fn add(&mut self, handle: u16, id: u8) {
        if let Some(slot) = self.reports.get_mut(self.count) {
            *slot = (handle, id);
            self.count += 1;
        }
    }

    /// The report id of a notification from `handle`: 0 for one not kept.
    #[must_use]
    pub fn report_id(&self, handle: u16) -> u8 {
        self.reports
            .get(..self.count)
            .unwrap_or_default()
            .iter()
            .find(|(h, _)| *h == handle)
            .map_or(0, |(_, id)| *id)
    }

    /// How many are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}
