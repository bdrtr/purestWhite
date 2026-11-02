//! The gauge loop on a clock the test moves: a reading at the start and every five minutes, each
//! charge to the application; the log keeps only the measurement's readings, from its begin to
//! its end, in one line; and the rest between two readings.

use super::timers::TestClock;
use crate::{Clock, GAUGE_MS, Gauge, LOG, Links, Reading, run_gauge};
use core::fmt::Write as _;
use embassy_futures::block_on;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use purestwhite_app::{Event, Measure};
use std::format;
use std::string::{String, ToString};
use std::vec::Vec;

/// A gauge that gives its readings in turn, then the last again; how many times it was read.
struct FakeGauge(Vec<Reading>, usize);

impl Gauge for FakeGauge {
    #[allow(clippy::unused_async_trait_impl, reason = "a fake: ready at once")]
    async fn read(&mut self) -> Reading {
        let reading = self.0.get(self.1).or(self.0.last()).copied();
        self.1 += 1;
        reading.unwrap_or_default()
    }
}

fn reading(percent: u8, millivolts: u16) -> Reading {
    Reading {
        percent: Some(percent),
        millivolts: Some(millivolts),
    }
}

const GAP: Reading = Reading {
    percent: None,
    millivolts: None,
};

/// A step of a run: the clock moved to this minute, or the measurement's begin or end.
enum Step {
    At(u64),
    Measure(Measure),
}

/// Runs the gauge loop over `readings` through `steps`: the events sent, and the log's line.
fn run(readings: Vec<Reading>, steps: &[Step]) -> (Vec<Event>, String) {
    let clock = TestClock::default();
    let links: Links<NoopRawMutex> = Links::new();
    let mut gauge = FakeGauge(readings, 0);
    let script = async {
        let mut events = Vec::new();
        for step in steps {
            match step {
                Step::At(minute) => clock.move_to(minute * 60 * 1000).await,
                Step::Measure(measure) => {
                    links.battery.measure(*measure);
                    // The loop acts on it before the script goes on.
                    clock.move_to(clock.now_ms()).await;
                }
            }
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
        Either::First(()) => panic!("nothing asked the gauge loop to rest"),
        Either::Second(events) => (events, links.battery.readings().to_string()),
    }
}

const BEGIN: Step = Step::Measure(Measure::Begin);

fn end(shown: u16) -> Step {
    Step::Measure(Measure::End { shown })
}

#[test]
fn the_gauge_is_read_every_five_minutes_and_only_a_measurement_is_logged() {
    let readings = [reading(90, 4100), reading(89, 4090), reading(88, 4080)].into();
    let (events, line) = run(readings, &[Step::At(0), Step::At(4), Step::At(5)]);
    assert_eq!(events, [Event::Battery(90), Event::Battery(89)]);
    assert_eq!(line, "battery:", "no measurement: nothing logged");
    assert_eq!(GAUGE_MS, 5 * 60 * 1000);
}

#[test]
fn a_measurement_logs_from_its_begin_to_its_end_on_its_own_beat() {
    let readings = [
        reading(90, 4100),
        reading(87, 4012),
        reading(86, 3998),
        reading(86, 3990),
    ]
    .into();
    let steps = [
        Step::At(0),
        Step::At(2),
        BEGIN,
        Step::At(6),
        Step::At(7),
        end(10),
    ];
    let (events, line) = run(readings, &steps);
    // Read at the start, at the begin (minute 2), on the new beat (minute 7) and at the end; not
    // on the old beat (minute 5).
    assert_eq!(events.len(), 4);
    assert_eq!(line, "battery: 87% 4012 3998 3990 86%, 10 pages");
}

#[test]
fn a_part_the_gauge_does_not_give_is_a_gap_and_a_charge_alone_is_kept() {
    let readings = [
        reading(95, 4150),
        reading(80, 3900),
        GAP,
        Reading {
            percent: None,
            millivolts: Some(3895),
        },
        Reading {
            percent: Some(79),
            millivolts: None,
        },
    ]
    .into();
    // Read at the start (not logged), the begin, two beats and the end.
    let steps = [BEGIN, Step::At(5), Step::At(10), end(3)];
    let (events, line) = run(readings, &steps);
    assert_eq!(
        events,
        [Event::Battery(95), Event::Battery(80), Event::Battery(79)]
    );
    assert_eq!(line, "battery: 80% 3900 - 3895 - 79%, 3 pages");
}

#[test]
fn without_a_charge_the_line_has_none_and_one_charge_shows_twice() {
    let (_, gaps) = run([GAP].into(), &[BEGIN, Step::At(5)]);
    assert_eq!(gaps, "battery: - -");
    // The start's reading, then the begin's and the end's.
    let readings = [GAP, GAP, reading(64, 3800)].into();
    let (_, one) = run(readings, &[BEGIN, end(1)]);
    assert_eq!(one, "battery: 64% - 3800 64%, 1 pages");
}

#[test]
fn an_hours_beat_leaves_the_last_place_to_the_end_and_fits_the_report() {
    // The start's reading, then the begin's and nineteen beats', then the end's.
    let mut readings: Vec<Reading> = std::vec![reading(100, 4300)];
    readings.extend((0..20).map(|i| reading(100 - i, 4200 - u16::from(i))));
    readings.push(reading(5, 3500));
    let mut steps: Vec<Step> = std::vec![BEGIN];
    steps.extend((1..=19).map(|i| Step::At(i * 5)));
    steps.push(end(120));
    let (events, line) = run(readings, &steps);
    assert_eq!(
        events.len(),
        22,
        "every reading still goes to the application"
    );
    // The begin and eleven beats, then the end: the beats after the twelfth are not kept.
    let beats = (0..LOG - 1).fold(String::new(), |mut beats, i| {
        let _ = write!(beats, " {}", 4200 - i);
        beats
    });
    assert_eq!(line, format!("battery: 100%{beats} 3500 5%, 120 pages"));
    // The longest: full charges, the CW2017's largest millivolts, and three-digit pages.
    let longest = format!("battery: 100%{} 100%, 999 pages", " 5120".repeat(LOG));
    assert_eq!(longest.len(), 94);
    assert!(line.len() <= longest.len());
}

#[test]
fn asked_to_rest_the_loop_ends_while_it_waits_for_the_beat() {
    let clock = TestClock::default();
    let links: Links<NoopRawMutex> = Links::new();
    let mut gauge = FakeGauge([reading(90, 4100)].into(), 0);
    let script = async {
        clock.move_to(GAUGE_MS).await;
        links.gauge_rest.signal(());
        // The clock stays still: only the rest can end the loop.
        core::future::pending::<()>().await;
    };
    let rested = block_on(select(
        run_gauge(&mut gauge, &clock, links.gauge_links()),
        script,
    ));
    assert!(matches!(rested, Either::First(())), "the loop ended");
    assert_eq!(gauge.1, 2, "read at the start and at five minutes");
}
