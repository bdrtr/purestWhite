// The tests unwrap and index what must be there, and fail when the loops do not settle.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

mod pages;
mod timers;

use super::*;
use embassy_futures::block_on;
use embassy_futures::select::{Either, Either4, select, select4};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Duration;
use purestwhite_app::view::{Time, View};
use purestwhite_app::{App, Show};
use std::boxed::Box;
use std::vec;
use std::vec::Vec;

/// How long the loop gives the test's panel for one call.
const LIMIT: Duration = Duration::from_millis(50);

/// A screen that records each refresh and how many pixels of the frame were black, takes `slow`
/// turns of the loops for each frame, fails the calls numbered in `fail` (from 0), never finishes
/// those in `stuck`, and notes its sleep (which never finishes when `sleep_stuck`).
#[derive(Default)]
struct Recorder {
    shown: Vec<(Refresh, u32)>,
    calls: usize,
    fail: Vec<usize>,
    stuck: Vec<usize>,
    slow: usize,
    asleep: bool,
    sleep_stuck: bool,
}

impl Screen for Recorder {
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError> {
        for _ in 0..self.slow {
            yield_now().await;
        }
        let call = self.calls;
        self.calls += 1;
        if self.stuck.contains(&call) {
            core::future::pending::<()>().await;
        }
        if self.fail.contains(&call) {
            return Err(ScreenError);
        }
        let black = frame.iter().map(|b| (!b).count_ones()).sum();
        self.shown.push((refresh, black));
        Ok(())
    }

    async fn sleep(&mut self) -> Result<(), ScreenError> {
        if self.sleep_stuck {
            core::future::pending::<()>().await;
        }
        self.asleep = true;
        Ok(())
    }
}

/// What the test's script does, in order, after the first screen has settled.
#[derive(Clone, Copy)]
enum Step {
    Send(Event),
    /// Lets the loops take this many turns.
    Turns(usize),
    /// Waits until the loops are idle.
    Settle,
    /// Asks the panel to rest now, as the power loop does, and lets the run end with it.
    Rest,
}

/// `batches`, each sent at once, then settled.
fn batches(batches: &[&[Event]]) -> Vec<Step> {
    let mut steps = Vec::new();
    for batch in batches {
        steps.extend(batch.iter().map(|&event| Step::Send(event)));
        steps.push(Step::Settle);
    }
    steps
}

fn frames() -> [Frame; FRAMES] {
    core::array::from_fn(|_| {
        let frame: Box<[u8; FRAME_BYTES]> = vec![0xFF; FRAME_BYTES]
            .into_boxed_slice()
            .try_into()
            .unwrap();
        Box::leak(frame)
    })
}

/// Waits until the loops are idle; panics if they are not within seconds.
async fn settle(links: &Links<NoopRawMutex>) {
    let start = std::time::Instant::now();
    while !links.is_idle() {
        assert!(start.elapsed().as_secs() < 5, "the loops did not settle");
        yield_now().await;
    }
}

/// What a run gave.
struct Outcome {
    screen: Recorder,
    /// Whether the application asked to sleep.
    slept: bool,
    /// The panel loop's end, when it was asked to rest (after the application asked to sleep).
    rested: Option<Result<(), ScreenError>>,
}

/// Runs the loops: the first screen, then `steps`; once settled, when the application asked to
/// sleep, the panel is asked to rest, as the power loop does.
fn run(steps: &[Step], screen: Recorder) -> Outcome {
    let links: Links<NoopRawMutex> = Links::new();
    assert!(links.give_frames(frames()));
    let mut app = App::new();
    let mut screen = screen;
    let script = async {
        settle(&links).await;
        for &step in steps {
            match step {
                Step::Send(event) => links.events.try_send(event).unwrap(),
                Step::Turns(turns) => {
                    for _ in 0..turns {
                        yield_now().await;
                    }
                }
                Step::Settle => settle(&links).await,
                Step::Rest => {
                    links.panel_rest.signal(());
                    core::future::pending::<()>().await;
                }
            }
        }
        settle(&links).await;
        if links.sleep.signaled() {
            links.panel_rest.signal(());
            core::future::pending::<()>().await;
        }
    };
    let rested = block_on(async {
        match select4(
            run_app(&mut app, links.app_links()),
            run_draw(links.draw_links(), None),
            run_panel(&mut screen, links.panel_links(), LIMIT),
            select(script, run_timers(&EmbassyClock, links.timer_links())),
        )
        .await
        {
            Either4::First(never)
            | Either4::Second(never)
            | Either4::Fourth(Either::Second(never)) => match never {},
            Either4::Third(rested) => Some(rested),
            Either4::Fourth(Either::First(())) => None,
        }
    });
    Outcome {
        screen,
        slept: links.sleep.signaled(),
        rested,
    }
}

fn refreshes(screen: &Recorder) -> Vec<Refresh> {
    screen.shown.iter().map(|s| s.0).collect()
}

#[test]
fn home_then_its_updates_then_the_sleep_screen_then_sleep() {
    let time = Time { hour: 7, minute: 5 };
    let Outcome {
        screen,
        slept,
        rested,
    } = run(
        &batches(&[
            &[Event::Battery(80)],
            &[Event::Battery(80)],
            &[Event::Time(time)],
            &[Event::SleepRequested],
            &[Event::Battery(10)],
        ]),
        Recorder::default(),
    );
    // Home, home with the battery, home with the time (the same battery drew nothing), sleep.
    assert_eq!(
        refreshes(&screen),
        [Refresh::Full, Refresh::Fast, Refresh::Fast, Refresh::Full]
    );
    assert!(slept);
    assert_eq!(rested, Some(Ok(())));
    assert!(screen.asleep);
    // The battery added ink; the sleep screen's moon is the most ink of all.
    let ink: Vec<_> = screen.shown.iter().map(|s| s.1).collect();
    assert!(ink[1] > ink[0]);
    assert!(ink[3] > ink[1]);
}

