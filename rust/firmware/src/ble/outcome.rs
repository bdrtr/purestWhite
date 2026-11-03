//! What `remote` did, step by step, as its lines in the report (no name or address).

use super::reports::Turns;
use core::fmt::{self, Display, Formatter};
use embassy_time::Duration;
use purestwhite_ble::Seen;
use purestwhite_ble::decoder::MapHints;
use trouble_host::prelude::SecurityLevel;

/// The step that failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The BLE controller could not start.
    Start,
    /// The host stack failed.
    Host,
    /// The scan could not start.
    Scan,
    /// The scan heard no HID remote.
    NoRemote,
    Connect,
    ConnectTimeout,
    Pair,
    PairTimeout,
    /// The link dropped.
    Dropped,
    /// A GATT request failed, or the remote sent what GATT cannot read.
    Gatt,
    /// The remote has no HID service.
    NoHid,
    /// No input report could be subscribed to.
    NoInput,
    SetupTimeout,
}

/// How long `remote` scans for, and how long it reads the remote's reports. The scan runs with
/// no watchdog feed: keep it well inside the watchdog's 60 s.
#[derive(Clone, Copy, Debug)]
pub struct Plan {
    pub scan: Duration,
    pub listen: Duration,
}

/// The link's times, in ms, and its security.
#[derive(Clone, Copy, Debug)]
pub struct LinkNote {
    pub connect_ms: u64,
    pub pair_ms: u64,
    pub level: SecurityLevel,
}

/// The HID service as set up, and the built-in profile the remote's name picked.
#[derive(Clone, Copy, Debug)]
pub struct HidNote {
    pub inputs: usize,
    /// Subscribed to the Boot Keyboard Input, no Report being one.
    pub boot: bool,
    pub map_len: usize,
    pub hints: MapHints,
    pub profile: Option<&'static str>,
}

/// What `remote` did, up to the step that failed.
#[derive(Default)]
pub struct Outcome {
    pub seen: Seen,
    pub link: Option<LinkNote>,
    pub hid: Option<HidNote>,
    pub turns: Option<Turns>,
    pub failed: Option<Failure>,
}

/// The steps in the report: the scan's line (`purestwhite-ble`'s `Seen`), then
/// `link 420 ms, pair 1800 ms, enc` (`auth` when authenticated),
/// `hid 2 in, map 63 B, kbd 1 cons 0, BOOX Keypage`,
/// `turns 40 fr, 12 k, 24 r, next 6 prev 6 other 0`, and `failed: Pair` for the step that failed.
/// In a usual run, with the image's first lines, they fit the report's 248 bytes (a longer one
/// ends in "~").
impl Display for Outcome {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        writeln!(out, "{}", self.seen)?;
        if let Some(link) = &self.link {
            write_link(out, link)?;
        }
        if let Some(hid) = &self.hid {
            write_hid(out, hid)?;
        }
        if let Some(turns) = &self.turns {
            writeln!(out, "{turns}")?;
        }
        match self.failed {
            Some(failed) => writeln!(out, "failed: {failed:?}"),
            None => Ok(()),
        }
    }
}

fn write_link(out: &mut Formatter<'_>, link: &LinkNote) -> fmt::Result {
    let level = match link.level {
        SecurityLevel::NoEncryption => "plain",
        SecurityLevel::Encrypted => "enc",
        SecurityLevel::EncryptedAuthenticated => "auth",
    };
    let (connect, pair) = (link.connect_ms, link.pair_ms);
    writeln!(out, "link {connect} ms, pair {pair} ms, {level}")
}

fn write_hid(out: &mut Formatter<'_>, hid: &HidNote) -> fmt::Result {
    let boot = if hid.boot { " boot" } else { "" };
    let (kbd, cons) = (u8::from(hid.hints.keyboard), u8::from(hid.hints.consumer));
    let profile = hid.profile.unwrap_or("no profile");
    let (inputs, map) = (hid.inputs, hid.map_len);
    writeln!(
        out,
        "hid {inputs} in{boot}, map {map} B, kbd {kbd} cons {cons}, {profile}"
    )
}
