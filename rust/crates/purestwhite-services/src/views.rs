//! The view to draw next: only the latest one, so the panel never spends a refresh on a view that
//! is already out of date, and the application never waits for the panel.

use core::cell::Cell;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::signal::Signal;
use purestwhite_app::Show;

/// One view waiting to be drawn. A newer view replaces one not yet taken and keeps the stronger
/// of their refreshes, so a full refresh asked for is never lost.
pub struct Views<M: RawMutex> {
    waiting: Mutex<M, Cell<Option<Show>>>,
    put: Signal<M, ()>,
}

impl<M: RawMutex> Views<M> {
    #[must_use]
    pub const fn new() -> Self {
        Views {
            waiting: Mutex::new(Cell::new(None)),
            put: Signal::new(),
        }
    }

    /// Puts `show` in, over the one waiting if any; never waits.
    pub fn put(&self, show: Show) {
        self.waiting.lock(|waiting| {
            let merged = waiting.take().map_or(show, |older| Show {
                refresh: older.refresh.stronger(show.refresh),
                ..show
            });
            waiting.set(Some(merged));
        });
        self.put.signal(());
    }

    /// The view waiting, if any.
    pub fn try_take(&self) -> Option<Show> {
        self.waiting.lock(Cell::take)
    }

    /// Waits for a view and takes it. One loop calls this (drawing): the signal wakes one waiter.
    pub async fn take(&self) -> Show {
        loop {
            if let Some(show) = self.try_take() {
                return show;
            }
            // A signal left over from a view `try_take` already took costs one more turn.
            self.put.wait().await;
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.waiting.lock(|waiting| waiting.get().is_none())
    }
}

impl<M: RawMutex> Default for Views<M> {
    fn default() -> Self {
        Self::new()
    }
}
