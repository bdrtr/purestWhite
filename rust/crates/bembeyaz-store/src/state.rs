//! `CrossPointState` (`src/CrossPointState.cpp`, `/.crosspoint/state.json`): the book left open,
//! the reader's load count, and two rings of the sleep images shown last (plain and overlay), so
//! a sleep screen does not repeat one too soon.

use alloc::vec::Vec;
use bembeyaz_jsondoc::{Error, Value, items, or_bool, or_integer, or_str, parse};

/// The images each ring remembers (`SLEEP_RECENT_COUNT`).
pub const RECENT: u8 = 16;
const SLOTS: usize = RECENT as usize;

/// A ring of the images shown last: `pos` is where the next goes, `fill` how many it holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ring {
    pub images: [u16; SLOTS],
    pub pos: u8,
    pub fill: u8,
}

impl Ring {
    /// Whether `image` is among the last `count` (at most `fill`) pushed.
    #[must_use]
    pub fn is_recent(&self, image: u16, count: u8) -> bool {
        let pos = usize::from(self.pos);
        (0..usize::from(count.min(self.fill))).any(|i| {
            let slot = (pos + SLOTS - 1 - i) % SLOTS;
            self.images.get(slot) == Some(&image)
        })
    }

    /// Remembers `image` as the last shown.
    pub fn push(&mut self, image: u16) {
        if let Some(slot) = self.images.get_mut(usize::from(self.pos)) {
            *slot = image;
        }
        self.pos = self.pos.wrapping_add(1) % RECENT;
        if self.fill < RECENT {
            self.fill += 1;
        }
    }

    /// The ring of a state file: images from the array `images` (16 at most, an item that is
    /// not a `u16` read as 0), a position past the ring wrapped (or 0 without images), a fill
    /// no larger than the images read.
    fn from_json(doc: &Value, images: &[u8], pos: &[u8], fill: &[u8]) -> Ring {
        let list = items(doc.get(images));
        let count = list.len().min(SLOTS);
        let mut ring = Ring::default();
        for (slot, item) in ring.images.iter_mut().zip(list) {
            *slot = or_integer(Some(item), 0u16);
        }
        ring.pos = or_integer(doc.get(pos), 0u8);
        if ring.pos >= RECENT {
            ring.pos = if count > 0 { ring.pos % RECENT } else { 0 };
        }
        // `count` is at most 16.
        let count = u8::try_from(count).unwrap_or(RECENT);
        ring.fill = or_integer(doc.get(fill), 0u8).min(count);
        ring
    }
}

/// `CrossPointState`'s fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub open_epub_path: Vec<u8>,
    pub recent_sleep: Ring,
    pub recent_overlay_sleep: Ring,
    pub reader_activity_load_count: u8,
    pub last_sleep_from_reader: bool,
    pub show_boot_screen: bool,
}

impl Default for State {
    fn default() -> State {
        State {
            open_epub_path: Vec::new(),
            recent_sleep: Ring::default(),
            recent_overlay_sleep: Ring::default(),
            reader_activity_load_count: 0,
            last_sleep_from_reader: false,
            show_boot_screen: true,
        }
    }
}

/// Writes `state.json`'s text for `state` (`toJson`, then `serializeJson`).
#[must_use]
pub fn write(state: &State) -> Vec<u8> {
    let ring = |r: &Ring| {
        Value::Array(
            r.images
                .iter()
                .map(|i| Value::unsigned((*i).into()))
                .collect(),
        )
    };
    let small = |n: u8| Value::unsigned(n.into());
    let mut doc = Value::Object(Vec::new());
    doc.set(b"openEpubPath", Value::string(&state.open_epub_path));
    doc.set(b"recentSleepImages", ring(&state.recent_sleep));
    doc.set(b"recentSleepPos", small(state.recent_sleep.pos));
    doc.set(b"recentSleepFill", small(state.recent_sleep.fill));
    doc.set(
        b"recentOverlaySleepImages",
        ring(&state.recent_overlay_sleep),
    );
    doc.set(
        b"recentOverlaySleepPos",
        small(state.recent_overlay_sleep.pos),
    );
    doc.set(
        b"recentOverlaySleepFill",
        small(state.recent_overlay_sleep.fill),
    );
    doc.set(
        b"readerActivityLoadCount",
        small(state.reader_activity_load_count),
    );
    doc.set(
        b"lastSleepFromReader",
        Value::Bool(state.last_sleep_from_reader),
    );
    doc.set(b"showBootScreen", Value::Bool(state.show_boot_screen));
    doc.to_json()
}

/// The state a parsed `state.json` holds (`fromJson`): every field read, any of them missing or
/// of another type taking its default. A file from before the rings (the plain ring empty) whose
/// `lastSleepImage` is a `u8` other than 255 starts the plain ring with it.
fn from_json(doc: &Value) -> State {
    let mut recent_sleep = Ring::from_json(
        doc,
        b"recentSleepImages",
        b"recentSleepPos",
        b"recentSleepFill",
    );
    if recent_sleep.fill == 0 && !Value::is_null(doc.get(b"lastSleepImage")) {
        let legacy = or_integer(doc.get(b"lastSleepImage"), u8::MAX);
        if legacy != u8::MAX {
            recent_sleep.push(legacy.into());
        }
    }
    State {
        open_epub_path: or_str(doc.get(b"openEpubPath"), b"").to_vec(),
        recent_sleep,
        recent_overlay_sleep: Ring::from_json(
            doc,
            b"recentOverlaySleepImages",
            b"recentOverlaySleepPos",
            b"recentOverlaySleepFill",
        ),
        reader_activity_load_count: or_integer(doc.get(b"readerActivityLoadCount"), 0u8),
        last_sleep_from_reader: or_bool(doc.get(b"lastSleepFromReader"), false),
        show_boot_screen: or_bool(doc.get(b"showBootScreen"), true),
    }
}

/// Reads `state.json`'s text (`deserializeJson`, then `fromJson`).
///
/// # Errors
/// When the text is not JSON, with the error `deserializeJson` gives; the C++ store then keeps its
/// current values.
pub fn read(text: &[u8]) -> Result<State, Error> {
    parse(text).map(|doc| from_json(&doc))
}
