//! The raw edge path (the SDK's `ingestRawEdge`, `rawFrameOf`, `rawButtonCode`): each report id
//! read against its rest frame, a button known by where a frame first differs from it.

use super::{Decoder, RELEASE_TIMEOUT_MS, RawButton};

/// How many report ids keep a rest frame.
pub(super) const RESTS: usize = 4;

/// The rest frame and the last frame of a report id.
#[derive(Clone, Copy, Default)]
pub(super) struct Rest {
    id: u8,
    /// Frames seen, capped at 2: the first one may itself be the rest state.
    frames: u8,
    /// `frame` is a frame the remote sent, not the all-zero guess.
    known: bool,
    /// The rest frame: all zero (a guess) until learned.
    frame: [u8; 8],
    prev: [u8; 8],
}

/// What raw edges read of a report: its id and up to 8 payload bytes, a keyboard-shaped
/// report's modifier byte cleared (a held Shift is not a button).
fn frame_of(data: &[u8], report_id: u8) -> (u8, [u8; 8]) {
    let (id, payload) = match data {
        [id, rest @ ..] if data.len() == 9 => (*id, rest),
        _ => (report_id, data),
    };
    let mut bytes = [0u8; 8];
    for (to, from) in bytes.iter_mut().zip(payload) {
        *to = *from;
    }
    if payload.len() >= 7 {
        bytes[0] = 0;
    }
    (id, bytes)
}

/// The button a frame holds: its first byte that differs from `rest`, as
/// value | index << 8 | id << 16; 0 for the rest frame itself.
fn button_code(id: u8, bytes: [u8; 8], rest: [u8; 8]) -> u32 {
    bytes
        .iter()
        .zip(rest.iter())
        .zip(0u32..)
        .find(|((b, r), _)| b != r)
        .map_or(0, |((b, _), i)| {
            u32::from(*b) | i << 8 | u32::from(id) << 16
        })
}

impl Decoder {
    /// The slot of `id`'s rest, made when it is new (the last slot reused when all are taken).
    fn rest_slot(&mut self, id: u8) -> usize {
        if let Some(slot) = self
            .rests
            .iter()
            .take(self.rest_count)
            .position(|r| r.id == id)
        {
            return slot;
        }
        self.rest_count = (self.rest_count + 1).min(RESTS);
        let slot = self.rest_count - 1;
        if let Some(rest) = self.rests.get_mut(slot) {
            *rest = Rest {
                id,
                ..Rest::default()
            };
        }
        slot
    }

    /// Reads a frame against its report id's rest frame and updates that rest. Returns the button
    /// the frame holds (0 for the rest itself or for an axis pad's frame) and whether the rest is
    /// a frame the remote sent.
    fn read_against_rest(&mut self, data: &[u8], report_id: u8) -> (u32, bool) {
        let (id, bytes) = frame_of(data, report_id);
        let axis_pad = self.frame_axis_pad;
        let slot = self.rest_slot(id);
        let Some(r) = self.rests.get_mut(slot) else {
            return (0, false);
        };
        let (mut adds, mut clears) = (0u8, 0u8);
        for (b, p) in bytes.iter().zip(&r.prev) {
            adds |= b & !p;
            clears |= p & !b;
        }
        if clears != 0 && adds == 0 {
            r.frame = bytes;
            r.known = true;
        }
        if adds != 0 && clears == 0 && r.frames == 1 {
            r.frame = r.prev;
            r.known = true;
        }
        r.prev = bytes;
        r.frames = (r.frames + 1).min(2);
        let code = if axis_pad {
            0
        } else {
            button_code(id, bytes, r.frame)
        };
        (code, r.known)
    }

    /// Whether the held button, read while the rest was still the all-zero guess, was really the
    /// remote idling: the rest learned since has the same value at that byte.
    fn held_was_rest(&self) -> bool {
        if self.raw_code == 0 || !self.raw_guessed {
            return false;
        }
        let [value, at, id, _] = self.raw_code.to_le_bytes();
        self.rests
            .iter()
            .take(self.rest_count)
            .find(|r| r.id == id)
            .is_some_and(|r| r.known && r.frame.get(usize::from(at)) == Some(&value))
    }

    fn push(&mut self, code: u32, pressed: bool, at_ms: u32, keep: usize, was_rest: bool) -> bool {
        let (keycode, mods) = if pressed {
            (self.frame_press_usage, self.frame_press_mods)
        } else {
            (0, 0)
        };
        let button = RawButton {
            code,
            pressed,
            keycode,
            mods,
            was_rest,
            at_ms,
        };
        self.raws.push(button, keep)
    }

    /// The raw edges of a frame that came at `now_ms`, the one before it at `last_ms`.
    pub(super) fn raw_edges(&mut self, data: &[u8], report_id: u8, now_ms: u32, last_ms: u32) {
        let (code, known) = self.read_against_rest(data, report_id);
        let mut press_tried = false;
        if code != self.raw_code {
            // Release before press, so a swap of one button for another reads in that order.
            let was_rest = self.held_was_rest();
            if self.raw_code != 0 && !self.raw_dropped {
                self.push(self.raw_code, false, now_ms, 0, was_rest);
            }
            press_tried = code != 0;
            self.raw_dropped = press_tried && !self.push(code, true, now_ms, 1, false);
            self.raw_code = code;
            self.raw_guessed = !known;
            self.raw_reports = 1;
        } else if code != 0 {
            if now_ms.wrapping_sub(last_ms) > RELEASE_TIMEOUT_MS {
                // The same frame after silence: a remote that only reports presses. The press
                // before is settled first, so every press keeps exactly one release.
                if !self.raw_dropped {
                    self.push(code, false, last_ms, 0, false);
                }
                press_tried = true;
                self.raw_dropped = !self.push(code, true, now_ms, 1, false);
                self.raw_reports = 1;
            } else {
                self.raw_reports = self.raw_reports.saturating_add(1);
            }
        }
        self.untied_key(press_tried, now_ms);
    }

    /// A key the decoder read that no byte edge carries (an axis pad's zone, a key added while
    /// another is held): one tap of its own, byte index 0xFF.
    fn untied_key(&mut self, press_tried: bool, now_ms: u32) {
        if self.frame_press_usage == 0 || press_tried {
            return;
        }
        let open = usize::from(self.raw_code != 0 && !self.raw_dropped);
        let code = 0x00FF_FF00 | u32::from(self.frame_press_usage);
        if self.push(code, true, now_ms, 1 + open, false) {
            self.push(code, false, now_ms, open, false);
        }
    }

    /// A streamed button the reports stopped for: released, dated by the last frame.
    pub(super) fn raw_silence(&mut self, now_ms: u32) {
        let silent = now_ms.wrapping_sub(self.last_report_ms) > RELEASE_TIMEOUT_MS;
        if self.raw_code != 0 && self.raw_reports > 1 && silent {
            if !self.raw_dropped {
                self.push(self.raw_code, false, self.last_report_ms, 0, false);
            }
            self.raw_dropped = false;
            self.raw_code = 0;
            self.raw_reports = 0;
        }
    }
}