#[test]
fn views_that_come_faster_than_the_panel_are_drawn_once() {
    // Three changes before drawing runs again: one frame for them all.
    let Outcome {
        screen,
        slept,
        rested,
    } = run(
        &batches(&[&[Event::Battery(10), Event::Battery(20), Event::Battery(30)]]),
        Recorder::default(),
    );
    assert!(!slept);
    assert_eq!(rested, None);
    assert_eq!(refreshes(&screen), [Refresh::Full, Refresh::Fast]);
}

#[test]
fn a_frame_the_panel_has_not_taken_yet_is_drawn_again_with_the_newer_view() {
    // While the slow panel shows the first change, two more come one after the other: the second
    // was drawn into the frame waiting for the panel, and the third is drawn over it.
    let slow = Recorder {
        slow: 20,
        ..Recorder::default()
    };
    let steps = [
        Step::Send(Event::Battery(10)),
        Step::Turns(1),
        Step::Send(Event::Battery(20)),
        Step::Turns(1),
        Step::Send(Event::Battery(30)),
    ];
    let screen = run(&steps, slow).screen;
    assert_eq!(
        refreshes(&screen),
        [Refresh::Full, Refresh::Fast, Refresh::Fast]
    );
    let direct = run(&batches(&[&[Event::Battery(30)]]), Recorder::default()).screen;
    assert_eq!(screen.shown[2].1, direct.shown[1].1);
}

#[test]
fn a_failed_frame_is_answered_and_shown_again_and_sleep_still_comes() {
    let Outcome {
        screen,
        slept,
        rested,
    } = run(
        &batches(&[&[Event::Battery(50)], &[Event::SleepRequested]]),
        Recorder {
            fail: vec![1],
            ..Recorder::default()
        },
    );
    // Home; the battery failed and was shown again with a full refresh; then the sleep screen.
    assert_eq!(
        refreshes(&screen),
        [Refresh::Full, Refresh::Full, Refresh::Full]
    );
    assert!(slept);
    assert_eq!(rested, Some(Ok(())));
}

#[test]
fn a_panel_that_fails_the_sleep_screen_still_lets_the_device_sleep() {
    let Outcome {
        screen,
        slept,
        rested,
    } = run(
        &batches(&[&[Event::SleepRequested]]),
        Recorder {
            fail: vec![1],
            ..Recorder::default()
        },
    );
    assert_eq!(refreshes(&screen), [Refresh::Full]);
    assert!(slept);
    assert_eq!(rested, Some(Ok(())));
}

#[test]
fn a_panel_stuck_on_a_frame_is_given_up_after_its_time_and_the_frame_shown_again() {
    let Outcome { screen, rested, .. } = run(
        &batches(&[&[Event::Battery(50)], &[Event::SleepRequested]]),
        Recorder {
            stuck: vec![1],
            ..Recorder::default()
        },
    );
    // The battery's frame never finished: given up as failed, then shown again with a full
    // refresh; the sleep screen and the panel's sleep followed.
    assert_eq!(
        refreshes(&screen),
        [Refresh::Full, Refresh::Full, Refresh::Full]
    );
    assert_eq!(rested, Some(Ok(())));
}

#[test]
fn a_panel_stuck_going_to_sleep_is_given_up_after_its_time() {
    let Outcome {
        screen,
        slept,
        rested,
    } = run(
        &batches(&[&[Event::SleepRequested]]),
        Recorder {
            sleep_stuck: true,
            ..Recorder::default()
        },
    );
    assert!(slept);
    assert_eq!(rested, Some(Err(ScreenError)));
    assert!(!screen.asleep);
}

#[test]
fn a_frame_under_way_when_the_panel_is_asked_to_rest_is_finished_first() {
    let slow = Recorder {
        slow: 20,
        ..Recorder::default()
    };
    let steps = [Step::Send(Event::Battery(10)), Step::Turns(2), Step::Rest];
    let Outcome { screen, rested, .. } = run(&steps, slow);
    assert_eq!(refreshes(&screen), [Refresh::Full, Refresh::Fast]);
    assert_eq!(rested, Some(Ok(())));
    assert!(screen.asleep);
}

#[test]
fn a_newer_view_replaces_one_waiting_and_keeps_a_full_refresh() {
    let views: Views<NoopRawMutex> = Views::new();
    assert!(views.is_empty());
    let show = |seq, battery, refresh| Show {
        seq,
        view: View::Home(purestwhite_app::view::Home {
            battery: Some(battery),
            time: None,
        }),
        refresh,
    };
    views.put(show(0, 1, Refresh::Full));
    views.put(show(1, 2, Refresh::Fast));
    views.put(show(2, 3, Refresh::Half));
    assert!(!views.is_empty());
    assert_eq!(views.try_take(), Some(show(2, 3, Refresh::Full)));
    assert_eq!(views.try_take(), None);
    views.put(show(3, 4, Refresh::Fast));
    views.put(show(4, 5, Refresh::Half));
    assert_eq!(block_on(views.take()), show(4, 5, Refresh::Half));
}

#[test]
fn frames_are_given_once() {
    let links: Links<NoopRawMutex> = Links::new();
    assert!(links.give_frames(frames()));
    assert!(!links.give_frames(frames()));
}
