//! What a linked remote's events do (the C++ runtime's `serveReader`, in
//! lib/BlePageTurner/src/Runtime.cpp): a remote with a table goes by its raw edges, a button the
//! table does not name falling back to the decoded key's page turn; a remote without one goes by
//! the decoded keys alone.

use super::{Action, HoldWait, PageKeys, RemoteTable};
use crate::decoder::{KeyEvent, RawButton};

/// One link's decisions: the table picked for it (a copy, picked once per link) and its hold
/// wait. Make a new one for each link, and when the tables may have changed.
#[derive(Clone, Debug, Default)]
pub struct Router {
    table: Option<RemoteTable>,
    keys: PageKeys,
    wait: HoldWait,
}

impl Router {
    /// The decisions for a link to a remote with `table` (`Remotes::table_for`), the decoded
    /// keys read with `keys`.
    #[must_use]
    pub fn new(table: Option<&RemoteTable>, keys: PageKeys) -> Router {
        Router {
            table: table.copied(),
            keys,
            wait: HoldWait::default(),
        }
    }

    /// Whether the remote goes by its raw edges: it has a table with a slot.
    #[must_use]
    pub fn via_table(&self) -> bool {
        self.table.as_ref().is_some_and(RemoteTable::routes)
    }

    /// What a raw edge does: nothing without a table (the decoded key decides).
    pub fn raw(&mut self, raw: &RawButton) -> Action {
        let Some(table) = self.table.as_ref().filter(|t| t.routes()) else {
            return Action::None;
        };
        let legacy = self.keys.action(raw.keycode, raw.mods);
        self.wait
            .on_raw_edge(table, raw.code, raw.pressed, raw.at_ms, legacy)
    }

    /// Called every pass after the raw edges: a button held long enough is a hold now. `now_ms`
    /// must not be earlier than the last edge's `at_ms`.
    pub fn hold(&mut self, now_ms: u32) -> Action {
        if !self.via_table() {
            return Action::None;
        }
        self.wait.poll(now_ms)
    }

    /// What a decoded key does: with a table the raw edge already decided its frame, so nothing.
    #[must_use]
    pub fn key(&self, key: &KeyEvent) -> Action {
        if self.via_table() {
            return Action::None;
        }
        self.keys.action(key.keycode, key.mods)
    }
}
