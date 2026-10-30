//! The bare-metal firmware's services: the loops that run the application (`purestwhite-app`)
//! on devices, connected by `embassy-sync` channels. Each loop owns its part and is handed only
//! the ends of the channels it uses (`Links::app_links`, ...):
//!
//! - [`run_app`]: events in; views to draw and the sleep signal out;
//! - [`run_draw`]: draws the latest view into a free frame (`purestwhite-render`);
//! - [`run_panel`]: shows each drawn frame on a [`Screen`], hands the frame back, and tells the
//!   application whether the panel showed it; when asked to rest, puts the panel to sleep;
//! - [`run_timers`]: fires the application's timers on a [`Clock`];
//! - [`run_storage`]: does the application's jobs on the card, and reads pages into the page
//!   buffer the drawing loop copies them from.
//!
//! The loops are generic over the devices and over the channels' mutex, so the firmware can run
//! them in Embassy tasks over the X4 Pro's drivers (step 2), and the host simulator and the tests
//! over simulated ones. Two frames go round between drawing and the panel: one is drawn while
//! the other is shown; when both are in use, drawing waits for one to come back.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod app;
mod draw;
pub mod pages;
mod panel;
mod storage;
#[cfg(test)]
mod tests;
mod timers;
mod views;

pub use app::{AppLinks, run_app};
pub use draw::{DrawLinks, run_draw};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::signal::Signal;
pub use panel::{PanelLinks, Screen, ScreenError, run_panel};
use purestwhite_app::{Event, Refresh};
use purestwhite_render::FRAME_BYTES;
pub use storage::{Jobs, PageSlot, StorageLinks, run_storage};
pub use timers::{Clock, EmbassyClock, TimerLinks, Timers, run_timers};
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
    /// From the application to the timer loop.
    pub timers: Timers<M>,
    /// From the application to the storage loop.
    pub jobs: Jobs<M>,
    /// The page the storage loop reads and drawing copies (`give_page` puts the buffer here).
    pub page: PageSlot<M>,
    /// From drawing to the panel.
    pub drawn: Channel<M, Drawn, FRAMES>,
    /// From the panel back to drawing (`give_frames` puts the frames here first).
    pub empty: Channel<M, Frame, FRAMES>,
    /// Set by the application when the device is to sleep. One loop listens (power), and asks
    /// each service in turn to quiesce.
    pub sleep: Signal<M, ()>,
    /// Set by the power loop: the panel is to rest (`run_panel`).
    pub panel_rest: Signal<M, ()>,
    /// Set by the power loop: the key loop is to rest (it is the firmware's: it reads the pins).
    pub keys_rest: Signal<M, ()>,
}

impl<M: RawMutex> Links<M> {
    #[must_use]
    pub const fn new() -> Self {
        Links {
            events: Channel::new(),
            views: Views::new(),
            timers: Timers::new(),
            jobs: Jobs::new(),
            page: PageSlot::new(),
            drawn: Channel::new(),
            empty: Channel::new(),
            sleep: Signal::new(),
            panel_rest: Signal::new(),
            keys_rest: Signal::new(),
        }
    }

    /// The application loop's ends.
    #[must_use]
    pub fn app_links(&self) -> AppLinks<'_, M> {
        AppLinks {
            events: self.events.dyn_receiver(),
            views: &self.views,
            timers: &self.timers,
            jobs: &self.jobs,
            sleep: &self.sleep,
        }
    }

    /// The storage loop's ends.
    #[must_use]
    pub fn storage_links(&self) -> StorageLinks<'_, M> {
        StorageLinks {
            jobs: &self.jobs,
            page: &self.page,
            events: self.events.dyn_sender(),
        }
    }

    /// The timer loop's ends.
    #[must_use]
    pub fn timer_links(&self) -> TimerLinks<'_, M> {
        TimerLinks {
            timers: &self.timers,
            events: self.events.dyn_sender(),
        }
    }

    /// The drawing loop's ends.
    #[must_use]
    pub fn draw_links(&self) -> DrawLinks<'_, M> {
        DrawLinks {
            views: &self.views,
            page: &self.page,
            empty: self.empty.dyn_receiver(),
            drawn: self.drawn.dyn_sender(),
            unshown: self.drawn.dyn_receiver(),
        }
    }

    /// The panel loop's ends.
    #[must_use]
    pub fn panel_links(&self) -> PanelLinks<'_, M> {
        PanelLinks {
            drawn: self.drawn.dyn_receiver(),
            empty: self.empty.dyn_sender(),
            events: self.events.dyn_sender(),
            rest: &self.panel_rest,
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
    /// back, every timer armed seen by the timer loop, every job answered. Meaningful between the
    /// loops' turns on one executor, where each loop runs until it waits (the host simulator and
    /// the tests).
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.events.is_empty()
            && self.views.is_empty()
            && self.empty.len() == FRAMES
            && self.timers.is_settled()
            && self.jobs.is_idle()
    }
}

impl<M: RawMutex> Default for Links<M> {
    fn default() -> Self {
        Self::new()
    }
}
