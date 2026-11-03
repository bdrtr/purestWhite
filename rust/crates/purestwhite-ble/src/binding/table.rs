//! A remote's table of slots, the saved tables of every remote, and the built-in defaults.

use super::{Action, Binding, MAX_BINDINGS, MAX_REMOTES};
use crate::profiles::{PROFILES, Profile, RemoteKey, profile_index};

/// The longest address kept: "AA:BB:CC:DD:EE:FF". A longer one is cut to this.
pub const ADDR_MOST: usize = 17;

/// One remote's slots, by its address.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RemoteTable {
    addr: [u8; ADDR_MOST],
    addr_len: u8,
    count: u8,
    bindings: [Binding; MAX_BINDINGS],
}

impl RemoteTable {
    /// A table read back from the settings file: `addr` (cut to `ADDR_MOST` bytes) and the first
    /// `MAX_BINDINGS` of `slots` that `Binding::from_bits` keeps. None without an address. An
    /// empty table is kept: the user cleared it, and the built-in default must not come back.
    /// As the C++ loader does, the settings reader cuts `addr` at its first NUL and skips a slot
    /// that is not a u32 before this.
    #[must_use]
    pub fn saved(addr: &[u8], slots: impl IntoIterator<Item = u32>) -> Option<RemoteTable> {
        if addr.is_empty() {
            return None;
        }
        let mut table = RemoteTable::default();
        table.set_addr(addr);
        for binding in slots.into_iter().filter_map(Binding::from_bits) {
            if !table.push(binding) {
                break;
            }
        }
        Some(table)
    }

    /// Its address (empty for a built-in default).
    #[must_use]
    pub fn addr(&self) -> &[u8] {
        self.addr
            .get(..usize::from(self.addr_len))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn bindings(&self) -> &[Binding] {
        self.bindings
            .get(..usize::from(self.count))
            .unwrap_or_default()
    }

    /// Whether the table changes how the remote's buttons are read: it has a slot. A table the
    /// user cleared keeps its address, so the default does not come back, and takes the decoded
    /// key path.
    #[must_use]
    pub fn routes(&self) -> bool {
        self.count > 0
    }

    /// What `code`, tapped or held, does: its first matching slot's action.
    #[must_use]
    pub fn lookup(&self, code: u32, hold: bool) -> Action {
        self.bindings()
            .iter()
            .find(|b| b.matches(code, hold))
            .map_or(Action::None, |b| b.action())
    }

    /// The first slot doing `action`.
    #[must_use]
    pub fn find(&self, action: Action) -> Option<Binding> {
        self.bindings()
            .iter()
            .find(|b| b.action() == action)
            .copied()
    }

    /// Drops the slots doing `action`; whether there were any.
    pub fn clear_action(&mut self, action: Action) -> bool {
        let before = self.count;
        self.retain(|b| b.action() != action);
        self.count != before
    }

    /// Stores a learned button. An action has one button and a gesture (button and tap or hold)
    /// does one thing: the old slots of both go. False when the table is full (those slots gone
    /// even so), never a silent overwrite.
    pub fn learn(&mut self, action: Action, code: u32, hold: bool) -> bool {
        self.clear_action(action);
        self.retain(|b| !b.matches(code, hold));
        self.push(Binding::new(code, hold, action))
    }

    /// Adds a slot at the end; false when the table is full.
    fn push(&mut self, binding: Binding) -> bool {
        let Some(slot) = self.bindings.get_mut(usize::from(self.count)) else {
            return false;
        };
        *slot = binding;
        self.count += 1;
        true
    }

    /// Keeps the slots `keep` takes, in their order.
    fn retain(&mut self, keep: impl Fn(Binding) -> bool) {
        let mut kept = 0;
        for at in 0..usize::from(self.count) {
            let Some(b) = self.bindings.get(at).copied() else {
                break;
            };
            if keep(b) {
                if let Some(slot) = self.bindings.get_mut(kept) {
                    *slot = b;
                }
                kept += 1;
            }
        }
        self.count = u8::try_from(kept).unwrap_or(0);
    }

    /// Whether this is the table of `addr`.
    fn is_of(&self, addr: &[u8]) -> bool {
        self.addr() == addr
    }

    fn set_addr(&mut self, addr: &[u8]) {
        let len = addr.len().min(ADDR_MOST);
        self.addr = [0; ADDR_MOST];
        for (to, from) in self.addr.iter_mut().zip(addr) {
            *to = *from;
        }
        self.addr_len = u8::try_from(len).unwrap_or(0);
    }
}

/// The action of a profile's logical key.
const fn action_of(key: RemoteKey) -> Action {
    match key {
        RemoteKey::Right => Action::NextPage,
        RemoteKey::Left => Action::PrevPage,
        RemoteKey::PageDown => Action::NextChapter,
        RemoteKey::PageUp => Action::PrevChapter,
    }
}

#[allow(
    clippy::indexing_slicing,
    reason = "evaluated at compile time: an index out of range fails the build"
)]
const fn default_of(profile: &Profile) -> RemoteTable {
    let mut table = RemoteTable {
        addr: [0; ADDR_MOST],
        addr_len: 0,
        count: 0,
        bindings: [Binding(0); MAX_BINDINGS],
    };
    let mut at = 0;
    while at < profile.buttons.len() {
        let button = profile.buttons[at];
        let binding = Binding::new(button.code, false, action_of(button.key));
        table.bindings[at] = if button.any_report {
            binding.any_report()
        } else {
            binding
        };
        at += 1;
    }
    #[allow(clippy::cast_possible_truncation, reason = "at most MAX_BINDINGS")]
    {
        table.count = at as u8;
    }
    table
}

