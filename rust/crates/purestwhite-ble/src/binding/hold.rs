//! The decision for each raw button edge of a remote with a table: a tap now, or a wait for a
//! tap or a hold.

use super::{Action, HOLD_MS, RemoteTable};

/// A press still waiting to become a tap or a hold: only for a button with a hold slot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldWait {
    code: u32,
    at_ms: u32,
    tap: Action,
    hold: Action,
    active: bool,
}

impl HoldWait {
    /// The one decision for a raw edge (`code`, pressed or released at `at_ms`) of a remote with
    /// `table`. `legacy` is what the decoded key path gives the key read from the same frame: a
    /// button the table does not name keeps turning pages the way it always did.
    /// - A press of a button with no hold slot acts now.
    /// - A press of a button with a hold slot waits: a release before `HOLD_MS` is a tap, later
    ///   a hold.
    /// - A release only ends a wait.
    pub fn on_raw_edge(
        &mut self,
        table: &RemoteTable,
        code: u32,
        pressed: bool,
        at_ms: u32,
        legacy: Action,
    ) -> Action {
        if pressed {
            let tap = match table.lookup(code, false) {
                Action::None => legacy,
                tapped => tapped,
            };
            let hold = table.lookup(code, true);
            if hold == Action::None {
                return tap;
            }
            *self = HoldWait {
                code,
                at_ms,
                tap,
                hold,
                active: true,
            };
            return Action::None;
        }
        if !self.active || self.code != code {
            return Action::None;
        }
        self.active = false;
        if at_ms.wrapping_sub(self.at_ms) < HOLD_MS {
            self.tap
        } else {
            self.hold
        }
    }

    /// Called every pass: a waiting button still down after `HOLD_MS` is a hold, acted on now
    /// without waiting for its release. `now_ms` must not be earlier than the last edge's
    /// `at_ms` (read the clock after taking the edges): an earlier one reads as about 49 days
    /// later, a hold at once.
    pub fn poll(&mut self, now_ms: u32) -> Action {
        if !self.active || now_ms.wrapping_sub(self.at_ms) < HOLD_MS {
            return Action::None;
        }
        self.active = false;
        self.hold
    }
}
