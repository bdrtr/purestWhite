//! The application's timers: armed by the application's loop without waiting (`Timers::arm`), and
//! fired by the timer loop (`run_timers`) as events, on a [`Clock`]: embassy-time's on the device,
//! the script's in the host simulator.

use core::cell::Cell;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::DynamicSender;
use embassy_sync::signal::Signal;
use embassy_time::Instant;
use purestwhite_app::{Arm, Event, Timer};

/// The time the timer loop keeps.
#[allow(
    async_fn_in_trait,
    reason = "the loops run on a single-threaded executor; no `Send` bound is wanted"
)]
pub trait Clock {
    /// Milliseconds since the start.
    fn now_ms(&self) -> u64;

    /// Returns once `now_ms` has reached `at_ms`.
    async fn at(&self, at_ms: u64);
}

/// embassy-time's clock: the device's (esp-rtos), or the PC's in tests.
pub struct EmbassyClock;

impl Clock for EmbassyClock {
    fn now_ms(&self) -> u64 {
        Instant::now().as_millis()
    }

    async fn at(&self, at_ms: u64) {
        embassy_time::Timer::at(Instant::from_millis(at_ms)).await;
    }
}

/// How many timers there are: one per `Timer`.
const TIMERS: usize = 1;

/// A timer armed: when it fires, once the timer loop has seen it.
#[derive(Clone, Copy)]
struct Armed {
    arm: Arm,
    /// `None` until the timer loop has seen it: then `now_ms + arm.after_ms`.
    due: Option<u64>,
}

/// Each timer's latest arming. Arming a timer again replaces it.
pub struct Timers<M: RawMutex> {
    armed: Mutex<M, Cell<[Option<Armed>; TIMERS]>>,
    changed: Signal<M, ()>,
}

/// Where `timer`'s arming is kept.
fn slot(timer: Timer) -> usize {
    match timer {
        Timer::Idle => 0,
    }
}

impl<M: RawMutex> Timers<M> {
    #[must_use]
    pub const fn new() -> Self {
        Timers {
            armed: Mutex::new(Cell::new([None; TIMERS])),
            changed: Signal::new(),
        }
    }

    /// Arms `arm.timer`, over its earlier arming; never waits.
    pub fn arm(&self, arm: Arm) {
        self.update(|armed| {
            if let Some(entry) = armed.get_mut(slot(arm.timer)) {
                *entry = Some(Armed { arm, due: None });
            }
        });
        self.changed.signal(());
    }

    /// When the next timer the loop has seen fires, if any.
    #[must_use]
    pub fn next_due(&self) -> Option<u64> {
        self.armed
            .lock(|armed| armed.get().iter().flatten().filter_map(|a| a.due).min())
    }

    /// Whether the loop has seen every arming (between the loops' turns on one executor).
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.armed
            .lock(|armed| armed.get().iter().flatten().all(|a| a.due.is_some()))
    }

    /// Gives each arming not yet seen its time, from `now`; the next to fire, if any.
    fn schedule(&self, now: u64) -> Option<u64> {
        self.update(|armed| {
            for a in armed.iter_mut().flatten() {
                a.due
                    .get_or_insert(now.saturating_add(u64::from(a.arm.after_ms)));
            }
        });
        self.next_due()
    }

    /// The arming that fires first at `now`, taken off.
    fn take_due(&self, now: u64) -> Option<Arm> {
        let mut taken = None;
        self.update(|armed| {
            let first = armed
                .iter_mut()
                .filter(|a| a.is_some_and(|a| a.due.is_some_and(|due| due <= now)))
                .min_by_key(|a| a.and_then(|a| a.due));
            taken = first.and_then(Option::take).map(|a| a.arm);
        });
        taken
    }

    fn update(&self, change: impl FnOnce(&mut [Option<Armed>; TIMERS])) {
        self.armed.lock(|cell| {
            let mut armed = cell.get();
            change(&mut armed);
            cell.set(armed);
        });
    }
}

impl<M: RawMutex> Default for Timers<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// The ends of the channels the timer loop uses.
pub struct TimerLinks<'a, M: RawMutex> {
    pub timers: &'a Timers<M>,
    /// The firings, to the application.
    pub events: DynamicSender<'a, Event>,
}

/// Fires the timers armed in `links.timers` on `clock`, each as `Event::Fired` with its arming's
/// id. A timer armed again before it fires is fired only for the new arming.
pub async fn run_timers<M: RawMutex, C: Clock>(clock: &C, links: TimerLinks<'_, M>) -> ! {
    loop {
        let next = links.timers.schedule(clock.now_ms());
        let due = async {
            match next {
                Some(at) => clock.at(at).await,
                None => core::future::pending().await,
            }
        };
        if let Either::Second(()) = select(links.timers.changed.wait(), due).await {
            while let Some(arm) = links.timers.take_due(clock.now_ms()) {
                links.events.send(Event::Fired(arm.timer, arm.id)).await;
            }
        }
    }
}
