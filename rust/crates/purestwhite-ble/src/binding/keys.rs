//! The decoded key path's page turns, for a remote without a table (or a button its table does
//! not name).

use super::Action;

/// HID usages of the decoded key path. Keyboard page (0x07): Left, Right, Page Up, Page Down, Up,
/// Down, Space, Enter, Backspace. Volume Up/Down and Scan Next/Previous are on the Consumer page
/// (0x0C), as the low byte of the 16-bit usage.
pub mod usage {
    pub const LEFT: u8 = 0x50;
    pub const RIGHT: u8 = 0x4F;
    pub const PAGE_UP: u8 = 0x4B;
    pub const PAGE_DOWN: u8 = 0x4E;
    pub const UP: u8 = 0x52;
    pub const DOWN: u8 = 0x51;
    pub const SPACE: u8 = 0x2C;
    pub const ENTER: u8 = 0x28;
    pub const BACKSPACE: u8 = 0x2A;
    pub const VOLUME_UP: u8 = 0xE9;
    pub const VOLUME_DOWN: u8 = 0xEA;
    pub const SCAN_NEXT: u8 = 0xB5;
    pub const SCAN_PREV: u8 = 0xB6;
}

const PREV_KEYS: [u8; 6] = [
    usage::LEFT,
    usage::PAGE_UP,
    usage::UP,
    usage::BACKSPACE,
    usage::VOLUME_DOWN,
    usage::SCAN_PREV,
];
const NEXT_KEYS: [u8; 7] = [
    usage::RIGHT,
    usage::PAGE_DOWN,
    usage::DOWN,
    usage::SPACE,
    usage::ENTER,
    usage::VOLUME_UP,
    usage::SCAN_NEXT,
];

/// The decoded key path's settings: whether the page turner is on, and the usage learned for each
/// direction (0: none learned).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageKeys {
    pub enabled: bool,
    pub prev: u8,
    pub next: u8,
}

impl PageKeys {
    /// What a decoded key turns: `NextPage`, `PrevPage` or `None`. A learned key replaces its
    /// direction's default keys, so a wrong mapping can always be undone; with nothing learned
    /// the wide default set applies, since cheap remotes send only arrows, Space, Enter or the
    /// volume keys. Plain typing and any key with a modifier turn nothing.
    #[must_use]
    pub fn action(&self, key: u8, mods: u8) -> Action {
        if !self.enabled || key == 0 || mods != 0 {
            return Action::None;
        }
        // `key` is not 0 here, so it never matches a direction with nothing learned.
        if key == self.prev {
            return Action::PrevPage;
        }
        if key == self.next {
            return Action::NextPage;
        }
        if self.prev == 0 && PREV_KEYS.contains(&key) {
            return Action::PrevPage;
        }
        if self.next == 0 && NEXT_KEYS.contains(&key) {
            return Action::NextPage;
        }
        Action::None
    }
}
