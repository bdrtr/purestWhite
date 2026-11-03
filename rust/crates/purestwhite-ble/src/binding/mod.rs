//! What each button of a remote does: the C++ firmware's binding rules
//! (lib/BlePageTurner/include/BleKeyBinding.h, with `pageActionFor` from BlePageTurner.h), ported
//! to give the same answers. A button is known by its raw code (`RawButton::code`), since the
//! decoded key is too lossy for that. Each remote (by address) has one table of at most 8 slots;
//! a remote with none saved takes its profile's default, if it has one. Its golden corpus
//! (testdata/binding-corpus.txt) is the C++ rules' own output on seeded scenarios
//! (`test/ble_key_binding/make_corpus.py`).

mod hold;
mod keys;
mod table;
#[cfg(test)]
mod tests;

use core::fmt::{self, Display, Formatter};

pub use hold::HoldWait;
pub use keys::{PageKeys, usage};
pub use table::{ADDR_MOST, RemoteTable, Remotes, default_table};

/// One wait for a button to learn lasts at most this long, as the screen tells the user.
pub const WAIT_MS: u32 = 15_000;
/// A button held this long or more is a hold.
pub const HOLD_MS: u32 = 700;
/// Slots in a remote's table: 4 buttons, tap and hold.
pub const MAX_BINDINGS: usize = 8;
/// Remotes with a table of their own (the SDK keeps as many bonds).
pub const MAX_REMOTES: usize = 4;

/// What a button does. The numbers are saved (bits 28-31 of a slot) and never change; new actions
/// go last. The four moves are the remote's own; the rest are handed to the reader.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Action {
    #[default]
    None = 0,
    NextPage = 1,
    PrevPage = 2,
    NextChapter = 3,
    PrevChapter = 4,
    ReaderMenu = 5,
    SaveQuote = 6,
    Confirm = 7,
    Back = 8,
}

impl Action {
    const ALL: [Action; 9] = [
        Action::None,
        Action::NextPage,
        Action::PrevPage,
        Action::NextChapter,
        Action::PrevChapter,
        Action::ReaderMenu,
        Action::SaveQuote,
        Action::Confirm,
        Action::Back,
    ];

    /// The action numbered `number`, if there is one.
    #[must_use]
    pub fn from_number(number: u8) -> Option<Action> {
        Action::ALL.get(usize::from(number)).copied()
    }

    /// Its name in the settings file and the logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Action::None => "none",
            Action::NextPage => "next",
            Action::PrevPage => "prev",
            Action::NextChapter => "next_chapter",
            Action::PrevChapter => "prev_chapter",
            Action::ReaderMenu => "reader_menu",
            Action::SaveQuote => "save_quote",
            Action::Confirm => "confirm",
            Action::Back => "back",
        }
    }
}

const CODE_MASK: u32 = 0x00FF_FFFF;
const HOLD_BIT: u32 = 1 << 24;
/// Any report id: the built-in defaults only, for remotes whose report id has not been measured.
const ANY_REPORT_BIT: u32 = 1 << 25;
const ACTION_SHIFT: u32 = 28;
const ACTION_MASK: u32 = 0xF000_0000;

/// One slot of a table: bits 0-23 the button's code (`RawButton::code`: value | byte index << 8 |
/// report id << 16), bit 24 hold, bit 25 any report id, bits 28-31 the action.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Binding(u32);

impl Binding {
    /// `code` (its low 24 bits), tapped or held, doing `action`.
    #[must_use]
    pub const fn new(code: u32, hold: bool, action: Action) -> Binding {
        let hold = if hold { HOLD_BIT } else { 0 };
        Binding((code & CODE_MASK) | hold | (action as u32) << ACTION_SHIFT)
    }

    /// A slot as saved, kept only when every bit means something: an action in range (not
    /// `None`), a non-zero value, no unknown bit. Garbage is dropped rather than turned into a
    /// button that does something odd.
    #[must_use]
    pub fn from_bits(bits: u32) -> Option<Binding> {
        let known = CODE_MASK | HOLD_BIT | ANY_REPORT_BIT | ACTION_MASK;
        let action = u8::try_from(bits >> ACTION_SHIFT)
            .ok()
            .and_then(Action::from_number);
        let fits = bits & !known == 0 && bits & 0xFF != 0;
        (fits && action.is_some_and(|a| a != Action::None)).then_some(Binding(bits))
    }

    /// The slot as saved.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// The same slot for any report id.
    #[must_use]
    pub(crate) const fn any_report(self) -> Binding {
        Binding(self.0 | ANY_REPORT_BIT)
    }

    #[must_use]
    pub fn action(self) -> Action {
        u8::try_from(self.0 >> ACTION_SHIFT)
            .ok()
            .and_then(Action::from_number)
            .unwrap_or_default()
    }

    /// Whether `code`, tapped or held, is this slot's button and gesture. An any-report slot
    /// ignores the report id.
    #[must_use]
    pub const fn matches(self, code: u32, hold: bool) -> bool {
        let mask = if self.0 & ANY_REPORT_BIT != 0 {
            0x0100_FFFF
        } else {
            0x01FF_FFFF
        };
        let hold = if hold { HOLD_BIT } else { 0 };
        (self.0 ^ (code | hold)) & mask == 0
    }
}

/// The button as the settings screen shows it: `3:1=02` (report id 3, byte 1, value 0x02);
/// `1=02` for an any-report default; `0x43` for a button known only by the key the decoder read
/// (byte index 0xFF).
impl Display for Binding {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let [value, at, id, _] = self.0.to_le_bytes();
        if at == 0xFF {
            write!(out, "0x{value:02X}")
        } else if self.0 & ANY_REPORT_BIT != 0 {
            write!(out, "{at}={value:02X}")
        } else {
            write!(out, "{id}:{at}={value:02X}")
        }
    }
}
