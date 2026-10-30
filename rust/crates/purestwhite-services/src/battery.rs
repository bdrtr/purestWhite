//! The gauge loop and the battery log of the battery measurement (rust/firmware/DESIGN.md, gate
//! 1): the gauge read at the start and every `GAUGE_MS`, each reading's charge sent to the
//! application, and the first `LOG` readings kept for the report, in one short line.

use crate::Clock;
use core::cell::Cell;
use core::fmt::{self, Display, Formatter};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::DynamicSender;
use purestwhite_app::Event;

/// How often the gauge is read: every five minutes.
pub const GAUGE_MS: u64 = 5 * 60 * 1000;
/// How many readings the log keeps: the start's and an hour's.
pub const LOG: usize = 13;

/// A reading of the gauge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// The state of charge, 0 to 100.
    pub percent: u8,
    pub millivolts: u16,
}

/// The battery's fuel gauge.
#[allow(
    async_fn_in_trait,
    reason = "the loops run on a single-threaded executor; no `Send` bound is wanted"
)]
pub trait Gauge {
    /// A reading; `None` when the gauge gives none now.
    async fn read(&mut self) -> Option<Reading>;
}

/// The first `LOG` readings, a failed one kept as a gap.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Readings {
    readings: [Option<Reading>; LOG],
    count: usize,
}

impl Readings {
    /// How many readings were noted, failed ones included.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    fn note(&mut self, reading: Option<Reading>) {
        if let Some(entry) = self.readings.get_mut(self.count) {
            *entry = reading;
            self.count += 1;
        }
    }
}

/// `battery: 87/4012 86/3998 - 85/3990`: percent and millivolts of each reading, `-` for one the
/// gauge did not give; at most 125 bytes for `LOG` readings.
impl Display for Readings {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        out.write_str("battery:")?;
        for reading in self.readings.iter().take(self.count) {
            match reading {
                Some(r) => write!(out, " {}/{}", r.percent, r.millivolts)?,
                None => out.write_str(" -")?,
            }
        }
        Ok(())
    }
}

/// The log the gauge loop keeps, for whoever writes the report.
pub struct BatteryLog<M: RawMutex> {
    readings: Mutex<M, Cell<Readings>>,
}

impl<M: RawMutex> BatteryLog<M> {
    #[must_use]
    pub const fn new() -> Self {
        BatteryLog {
            readings: Mutex::new(Cell::new(Readings {
                readings: [None; LOG],
                count: 0,
            })),
        }
    }

    /// The readings so far.
    #[must_use]
    pub fn readings(&self) -> Readings {
        self.readings.lock(Cell::get)
    }

    fn note(&self, reading: Option<Reading>) {
        self.readings.lock(|cell| {
            let mut readings = cell.get();
            readings.note(reading);
            cell.set(readings);
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
}

/// Reads `gauge` at the start and every `GAUGE_MS` on `clock`, on a fixed beat (a slow reading
/// does not shift the next): each reading is noted in `links.log` and its charge sent to the
/// application.
pub async fn run_gauge<M: RawMutex, G: Gauge, C: Clock>(
    gauge: &mut G,
    clock: &C,
    links: GaugeLinks<'_, M>,
) -> ! {
    let mut next = clock.now_ms();
    loop {
        clock.at(next).await;
        let reading = gauge.read().await;
        links.log.note(reading);
        if let Some(reading) = reading {
            links.events.send(Event::Battery(reading.percent)).await;
        }
        next = next.saturating_add(GAUGE_MS);
    }
}
