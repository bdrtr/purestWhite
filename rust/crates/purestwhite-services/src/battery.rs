//! The gauge loop and the battery log of the battery measurement (rust/firmware/DESIGN.md, gate
//! 1). The gauge is read every `GAUGE_MS` from the start, each charge sent to the application.
//! The log keeps only the measurement's readings, which the application spans with
//! `Measure::Begin` and `Measure::End`: at its begin the gauge is read at once and the beat starts
//! again from there; at its end it is read once more, and the log notes the pages shown (see
//! `Readings`). Asked to rest, the loop ends between two readings, so that a reading is never cut
//! short on the bus.

use crate::Clock;
use crate::readings::{Reading, Readings};
use core::cell::Cell;
use embassy_futures::select::{Either3, select3};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::DynamicSender;
use embassy_sync::signal::Signal;
use purestwhite_app::{Event, Measure};

/// How often the gauge is read: every five minutes.
pub const GAUGE_MS: u64 = 5 * 60 * 1000;

/// The battery's fuel gauge.
#[allow(
    async_fn_in_trait,
    reason = "the loops run on a single-threaded executor; no `Send` bound is wanted"
)]
pub trait Gauge {
    /// A reading, each part `None` when the gauge gives it not now.
    async fn read(&mut self) -> Reading;
}

/// Where the log is.
#[derive(Clone, Copy, Default)]
struct State {
    readings: Readings,
    /// A measurement began this run.
    begun: bool,
    /// The measurement runs: readings on the beat are kept.
    running: bool,
}

/// The measurement's log, kept by the gauge loop for whoever writes the report; the application's
/// loop hands it the measurement's begin and end (`run_app`).
pub struct BatteryLog<M: RawMutex> {
    state: Mutex<M, Cell<State>>,
    /// The begin or end the gauge loop has not acted on yet.
    asked: Signal<M, Measure>,
}

impl<M: RawMutex> BatteryLog<M> {
    #[must_use]
    pub const fn new() -> Self {
        BatteryLog {
            state: Mutex::new(Cell::new(State {
                readings: Readings::new(),
                begun: false,
                running: false,
            })),
            asked: Signal::new(),
        }
    }

    /// The current measurement's readings so far.
    #[must_use]
    pub fn readings(&self) -> Readings {
        self.state.lock(Cell::get).readings
    }

    /// Whether a measurement began this run.
    #[must_use]
    pub fn begun(&self) -> bool {
        self.state.lock(Cell::get).begun
    }

    /// The measurement begins (its readings start afresh) or ends; the gauge loop reads the gauge
    /// at once either way.
    pub fn measure(&self, measure: Measure) {
        self.update(|state| match measure {
            Measure::Begin => {
                *state = State {
                    readings: Readings::new(),
                    begun: true,
                    running: true,
                };
            }
            Measure::End { .. } => state.running = false,
        });
        self.asked.signal(measure);
    }

    fn note_beat(&self, reading: Reading) {
        self.update(|state| {
            if state.running {
                state.readings.note_beat(reading);
            }
        });
    }

    /// The end's reading; dropped when a new measurement began meanwhile (Left, then Right at
    /// once), whose log it would not belong to.
    fn note_end(&self, reading: Reading, pages: u16) {
        self.update(|state| {
            if state.begun && !state.running {
                state.readings.note_end(reading, pages);
            }
        });
    }

    fn update(&self, change: impl FnOnce(&mut State)) {
        self.state.lock(|cell| {
            let mut state = cell.get();
            change(&mut state);
            cell.set(state);
        });
    }
}

impl<M: RawMutex> Default for BatteryLog<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// The ends of the channels the gauge loop uses.
pub struct GaugeLinks<'a, M: RawMutex> {
    pub log: &'a BatteryLog<M>,
    /// Each reading's charge, to the application.
    pub events: DynamicSender<'a, Event>,
    /// Set by the power loop: the gauge loop is to rest.
    pub rest: &'a Signal<M, ()>,
}

/// Reads `gauge` at the start and every `GAUGE_MS` on `clock`, on a fixed beat (a slow reading
/// does not shift the next), until `links.rest` is set; at the measurement's begin and end too
/// (`BatteryLog::measure`), the begin starting the beat again. Each charge goes to the
/// application, and the measurement's readings to `links.log`. A reading under way when
/// `links.rest` is set is finished first.
pub async fn run_gauge<M: RawMutex, G: Gauge, C: Clock>(
    gauge: &mut G,
    clock: &C,
    links: GaugeLinks<'_, M>,
) {
    let mut next = clock.now_ms();
    loop {
        match select3(links.rest.wait(), links.log.asked.wait(), clock.at(next)).await {
            Either3::First(()) => return,
            Either3::Second(Measure::Begin) => next = clock.now_ms(),
            Either3::Second(Measure::End { shown }) => {
                let reading = gauge.read().await;
                links.log.note_end(reading, shown);
                send(&links, reading).await;
            }
            Either3::Third(()) => {
                let reading = gauge.read().await;
                links.log.note_beat(reading);
                send(&links, reading).await;
                next = next.saturating_add(GAUGE_MS);
            }
        }
    }
}

/// The reading's charge, if any, to the application.
async fn send<M: RawMutex>(links: &GaugeLinks<'_, M>, reading: Reading) {
    if let Some(percent) = reading.percent {
        links.events.send(Event::Battery(percent)).await;
    }
}
