//! The panel's loop: each drawn frame shown, handed back to be drawn into again, and answered.

use crate::{Drawn, Frame};
use embassy_sync::channel::{DynamicReceiver, DynamicSender};
use purestwhite_app::{Event, Refresh};
use purestwhite_render::FRAME_BYTES;

/// The panel failed; its state is unknown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenError;

/// A panel that shows whole frames.
#[allow(
    async_fn_in_trait,
    reason = "the loops run on a single-threaded executor; no `Send` bound is wanted"
)]
pub trait Screen {
    /// Shows `frame` with `refresh`; returns once the panel shows it, within a time limit (a
    /// BUSY line that stays busy is an error). After an error, the next call first brings the
    /// panel back (the drivers' `begin`).
    ///
    /// # Errors
    /// The panel failed.
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError>;
}

/// Shows each frame from `drawn` on `screen`, sends it back to `empty`, then tells the
/// application through `events` whether the panel showed it.
pub async fn run_panel<S: Screen>(
    screen: &mut S,
    drawn: DynamicReceiver<'_, Drawn>,
    empty: DynamicSender<'_, Frame>,
    events: DynamicSender<'_, Event>,
) -> ! {
    loop {
        let Drawn {
            frame,
            seq,
            refresh,
        } = drawn.receive().await;
        let shown = screen.show(frame, refresh).await;
        // Never waits: there are only as many frames as room for them.
        empty.send(frame).await;
        let answer = match shown {
            Ok(()) => Event::Shown(seq),
            Err(ScreenError) => Event::ScreenFailed(seq),
        };
        events.send(answer).await;
    }
}
