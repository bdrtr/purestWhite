//! The bare-metal firmware's services: the loops that run the application (`purestwhite-app`)
//! on devices, connected by `embassy-sync` channels. Each loop owns its part, is handed only the
//! ends of the channels it uses, and never ends:
//!
//! - [`run_app`]: events in; views to draw and the sleep signal out;
//! - [`run_draw`]: draws the latest view into a free frame (`purestwhite-render`);
//! - [`run_panel`]: shows each drawn frame on a [`Screen`], hands the frame back, and tells the
//!   application whether the panel showed it.
//!
//! The loops are generic over the devices and over the channels' mutex, so the firmware can run
//! them in Embassy tasks over the X4 Pro's drivers (step 2), and the host simulator and the tests
//! over simulated ones. Two frames go round between drawing and the panel: one is drawn while
//! the other is shown; when both are in use, drawing waits for one to come back.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

mod app;
mod draw;
mod panel;
#[cfg(test)]
mod tests;
mod views;

pub use app::run_app;
pub use draw::run_draw;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
pub use panel::{Screen, ScreenError, run_panel};
use purestwhite_app::{Event, Refresh};
use purestwhite_render::FRAME_BYTES;
pub use views::Views;

/// A frame of the panel, owned by one loop at a time.
pub type Frame = &'static mut [u8; FRAME_BYTES];

/// The frames that go round between drawing and the panel.
pub const FRAMES: usize = 2;
/// Events waiting for the application.
pub const EVENTS: usize = 8;

/// A frame drawn, to show with `refresh`; `seq` is its view's (`purestwhite_app::Show::seq`).
#[derive(Debug)]
pub struct Drawn {
    pub frame: Frame,
    pub seq: u32,
    pub refresh: Refresh,
}

/// The channels between the loops. `M` is the mutex kind: `NoopRawMutex` when every loop runs on
/// one executor and `Links` is a local or in a `StaticCell` (the simulator, the tests);
/// `CriticalSectionRawMutex` for a plain `static`, or when an interrupt or the other core sends
/// too. A binary keeps one `Links` and hands each loop its ends. The loops' futures are not
/// `Send` (the channel ends are not), so they run on a thread-mode executor.
pub struct Links<M: RawMutex> {
    /// Into the application, from the services and the panel's answers.
    pub events: Channel<M, Event, EVENTS>,
    /// From the application to drawing.
    pub views: Views<M>,
    /// From drawing to the panel.
    pub drawn: Channel<M, Drawn, FRAMES>,
    /// From the panel back to drawing (`give_frames` puts the frames here first).
    pub empty: Channel<M, Frame, FRAMES>,
    /// Set by the application when the device is to sleep. One loop listens (power), and asks
    /// each service in turn to quiesce.
    pub sleep: Signal<M, ()>,
}

impl<M: RawMutex> Links<M> {
    #[must_use]
    pub const fn new() -> Self {
        Links {
            events: Channel::new(),
            views: Views::new(),
            drawn: Channel::new(),
            empty: Channel::new(),
            sleep: Signal::new(),
        }
    }

    /// Hands the frames to drawing, once, before the loops start; false if there was no room
    /// for them all (they were handed before).
    pub fn give_frames(&self, frames: [Frame; FRAMES]) -> bool {
        frames
            .into_iter()
            .all(|frame| self.empty.try_send(frame).is_ok())
    }

    /// Whether nothing is left to handle, draw or show: no event waiting, no view, every frame
    /// back. Meaningful between the loops' turns on one executor, where each loop runs until it
    /// waits (the host simulator and the tests).
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.events.is_empty() && self.views.is_empty() && self.empty.len() == FRAMES
    }
}

impl<M: RawMutex> Default for Links<M> {
    fn default() -> Self {
        Self::new()
    }
}
