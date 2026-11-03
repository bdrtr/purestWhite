//! The remote's HID service set up as the SDK's `setupHid` does (BleKeyboardHost.cpp): Report
//! Protocol asked for, the Report Map read for its hints, every input Report subscribed to (the
//! Boot Keyboard Input when none is), and its device name read for the built-in tables.

use super::Hci;
use super::outcome::Failure;
use core::ops::ControlFlow;
use purestwhite_ble::decoder::MapHints;
use purestwhite_ble::gatt::{
    BOOT_KEYBOARD_INPUT, DEVICE_NAME, HID_SERVICE, Inputs, NAME_MOST, NOTIFY_ON, PROTOCOL_MODE,
    REPORT, REPORT_MAP, REPORT_MAP_MOST, REPORT_PROTOCOL, REPORT_REFERENCE, input_report_id,
};
use trouble_host::prelude::{
    Characteristic, CharacteristicProp, DefaultPacketPool, GattClient, Uuid,
};

/// The GATT client over the link: services found, at most 4 kept (GAP, GATT, HID, battery).
pub(super) type Client<'c> = GattClient<'c, Hci, DefaultPacketPool, 4>;
/// Characteristics of the HID service read at most (a remote has 3 to 10; more fail the setup
/// as `Gatt`).
const CHARS_MOST: usize = 16;

/// The HID service as set up: the input reports subscribed to, the map's hints and its size.
pub(super) struct Hid {
    pub inputs: Inputs,
    pub hints: MapHints,
    pub map_len: usize,
    /// Subscribed to the Boot Keyboard Input, no Report being one.
    pub boot: bool,
}

type Chars = [Characteristic<[u8]>];

/// The remote's device name, up to `NAME_MOST` bytes.
pub(super) struct Name {
    bytes: [u8; NAME_MOST],
    len: usize,
}

impl Name {
    pub(super) fn as_bytes(&self) -> &[u8] {
        self.bytes.get(..self.len).unwrap_or_default()
    }
}

/// Sets the HID service up.
pub(super) async fn set_up(client: &Client<'_>) -> Result<Hid, Failure> {
    let services = client
        .services_by_uuid(&Uuid::new_short(HID_SERVICE))
        .await
        .map_err(|_| Failure::Gatt)?;
    let service = services.first().ok_or(Failure::NoHid)?;
    let chars = client
        .characteristics::<CHARS_MOST>(service)
        .await
        .map_err(|_| Failure::Gatt)?;
    if let Some(mode) = find(&chars, PROTOCOL_MODE, CharacteristicProp::Write) {
        let _ = client
            .write_characteristic_without_response(mode, &[REPORT_PROTOCOL])
            .await;
    }
    let (hints, map_len) = read_map(client, &chars).await;
    let mut hid = Hid {
        inputs: Inputs::default(),
        hints,
        map_len,
        boot: false,
    };
    let subscribed = subscribe_inputs(client, &chars, &mut hid.inputs).await;
    if !subscribed {
        let boot = find(&chars, BOOT_KEYBOARD_INPUT, CharacteristicProp::Notify);
        hid.boot = match boot {
            Some(boot) => notify_on(client, boot).await,
            None => false,
        };
        if !hid.boot {
            return Err(Failure::NoInput);
        }
    }
    Ok(hid)
}

/// The first characteristic of `uuid` with `prop`.
fn find(chars: &Chars, uuid: u16, prop: CharacteristicProp) -> Option<&Characteristic<[u8]>> {
    let uuid = Uuid::new_short(uuid);
    chars
        .iter()
        .find(|c| c.uuid == uuid && c.props.any(&[prop]))
}

/// The Report Map's hints and size; none when it cannot be read, as a remote without one.
async fn read_map(client: &Client<'_>, chars: &Chars) -> (MapHints, usize) {
    let Some(map) = find(chars, REPORT_MAP, CharacteristicProp::Read) else {
        return (MapHints::default(), 0);
    };
    let mut bytes = [0u8; REPORT_MAP_MOST];
    match client.read_characteristic(map, &mut bytes).await {
        Ok(len) => (MapHints::of(bytes.get(..len).unwrap_or_default()), len),
        Err(_) => (MapHints::default(), 0),
    }
}

/// Subscribes to every input Report (by its Report Reference), each one's id noted; whether one
/// was subscribed to.
async fn subscribe_inputs(client: &Client<'_>, chars: &Chars, inputs: &mut Inputs) -> bool {
    let report = Uuid::new_short(REPORT);
    let mut subscribed = false;
    for c in chars.iter().filter(|c| c.uuid == report) {
        if !c.props.any(&[CharacteristicProp::Notify]) {
            continue;
        }
        let mut reference = [0u8; 2];
        let read = read_reference(client, c, &mut reference).await;
        let Some(id) = input_report_id(read.map(|len| reference.get(..len).unwrap_or_default()))
        else {
            continue;
        };
        if notify_on(client, c).await {
            inputs.add(c.handle, id);
            subscribed = true;
        }
    }
    subscribed
}

/// The Report Reference of `c` into `to`: its length, or none without one.
async fn read_reference(
    client: &Client<'_>,
    c: &Characteristic<[u8]>,
    to: &mut [u8; 2],
) -> Option<usize> {
    let kind = Uuid::new_short(REPORT_REFERENCE);
    let first = c.handle.checked_add(1)?;
    let found = client
        .read_by_type(first, c.end_handle, &kind, |_, value| {
            let len = value.len().min(to.len());
            for (at, byte) in to.iter_mut().zip(value) {
                *at = *byte;
            }
            ControlFlow::Break(len)
        })
        .await;
    found.ok().flatten()
}

/// Turns `c`'s notifications on; whether it took.
async fn notify_on(client: &Client<'_>, c: &Characteristic<[u8]>) -> bool {
    match c.cccd_handle {
        Some(cccd) => client.write_handle(cccd, &NOTIFY_ON).await.is_ok(),
        None => false,
    }
}

/// The remote's device name (GAP), up to `NAME_MOST` bytes; empty when it gives none.
pub(super) async fn device_name(client: &Client<'_>) -> Name {
    let mut name = Name {
        bytes: [0; NAME_MOST],
        len: 0,
    };
    let kind = Uuid::new_short(DEVICE_NAME);
    let _ = client
        .read_by_type(1, u16::MAX, &kind, |_, value| {
            for (to, from) in name.bytes.iter_mut().zip(value) {
                *to = *from;
            }
            name.len = value.len().min(NAME_MOST);
            ControlFlow::Break(())
        })
        .await;
    name
}
