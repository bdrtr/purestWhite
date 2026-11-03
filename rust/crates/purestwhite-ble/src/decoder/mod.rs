//! A BLE HID remote's reports read as key presses and raw button edges: the SDK's report decode
//! (`BleKeyboardHost::onReportIngest`, `ingestRawEdge` and the release timeouts of `poll`, in
//! freeink-sdk/libs/network/BleKeyboardHost/src/BleKeyboardHost.cpp), ported to read the same
//! frames the same way. Its golden corpus (testdata/hid-corpus.txt) is the SDK's own output on
//! seeded scenarios (`test/ble_hid_decoder/make_corpus.py`).
//!
//! Two reads of each frame. The key path: a keyboard-shaped report's newly pressed keys, else,
//! for a consumer, vendor or compact report, its "primary" code byte on a press edge, or an axis
//! pad's zone on its release. The raw path: each report id read against its rest frame, a button
//! known by where a frame first differs from it, with its press and release.

mod raw;
mod ring;
#[cfg(test)]
mod tests;

use crate::keymap::{SpecialKey, translate};
use raw::{RESTS, Rest};
use ring::Ring;

/// A held key or streamed button is released once no report has come for longer than this
/// (remotes stream a held key, and many send no release frame).
pub const RELEASE_TIMEOUT_MS: u32 = 150;

/// A key press the key path read. Every one is a press: the key path reports no releases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyEvent {
    /// Its printable ASCII, 0 for none.
    pub ch: u8,
    /// The HID usage (or the remote's code byte, or an axis pad's zone 0x40..=0x48).
    pub keycode: u8,
    /// The HID modifier byte.
    pub mods: u8,
    pub special: SpecialKey,
}

/// A button edge the raw path read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawButton {
    /// value | byte index << 8 | report id << 16; byte index and id 0xFF for a key no byte
    /// edge carries.
    pub code: u32,
    pub pressed: bool,
    /// On a press, the key the key path read from the same frame (0: none).
    pub keycode: u8,
    pub mods: u8,
    /// On a release: the press it ends was the remote idling on a non-zero byte, read before its
    /// rest frame was known (a button-learning screen drops such a press).
    pub was_rest: bool,
    /// When the frame came (a release after silence: when the last frame did).
    pub at_ms: u32,
}

impl RawButton {
    /// The report id the button came on (0xFF for a key no byte edge carries).
    #[must_use]
    pub fn report_id(&self) -> u8 {
        let [_, _, id, _] = self.code.to_le_bytes();
        id
    }

    /// The byte of the frame that differs from the rest frame (0xFF for a key no byte edge
    /// carries).
    #[must_use]
    pub fn byte_index(&self) -> u8 {
        let [_, at, _, _] = self.code.to_le_bytes();
        at
    }

    /// That byte's value (the key's usage for a key no byte edge carries).
    #[must_use]
    pub fn value(&self) -> u8 {
        let [value, _, _, _] = self.code.to_le_bytes();
        value
    }
}

/// What the Report Map hints (the SDK's `parseReportMapHints`): a keyboard page, a consumer page,
/// and the byte the code likely sits at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapHints {
    pub keyboard: bool,
    pub consumer: bool,
    preferred: Option<usize>,
}

impl MapHints {
    /// The hints of a Report Map: any 0x05 byte followed by 0x07 (keyboard page) or 0x0C
    /// (consumer page). A byte scan, not a descriptor parse. A remote without a readable map
    /// gives `MapHints::default()`.
    #[must_use]
    pub fn of(map: &[u8]) -> Self {
        let mut hints = MapHints::default();
        for pair in map.windows(2) {
            match pair {
                [0x05, 0x07] => hints.keyboard = true,
                [0x05, 0x0C] => hints.consumer = true,
                _ => {}
            }
        }
        hints.preferred = if hints.keyboard {
            Some(2)
        } else if hints.consumer {
            Some(1)
        } else {
            None
        };
        hints
    }
}

/// One link's decoder: make a new one for each connection (take its queued events first; the SDK
/// keeps them across a link drop).
pub struct Decoder {
    hints: MapHints,
    prev_keys: [u8; 6],
    held_usage: u8,
    last_generic: u8,
    last_report_ms: u32,
    frame_press_usage: u8,
    frame_press_mods: u8,
    frame_axis_pad: bool,
    raw_code: u32,
    raw_reports: u8,
    raw_dropped: bool,
    raw_guessed: bool,
    rests: [Rest; RESTS],
    rest_count: usize,
    keys: Ring<KeyEvent, 16>,
    raws: Ring<RawButton, 17>,
}

impl Decoder {
    /// A decoder for a remote whose Report Map gave `hints`.
    #[must_use]
    pub fn new(hints: MapHints) -> Self {
        Decoder {
            hints,
            prev_keys: [0; 6],
            held_usage: 0,
            last_generic: 0,
            last_report_ms: 0,
            frame_press_usage: 0,
            frame_press_mods: 0,
            frame_axis_pad: false,
            raw_code: 0,
            raw_reports: 0,
            raw_dropped: false,
            raw_guessed: false,
            rests: [Rest::default(); RESTS],
            rest_count: 0,
            keys: Ring::new(),
            raws: Ring::new(),
        }
    }

