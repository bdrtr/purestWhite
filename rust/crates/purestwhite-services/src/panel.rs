//! The panel's loop: each drawn frame shown, handed back to be drawn into again, and answered;
//! until the panel is asked to rest, when it is put to sleep and the loop ends.

use crate::{Drawn, Frame};
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::{DynamicReceiver, DynamicSender};
use embassy_sync::signal::Signal;
use embassy_time::{Duration, with_timeout};
use purestwhite_app::{Event, Refresh};
use purestwhite_render::FRAME_BYTES;

/// The panel failed, or took longer than its time; its state is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenError;

/// A panel that shows whole frames.
#[allow(
    async_fn_in_trait,
    reason = "the loops run on a single-threaded executor; no `Send` bound is wanted"
)]
pub trait Screen {
    /// Shows `frame` with `refresh`; returns once the panel shows it. After an error, or a call
    /// the loop gave up on, the next call first brings the panel back (the drivers' `begin`).
    ///
    /// # Errors
    /// The panel failed.
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError>;

    /// Puts the panel into its deep sleep; it keeps showing the last frame.
    ///
    /// # Errors
    /// The panel failed.
    async fn sleep(&mut self) -> Result<(), ScreenError>;
}

/// The ends of the channels the panel's loop uses.
pub struct PanelLinks<'a, M: RawMutex> {
    /// Frames to show.
    pub drawn: DynamicReceiver<'a, Drawn>,
    /// Where each frame goes back once shown.
    pub empty: DynamicSender<'a, Frame>,
    /// The panel's answers, to the application.
    pub events: DynamicSender<'a, Event>,
    /// Set when the panel is to rest before the device sleeps.
    pub rest: &'a Signal<M, ()>,
}

/// Shows each frame from `links.drawn` on `screen`, sends it back to `links.empty`, then tells
/// the application through `links.events` whether the panel showed it. A call to the panel that
/// takes longer than `limit` is given up as failed, so that a stuck panel cannot hold back the
/// sleep, which waits for the panel's answer. Once `links.rest` is set, puts the panel to sleep
/// (within `limit` too) and returns whether it went.
///
/// # Errors
/// The panel did not go to sleep.
pub async fn run_panel<M: RawMutex, S: Screen>(
    screen: &mut S,
    links: PanelLinks<'_, M>,
    limit: Duration,
) -> Result<(), ScreenError> {
    loop {
        match select(links.drawn.receive(), links.rest.wait()).await {
            Either::First(Drawn {
                frame,
                seq,
                refresh,
            }) => {
                let shown = with_timeout(limit, screen.show(frame, refresh)).await;
                // Never waits: there are only as many frames as room for them.
                links.empty.send(frame).await;
                let answer = match shown {
                    Ok(Ok(())) => Event::Shown(seq),
                    Ok(Err(ScreenError)) | Err(_) => Event::ScreenFailed(seq),
                };
                links.events.send(answer).await;
            }
            Either::Second(()) => {
                return with_timeout(limit, screen.sleep())
                    .await
                    .unwrap_or(Err(ScreenError));
            }
        }
    }
}
