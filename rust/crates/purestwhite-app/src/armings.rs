//! Which arming of each timer is the latest: a firing of an earlier one is out of date.

use crate::{Arm, Timer};

#[derive(Debug, Default)]
pub(crate) struct Armings {
    /// Each timer's latest `Arm::id`, by `Timer::index`; `None` when it is not armed.
    latest: [Option<u32>; Timer::COUNT],
    /// The next `Arm::id`.
    next: u32,
}

impl Armings {
    /// `timer` armed from now, for `after_ms`.
    pub(crate) fn arm(&mut self, timer: Timer, after_ms: u32) -> Arm {
        let id = self.next;
        self.next = id.wrapping_add(1);
        if let Some(latest) = self.latest.get_mut(timer.index()) {
            *latest = Some(id);
        }
        Arm {
            timer,
            after_ms,
            id,
        }
    }

    /// Whether a firing of `timer` with `id` is its latest arming's.
    pub(crate) fn is_latest(&self, timer: Timer, id: u32) -> bool {
        self.latest.get(timer.index()) == Some(&Some(id))
    }

    /// Makes any firing of `timer` out of date.
    pub(crate) fn disarm(&mut self, timer: Timer) {
        if let Some(latest) = self.latest.get_mut(timer.index()) {
            *latest = None;
        }
    }
}
