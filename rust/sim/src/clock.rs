//! The simulator's clock: it moves only when the script waits, so a scenario of hours runs at once,
//! and the same every time.

use core::cell::Cell;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use purestwhite_services::Clock;

#[derive(Default)]
pub(crate) struct SimClock {
    now: Cell<u64>,
    /// Set when the clock moves; the timer loop is the one waiter.
    moved: Signal<NoopRawMutex, ()>,
}

impl Clock for SimClock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }

    async fn at(&self, at_ms: u64) {
        while self.now.get() < at_ms {
            self.moved.wait().await;
        }
    }
}

impl SimClock {
    /// Moves the clock on to `ms`; never back.
    pub(crate) fn move_to(&self, ms: u64) {
        self.now.set(self.now.get().max(ms));
        self.moved.signal(());
    }
}
