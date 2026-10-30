//! The bare-metal firmware's application: what the device shows and does, as a state machine. It
//! takes events (keys, Power held, the battery, the time, the panel's answers, its timers) and
//! answers with commands (show a view, arm a timer, sleep). It touches no hardware, does not wait,
//! does not read a clock and does not allocate, so the same code runs on the device, in the host
//! simulator and in tests. The service loops around it own the devices and the time: they turn
//! what they read into events, and carry out the commands.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod app;
mod armings;
mod cadence;
mod commands;
#[cfg(test)]
mod tests;
pub mod view;

pub use app::pages::{LOOP_PAGES, PAGE_MS};
pub use app::{App, IDLE_MS};
pub use commands::Commands;
use view::{Time, View};

/// A key of the X4 Pro: Left (the SDK's Up), Right (its Down), Power, and the touch Home key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Left,
    Right,
    Power,
    Home,
}

/// What happened, as the services report it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// A key pressed, debounced.
    Pressed(Key),
    /// Power held long enough to sleep (`purestwhite_buttons::PowerHold`).
    SleepRequested,
    /// The battery's charge, in percent.
    Battery(u8),
    /// The time of day.
    Time(Time),
    /// The panel shows the view with this `Show::seq`, and so every earlier one is done.
    Shown(u32),
    /// The panel failed to show the view with this `Show::seq`; its state is unknown.
    ScreenFailed(u32),
    /// The timer armed with this `Arm::id` fired.
    Fired(Timer, u32),
    /// The card holds this many pages (`Job::CountPages`).
    Pages(u16),
    /// The page is read, ready to draw (`Job::LoadPage`).
    PageLoaded(u16),
    /// The page could not be read.
    PageFailed(u16),
}

/// A timer of the application's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timer {
    /// No key pressed for a while: the device sleeps, as the C++ firmware's auto-sleep.
    Idle,
    /// The next page of the page loop.
    Page,
}

impl Timer {
    /// How many timers there are.
    pub const COUNT: usize = 2;

    /// The timer's place among `COUNT`.
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Timer::Idle => 0,
            Timer::Page => 1,
        }
    }
}

/// Work on the card, done by the storage loop, which answers with an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Job {
    /// How many pages the card holds: `Event::Pages`.
    CountPages,
    /// Read page n (from 1) for drawing: `Event::PageLoaded` or `Event::PageFailed`.
    LoadPage(u16),
}

/// A timer to arm: `timer` fires `after_ms` from when the services see it, unless armed again
/// first, which replaces it. Its event names `id`, so that the application can tell a firing that
/// was already on its way when it armed the timer again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Arm {
    pub timer: Timer,
    pub after_ms: u32,
    pub id: u32,
}

/// How the panel changes to a view: `Full` flashes from white, `Half` from the new frame's
/// complement (both clear what fast refreshes left behind), `Fast` changes only the pixels that
/// differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refresh {
    Full,
    Half,
    Fast,
}

impl Refresh {
    /// The one of `self` and `other` that clears more: `Full`, then `Half`, then `Fast`. Two views
    /// shown as one keep the stronger refresh.
    #[must_use]
    pub fn stronger(self, other: Refresh) -> Refresh {
        match (self, other) {
            (Refresh::Full, _) | (_, Refresh::Full) => Refresh::Full,
            (Refresh::Half, _) | (_, Refresh::Half) => Refresh::Half,
            (Refresh::Fast, Refresh::Fast) => Refresh::Fast,
        }
    }
}

/// A view to draw and show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Show {
    /// Counts up from 0 with each show; the panel's answer names it (`Event::Shown`).
    pub seq: u32,
    pub view: View,
    pub refresh: Refresh,
}

/// What the services are asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Draw the view and show it.
    Show(Show),
    /// Arm a timer.
    Arm(Arm),
    /// Do work on the card.
    Run(Job),
    /// Put the device to sleep: the sleep screen is shown, or the panel failed to show it.
    Sleep,
}
