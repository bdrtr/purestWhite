//! The battery measurement's side of the image (DESIGN.md, gate 1): the gauge task, which runs the
//! services' gauge loop on the CW2017 over the shared I2C bus, and the report's shape during a
//! measurement.
//!
//! The report holds 248 bytes. Once the application begins a measurement (the page loop), the
//! report starts again with only its title and its runs line (12 and 19 bytes, 33 with
//! `recovery due`): the other lines give way to the battery line (up to 95), written again with
//! each new reading so that a run that dies still has the readings so far. The panel's and the
//! sleep's lines follow it at the sleep (about 61 and 41), so the sleep line, which the C++
//! firmware looks for, always starts within the room.

use crate::tasks::{LINKS, Mutex};
use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_sync::signal::Signal;
use embassy_time::{Duration, Timer};
use esp_hal::Async;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::i2c::master::I2c;
use esp_hal::peripherals::GPIO4;
use purestwhite_bootcount::Runs;
use purestwhite_firmware::board::I2cParts;
use purestwhite_firmware::boot::Report;
use purestwhite_firmware::gauge::DeviceGauge;
use purestwhite_firmware::i2c::{self, SUPPLY_MS};
use purestwhite_services::{EmbassyClock, Readings, run_gauge};

/// The report's line when the gauge cannot have its bus.
const NO_BUS: &str = "i2c: not set up";

/// Set once the gauge task rests (at once without a bus).
pub static GAUGE_RESTED: Signal<Mutex, ()> = Signal::new();

#[embassy_executor::task]
async fn gauge(bus: I2c<'static, Async>) {
    // The touch panel's supply was turned on just before the task was spawned.
    Timer::after(Duration::from_millis(SUPPLY_MS)).await;
    let mut gauge = DeviceGauge::new(bus);
    run_gauge(&mut gauge, &EmbassyClock, LINKS.gauge_links()).await;
    GAUGE_RESTED.signal(());
}

/// What the gauge leaves the image to hold for the run: the touch panel's supply, on for the bus
/// (active low), and the touch controller's reset, low: nothing drives the controller yet, so it
/// stays off the bus.
pub struct BusPins {
    /// Whether the bus was set up, and the gauge task spawned on it.
    pub bus: bool,
    pub touch_supply: Output<'static>,
    #[allow(dead_code, reason = "only held: the pin stays low while it is kept")]
    pub touch_reset: Output<'static>,
}

/// Turns the bus's supply on and spawns the gauge task on the bus, noting `i2c: not set up` in
/// `report` when the bus cannot be had (then there are no readings, and the power loop need not
/// wait for the task).
pub fn spawn(
    spawner: Spawner,
    parts: I2cParts,
    touch_reset: GPIO4<'static>,
    report: &mut Report,
) -> BusPins {
    let touch_supply = Output::new(parts.supply, Level::Low, OutputConfig::default());
    let touch_reset = Output::new(touch_reset, Level::Low, OutputConfig::default());
    let bus = i2c::open_bus(parts.i2c, parts.sda, parts.scl);
    let pins = BusPins {
        bus: bus.is_some(),
        touch_supply,
        touch_reset,
    };
    if let Some(bus) = bus {
        spawner.spawn(gauge(bus).unwrap());
    } else {
        let _ = writeln!(report, "{NO_BUS}");
        report.save();
        GAUGE_RESTED.signal(());
    }
    pins
}

/// The report during a measurement: its start lines, and the readings it shows.
pub struct Measurement {
    title: &'static str,
    runs: Runs,
    /// Whether the gauge has its bus: without it the start lines say so.
    bus: bool,
    /// Where the start lines end once the measurement began.
    start: Option<usize>,
    /// The readings the battery line shows.
    shown: Readings,
}

impl Measurement {
    /// The report's start lines during a measurement: `title`, `runs`, and `i2c: not set up`
    /// without the gauge's `bus`.
    pub const fn new(title: &'static str, runs: Runs, bus: bool) -> Self {
        Measurement {
            title,
            runs,
            bus,
            start: None,
            shown: Readings::new(),
        }
    }

    /// Whether a measurement began this run.
    pub fn begun() -> bool {
        LINKS.battery.begun()
    }

    /// During a measurement, the battery line written again into `report` when its readings
    /// changed (the first time, the report starts again from its start lines); saved.
    pub fn note(&mut self, report: &mut Report) {
        if !Self::begun() {
            return;
        }
        let readings = LINKS.battery.readings();
        let start = match self.start {
            Some(_) if readings == self.shown => return,
            Some(start) => start,
            None => self.start_again(report),
        };
        report.truncate(start);
        let _ = writeln!(report, "{readings}");
        report.save();
        self.shown = readings;
    }

    /// The report started again with its start lines alone: where they end.
    fn start_again(&mut self, report: &mut Report) -> usize {
        report.truncate(0);
        let _ = writeln!(report, "{}", self.title);
        let _ = writeln!(report, "{}", self.runs);
        if !self.bus {
            let _ = writeln!(report, "{NO_BUS}");
        }
        let start = report.len();
        self.start = Some(start);
        start
    }
}
