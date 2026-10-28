//! The host simulator of the bare-metal firmware: the application (`purestwhite-app`), the
//! renderer and the service loops (`purestwhite-services`) run on a PC over a simulated panel,
//! driven by a scenario script. Each frame the panel shows is kept as a [`Shot`] and can be
//! written as a PNG upright, as the device is held.
//!
//! It checks what does not depend on hardware: what the application asks for, what is drawn, the
//! loops and their channels. Timing, interrupts, sleep and the flash stay on the device and in
//! eink-qemu.
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

pub mod script;
mod shots;
#[cfg(test)]
mod tests;

use embassy_futures::block_on;
use embassy_futures::select::{Either4, select4};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use purestwhite_app::{App, Refresh};
use purestwhite_render::{FRAME_BYTES, FaceError, Text};
use purestwhite_services::{Frame, Links, Screen, ScreenError, run_app, run_draw, run_panel};
pub use script::{Script, ScriptError};
pub use shots::{clear_shots, shot_name, write_png};

/// How many turns of the loops a run may take to settle after an event.
const TURNS: usize = 10_000;

/// A frame the panel showed, and how.
#[derive(Clone, Debug)]
pub struct Shot {
    pub refresh: Refresh,
    pub frame: Vec<u8>,
}

/// What a run of a scenario gave.
#[derive(Debug, Default)]
pub struct Run {
    pub shots: Vec<Shot>,
    /// Whether the application asked to sleep.
    pub slept: bool,
}

/// Why a run failed.
#[derive(Debug)]
pub enum RunError {
    /// The font cannot be read.
    Font(FaceError),
    /// The loops did not settle after the script's event with this index (from 0), or before
    /// the first event when `None`.
    Stuck(Option<usize>),
}

/// The simulated panel: keeps every frame shown.
#[derive(Default)]
struct Panel {
    shots: Vec<Shot>,
}

impl Screen for Panel {
    #[allow(clippy::unused_async_trait_impl, reason = "simulated: shown at once")]
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError> {
        self.shots.push(Shot {
            refresh,
            frame: frame.to_vec(),
        });
        Ok(())
    }
}

/// Runs `script` with text in the font `font` (a `.cpfont`, style 0) when given. The first
/// screen is shown, then the events go in one by one, each once the loops have settled (nothing
/// left to handle, draw or show). The run ends after the last event, or as soon as the
/// application asks to sleep: later events are not sent.
///
/// # Errors
/// The font cannot be read, or the loops do not settle.
pub fn run(script: &Script, font: Option<&[u8]>) -> Result<Run, RunError> {
    let mut table = vec![0u8; 8192];
    let mut text = font
        .map(|bytes| Text::open(bytes, 0, &mut table))
        .transpose()
        .map_err(RunError::Font)?;
    let links: Links<NoopRawMutex> = Links::new();
    links.give_frames(core::array::from_fn(|_| frame()));
    let mut app = App::new();
    let mut panel = Panel::default();
    let driven = block_on(async {
        let loops = (
            run_app(
                &mut app,
                links.events.dyn_receiver(),
                &links.views,
                &links.sleep,
            ),
            run_draw(
                &links.views,
                links.empty.dyn_receiver(),
                links.drawn.dyn_sender(),
                links.drawn.dyn_receiver(),
                text.as_mut(),
            ),
            run_panel(
                &mut panel,
                links.drawn.dyn_receiver(),
                links.empty.dyn_sender(),
                links.events.dyn_sender(),
            ),
        );
        match select4(loops.0, loops.1, loops.2, drive(script, &links)).await {
            Either4::Fourth(driven) => driven,
        }
    });
    Ok(Run {
        shots: panel.shots,
        slept: driven?,
    })
}

/// A white frame, for the run.
#[allow(
    clippy::large_stack_arrays,
    reason = "48 KB on the PC's stack, for a moment, twice a run"
)]
fn frame() -> Frame {
    Box::leak(Box::new([0xFF; FRAME_BYTES]))
}

/// Lets the first screen show, then sends the script's events, settling after each; whether the
/// application asked to sleep.
async fn drive(script: &Script, links: &Links<NoopRawMutex>) -> Result<bool, RunError> {
    settle(links).await.ok_or(RunError::Stuck(None))?;
    for (index, &event) in script.events().iter().enumerate() {
        if links.sleep.signaled() {
            break;
        }
        links.events.send(event).await;
        settle(links).await.ok_or(RunError::Stuck(Some(index)))?;
    }
    Ok(links.sleep.signaled())
}

/// Waits until the loops are idle; `None` if they are not within `TURNS` turns.
async fn settle(links: &Links<NoopRawMutex>) -> Option<()> {
    for _ in 0..TURNS {
        if links.is_idle() {
            return Some(());
        }
        yield_now().await;
    }
    None
}
