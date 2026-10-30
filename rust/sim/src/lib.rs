//! The host simulator of the bare-metal firmware: the application (`purestwhite-app`), the
//! renderer and the service loops (`purestwhite-services`) run on a PC over a simulated panel and
//! a clock that moves only when the scenario script waits. Each frame the panel shows is kept as a
//! [`Shot`], with the time it was shown, and can be written as a PNG upright, as the device is
//! held.
//!
//! It checks what does not depend on hardware: what the application asks for, what is drawn, its
//! timers, the loops and their channels. Real timing, interrupts, sleep and the flash stay on the
//! device and in eink-qemu.
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

pub mod bmp;
mod clock;
pub mod script;
mod shots;
#[cfg(test)]
mod tests;

use clock::SimClock;
use embassy_futures::block_on;
use embassy_futures::select::{Either3, Either4, select3, select4};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Duration;
use purestwhite_app::{App, Refresh};
use purestwhite_fat::Volume;
use purestwhite_fat::testing::{MemCard, ready};
use purestwhite_render::{FRAME_BYTES, FaceError, Text};
use purestwhite_services::{
    Clock, Frame, Links, Screen, ScreenError, run_app, run_draw, run_panel, run_storage, run_timers,
};
pub use script::{Script, ScriptError, Step};
pub use shots::{clear_shots, shot_name, write_png};

/// How many turns of the loops a run may take to settle after an event, and how many timers may
/// fire in one wait.
const TURNS: usize = 10_000;
/// How long the simulated panel may take over a call: it takes none.
const PANEL_LIMIT: Duration = Duration::from_secs(1);

/// A frame the panel showed, how, and when (by the simulator's clock).
#[derive(Clone, Debug)]
pub struct Shot {
    pub refresh: Refresh,
    pub frame: Vec<u8>,
    pub at_ms: u64,
}

/// What a run of a scenario gave.
#[derive(Debug, Default)]
pub struct Run {
    pub shots: Vec<Shot>,
    /// Whether the application asked to sleep and the panel was put to rest.
    pub slept: bool,
}

/// Why a run failed.
#[derive(Debug)]
pub enum RunError {
    /// The font cannot be read.
    Font(FaceError),
    /// The card holds no FAT volume the firmware reads.
    Card(purestwhite_fat::Error<()>),
    /// The loops did not settle after the script's step with this index (from 0), or before
    /// the first step when `None`.
    Stuck(Option<usize>),
}

/// The simulated panel: keeps every frame shown, with the time.
struct Panel<'a> {
    shots: Vec<Shot>,
    clock: &'a SimClock,
}

impl Screen for Panel<'_> {
    #[allow(clippy::unused_async_trait_impl, reason = "simulated: shown at once")]
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError> {
        self.shots.push(Shot {
            refresh,
            frame: frame.to_vec(),
            at_ms: self.clock.now_ms(),
        });
        Ok(())
    }

    #[allow(clippy::unused_async_trait_impl, reason = "simulated: asleep at once")]
    async fn sleep(&mut self) -> Result<(), ScreenError> {
        Ok(())
    }
}

/// Runs `script` with text in the font `font` (a `.cpfont`, style 0) and the card `card` (an image
/// of a FAT volume) when given. The first
/// screen is shown, then the steps are taken one by one, each once the loops have settled (nothing
/// left to handle, draw or show). The run ends after the last step, or once the application has
/// asked to sleep and the panel was put to rest, as the power loop does: later steps are not
/// taken.
///
/// # Errors
/// The font or the card cannot be read, or the loops do not settle.
pub fn run(script: &Script, font: Option<&[u8]>, card: Option<MemCard>) -> Result<Run, RunError> {
    let mut table = vec![0u8; 8192];
    let mut text = font
        .map(|bytes| Text::open(bytes, 0, &mut table))
        .transpose()
        .map_err(RunError::Font)?;
    let volume = card.map(mount).transpose()?;
    let links: Links<NoopRawMutex> = Links::new();
    links.give_frames(core::array::from_fn(|_| frame()));
    links.page.give(frame());
    let mut app = App::new();
    let clock = SimClock::default();
    let mut panel = Panel {
        shots: Vec::new(),
        clock: &clock,
    };
    let slept = block_on(async {
        match select4(
            run_app(&mut app, links.app_links()),
            run_draw(links.draw_links(), text.as_mut()),
            run_panel(&mut panel, links.panel_links(), PANEL_LIMIT),
            select3(
                run_timers(&clock, links.timer_links()),
                run_storage(volume, links.storage_links()),
                drive(script, &links, &clock),
            ),
        )
        .await
        {
            Either4::First(never)
            | Either4::Second(never)
            | Either4::Fourth(Either3::First(never) | Either3::Second(never)) => match never {},
            Either4::Third(rested) => Ok(rested.is_ok()),
            Either4::Fourth(Either3::Third(driven)) => driven.map(|()| false),
        }
    })?;
    Ok(Run {
        shots: panel.shots,
        slept,
    })
}

/// The FAT volume on `card` (its reads are ready at once).
fn mount(card: MemCard) -> Result<Volume<MemCard>, RunError> {
    ready(Volume::mount(card))
        .ok_or(RunError::Card(purestwhite_fat::Error::NoVolume))?
        .map_err(RunError::Card)
}

/// A white frame, for the run.
#[allow(
    clippy::large_stack_arrays,
    reason = "48 KB on the PC's stack, for a moment, three times a run"
)]
fn frame() -> Frame {
    Box::leak(Box::new([0xFF; FRAME_BYTES]))
}

/// Lets the first screen show, then takes the script's steps, settling after each. Once the
/// application asks to sleep, asks the panel to rest and waits for the panel loop to end the run.
async fn drive(
    script: &Script,
    links: &Links<NoopRawMutex>,
    clock: &SimClock,
) -> Result<(), RunError> {
    settle(links).await.ok_or(RunError::Stuck(None))?;
    for (index, &step) in script.steps().iter().enumerate() {
        if links.sleep.signaled() {
            break;
        }
        let settled = match step {
            Step::Event(event) => {
                links.events.send(event).await;
                settle(links).await
            }
            Step::Wait(ms) => pass(ms, links, clock).await,
        };
        settled.ok_or(RunError::Stuck(Some(index)))?;
    }
    if links.sleep.signaled() {
        links.panel_rest.signal(());
        core::future::pending::<()>().await;
    }
    Ok(())
}

/// Lets `ms` pass: the clock moves to each timer due meanwhile in turn, so that each fires at its
/// own time, and the loops settle after each; `None` if they do not, or if more than `TURNS`
/// timers fire. A timer is not followed once the application has asked to sleep.
async fn pass(ms: u64, links: &Links<NoopRawMutex>, clock: &SimClock) -> Option<()> {
    let until = clock.now_ms().saturating_add(ms);
    for _ in 0..TURNS {
        match links.timers.next_due() {
            Some(due) if due <= until && !links.sleep.signaled() => {
                clock.move_to(due);
                settle(links).await?;
            }
            _ => {
                clock.move_to(until);
                return settle(links).await;
            }
        }
    }
    None
}

/// Waits until the loops are idle, after one turn at least (a loop the clock just woke runs);
/// `None` if they are not within `TURNS` turns.
async fn settle(links: &Links<NoopRawMutex>) -> Option<()> {
    for _ in 0..TURNS {
        yield_now().await;
        if links.is_idle() {
            return Some(());
        }
    }
    None
}