/// The built-in tables, one per profile (a profile added fails the build here until it is listed).
static DEFAULTS: [RemoteTable; PROFILES.len()] =
    [default_of(&PROFILES[0]), default_of(&PROFILES[1])];

#[cfg(test)]
pub(super) fn default_tables() -> &'static [RemoteTable] {
    &DEFAULTS
}

/// The built-in table of a remote called `name`, if its profile has one.
#[must_use]
pub fn default_table(name: &[u8]) -> Option<&'static RemoteTable> {
    DEFAULTS.get(profile_index(name)?)
}

/// The saved tables, one per remote, at most `MAX_REMOTES`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Remotes {
    tables: [RemoteTable; MAX_REMOTES],
    count: u8,
}

impl Remotes {
    #[must_use]
    pub fn tables(&self) -> &[RemoteTable] {
        self.tables
            .get(..usize::from(self.count))
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(super) fn table_mut(&mut self, at: usize) -> &mut RemoteTable {
        let count = usize::from(self.count);
        self.tables
            .get_mut(..count)
            .and_then(|t| t.get_mut(at))
            .expect("a saved table")
    }

    /// Adds a table read back from the settings file (made by `RemoteTable::saved`: one without
    /// an address is never found), after the others; false when `MAX_REMOTES` remotes already
    /// have one.
    pub fn push_saved(&mut self, table: RemoteTable) -> bool {
        let Some(slot) = self.tables.get_mut(usize::from(self.count)) else {
            return false;
        };
        *slot = table;
        self.count += 1;
        true
    }

    /// The table of the connected remote: the saved one for its address, else the default for
    /// its name, else none (the decoded key path). None without an address.
    #[must_use]
    pub fn table_for(&self, addr: &[u8], name: &[u8]) -> Option<&RemoteTable> {
        if addr.is_empty() {
            return None;
        }
        match self.tables().iter().find(|t| t.is_of(addr)) {
            Some(saved) => Some(saved),
            None => default_table(name),
        }
    }

    /// The table of a remote to edit: the saved one, or a new one copied from its default (its
    /// address cut to `ADDR_MOST` bytes). None without an address, or when `MAX_REMOTES` remotes
    /// already have one.
    pub fn editable(&mut self, addr: &[u8], name: &[u8]) -> Option<&mut RemoteTable> {
        if addr.is_empty() {
            return None;
        }
        let count = usize::from(self.count);
        let at = match self.tables().iter().position(|t| t.is_of(addr)) {
            Some(at) => at,
            None if count < MAX_REMOTES => {
                let mut table = default_table(name).copied().unwrap_or_default();
                table.set_addr(addr);
                *self.tables.get_mut(count)? = table;
                self.count += 1;
                count
            }
            None => return None,
        };
        self.tables.get_mut(at)
    }

    /// Forgets a remote's table (a forgotten device forgets its buttons too); whether it had one.
    pub fn forget(&mut self, addr: &[u8]) -> bool {
        let Some(at) = self.tables().iter().position(|t| t.is_of(addr)) else {
            return false;
        };
        let count = usize::from(self.count);
        if let Some(moved) = self.tables.get_mut(at..count) {
            moved.rotate_left(1);
        }
        self.count -= 1;
        true
    }
}
