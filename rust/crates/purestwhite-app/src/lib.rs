//! The bare-metal firmware's application: what the device shows and does, as a state machine. It
//! takes events (keys, Power held, the battery, the time, the panel's answers) and answers with
//! commands (show a view, sleep). It touches no hardware, does not wait and does not allocate, so
//! the same code runs on the device, in the host simulator and in tests. The service loops around
//! it own the devices: they turn what they read into events, and carry out the commands.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

mod app;
mod cadence;
mod commands;
#[cfg(test)]
mod tests;
pub mod view;

pub use app::App;
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
    /// Put the device to sleep: the sleep screen is shown, or the panel failed to show it.
    Sleep,
}