    /// The next key press read, if any.
    pub fn pop_key(&mut self) -> Option<KeyEvent> {
        self.keys.pop()
    }

    /// The next raw button edge read, if any.
    pub fn pop_raw(&mut self) -> Option<RawButton> {
        self.raws.pop()
    }

    /// A report `data` came at `now_ms` from the characteristic whose Report Reference id is
    /// `report_id` (0 when it has none).
    pub fn ingest(&mut self, data: &[u8], report_id: u8, now_ms: u32) {
        if data.is_empty() {
            return;
        }
        let last_ms = self.last_report_ms;
        self.last_report_ms = now_ms;
        self.frame_press_usage = 0;
        self.frame_press_mods = 0;
        self.frame_axis_pad = false;
        // A 9-byte frame leads with its report id.
        let payload = match data {
            [_, rest @ ..] if data.len() == 9 => rest,
            _ => data,
        };
        let (emitted, held) = self.keyboard_keys(payload);
        let generic =
            !emitted && !held && (self.hints.consumer || !self.hints.keyboard || payload.len() < 7);
        if generic {
            self.generic_code(payload);
        } else if emitted {
            self.last_generic = 0;
        }
        self.raw_edges(data, report_id, now_ms, last_ms);
    }

    /// The release timeouts at `now_ms`: a key, a generic code or a streamed button the reports
    /// stopped for is released. `now_ms` must not be earlier than the last report's (read the
    /// clock after taking the reports): an earlier one reads as about 49 days later, a release
    /// at once.
    pub fn poll(&mut self, now_ms: u32) {
        let silent = now_ms.wrapping_sub(self.last_report_ms) > RELEASE_TIMEOUT_MS;
        if (self.held_usage != 0 || self.last_generic != 0) && silent {
            self.held_usage = 0;
            self.prev_keys = [0; 6];
            self.last_generic = 0;
        }
        self.raw_silence(now_ms);
    }

    /// A key press of `usage`, kept as the frame's raw press key when it is the first.
    fn emit(&mut self, usage: u8, mods: u8) {
        if usage == 0 {
            return;
        }
        let (ch, special) = translate(usage, mods);
        self.keys.push(
            KeyEvent {
                ch,
                keycode: usage,
                mods,
                special,
            },
            0,
        );
        if self.frame_press_usage == 0 {
            self.frame_press_usage = usage;
            self.frame_press_mods = mods;
        }
    }

    /// The keyboard path: in an 8-byte (or 7-byte compact) report, each key newly present; and
    /// remembers the keys held. Whether it emitted, and whether a key is still held.
    fn keyboard_keys(&mut self, payload: &[u8]) -> (bool, bool) {
        let (mods, keys) = match payload {
            [mods, _, keys @ ..] if payload.len() >= 8 => (*mods, keys),
            [mods, keys @ ..] if payload.len() == 7 => (*mods, keys),
            _ => return (false, false),
        };
        let mut slots = [0u8; 6];
        for (slot, key) in slots.iter_mut().zip(keys) {
            *slot = *key;
        }
        let mut emitted = false;
        for &key in &slots {
            if key != 0 && key != 0x01 && !self.prev_keys.contains(&key) {
                self.emit(key, mods);
                emitted = true;
            }
        }
        let current = slots.iter().rev().copied().find(|&k| k != 0 && k != 0x01);
        let current = current.unwrap_or(0);
        if current != self.held_usage {
            self.held_usage = current;
        }
        self.prev_keys = slots;
        (emitted, current != 0)
    }

    /// The generic path: the report's primary code (the hinted byte when it is set, else the
    /// first non-zero of up to 8), on a press edge; an axis pad's zone on its release edge.
    fn generic_code(&mut self, payload: &[u8]) {
        let (code, index) = primary_code(payload, self.hints.preferred);
        let last = self.last_generic;
        let edge = code != 0 && code != last;
        let press = edge && code & !last != 0;
        let release = edge && code & !last == 0;
        if index == 0 && payload.len() >= 5 {
            self.frame_axis_pad = true;
            if release && let Some(zones) = axis_zones(payload) {
                self.emit(0x40 + zones, 0);
            }
        } else if press {
            self.emit(code, 0);
        }
        self.last_generic = code;
    }
}

/// The primary code byte and its index (0 with the code when there is none).
fn primary_code(payload: &[u8], preferred: Option<usize>) -> (u8, usize) {
    if let Some((i, &b)) = preferred.and_then(|i| payload.get(i).map(|b| (i, b)))
        && b != 0
    {
        return (b, i);
    }
    payload
        .iter()
        .take(8)
        .enumerate()
        .find(|(_, b)| **b != 0)
        .map_or((0, 0), |(i, &b)| (b, i))
}

/// An axis pad's release frame (a status byte, then two 16-bit little-endian axes centered near
/// 2000): its zone pair, each axis low, center or high; none when both are centered.
fn axis_zones(payload: &[u8]) -> Option<u8> {
    let zone = |v: u16| -> u8 {
        if v < 1700 {
            0
        } else if v > 2300 {
            2
        } else {
            1
        }
    };
    let [_, a, b, c, d, ..] = *payload else {
        return None;
    };
    let pair = zone(u16::from_le_bytes([a, b])) * 3 + zone(u16::from_le_bytes([c, d]));
    (pair != 4).then_some(pair)
}
