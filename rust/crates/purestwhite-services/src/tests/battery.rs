//! The gauge loop on a clock the test moves: a reading at the start and every five minutes, its
//! charge to the application, and the first readings in the log's one line.

use super::timers::TestClock;
use crate::{GAUGE_MS, Gauge, LOG, Links, Reading, run_gauge};
use embassy_futures::block_on;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use purestwhite_app::Event;
use std::format;
use std::string::ToString;
use std::vec::Vec;

/// A gauge that gives its readings in turn, then the last again.
struct FakeGauge(Vec<Option<Reading>>, usize);

impl Gauge for FakeGauge {
    #[allow(clippy::unused_async_trait_impl, reason = "a fake: ready at once")]
    async fn read(&mut self) -> Option<Reading> {
        let reading = self.0.get(self.1).or(self.0.last()).copied().flatten();
        self.1 += 1;
        reading
    }
}

fn reading(percent: u8, millivolts: u16) -> Reading {
    Reading {
        percent,
        millivolts,
    }
}

/// Runs the gauge loop over `readings` while the clock moves to each of `steps` (in minutes);
/// the events sent, and the log's line.
fn run(readings: Vec<Option<Reading>>, minutes: &[u64]) -> (Vec<Event>, std::string::String) {
    let clock = TestClock::default();
    let links: Links<NoopRawMutex> = Links::new();
    let mut gauge = FakeGauge(readings, 0);
    let script = async {
        let mut events = Vec::new();
        for &minute in minutes {
            clock.move_to(minute * 60 * 1000).await;
            while let Ok(event) = links.events.try_receive() {
                events.push(event);
            }
        }
        events
    };
    match block_on(select(
        run_gauge(&mut gauge, &clock, links.gauge_links()),
        script,
    )) {
        Either::First(never) => match never {},
        Either::Second(events) => (events, links.battery.readings().to_string()),
    }
}

#[test]
fn the_gauge_is_read_at_the_start_and_every_five_minutes() {
    let readings = [
        Some(reading(87, 4012)),
        Some(reading(86, 3998)),
        Some(reading(86, 3990)),
    ]
    .into();
    let (events, line) = run(readings, &[0, 4, 5, 9, 10]);
    assert_eq!(
        events,
        [Event::Battery(87), Event::Battery(86), Event::Battery(86)]
    );
    assert_eq!(line, "battery: 87/4012 86/3998 86/3990");
    assert_eq!(GAUGE_MS, 5 * 60 * 1000);
}

#[test]
fn a_reading_the_gauge_does_not_give_is_a_gap_and_no_event() {
    let readings = [Some(reading(80, 3900)), None, Some(reading(79, 3890))].into();
    let (events, line) = run(readings, &[0, 5, 10]);
    assert_eq!(events, [Event::Battery(80), Event::Battery(79)]);
    assert_eq!(line, "battery: 80/3900 - 79/3890");
}

#[test]
fn the_log_keeps_the_first_hour_and_fits_the_report() {
    let readings = [Some(reading(100, 4200))].into();
    let minutes: Vec<u64> = (0..=20).map(|i| i * 5).collect();
    let (events, line) = run(readings, &minutes);
    assert_eq!(
        events.len(),
        21,
        "every reading still goes to the application"
    );
    assert_eq!(line, format!("battery:{}", " 100/4200".repeat(LOG)));
    assert!(line.len() <= 125);
}
