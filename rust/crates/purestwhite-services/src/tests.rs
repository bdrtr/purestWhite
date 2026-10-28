// The tests unwrap and index what must be there, and fail when the loops do not settle.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]

extern crate std;

use super::*;
use embassy_futures::block_on;
use embassy_futures::select::{Either4, select4};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use purestwhite_app::view::{Time, View};
use purestwhite_app::{App, Show};
use std::boxed::Box;
use std::vec;
use std::vec::Vec;

/// A screen that records each refresh and how many pixels of the frame were black, takes `slow`
/// turns of the loops for each frame, and fails the calls numbered in `fail` (from 0).
#[derive(Default)]
struct Recorder {
    shown: Vec<(Refresh, u32)>,
    calls: usize,
    fail: Vec<usize>,
    slow: usize,
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
        if self.fail.contains(&call) {
            return Err(ScreenError);
        }
        let black = frame.iter().map(|b| (!b).count_ones()).sum();
        self.shown.push((refresh, black));
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

/// Waits until the loops are idle; panics if they are not after many turns.
async fn settle(links: &Links<NoopRawMutex>) {
    for _ in 0..1000 {
        if links.is_idle() {
            return;
        }
        yield_now().await;
    }
    panic!("the loops did not settle");
}

/// Runs the loops: the first screen, then `steps`; what the screen showed, and whether the
/// application asked to sleep.
fn run(steps: &[Step], screen: Recorder) -> (Recorder, bool) {
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
            }
        }
        settle(&links).await;
    };
    block_on(async {
        let result = select4(
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
                None,
            ),
            run_panel(
                &mut screen,
                links.drawn.dyn_receiver(),
                links.empty.dyn_sender(),
                links.events.dyn_sender(),
            ),
            script,
        )
        .await;
        assert!(matches!(result, Either4::Fourth(())));
    });
    let slept = links.sleep.signaled();
    (screen, slept)
}

fn refreshes(screen: &Recorder) -> Vec<Refresh> {
    screen.shown.iter().map(|s| s.0).collect()
}

#[test]
fn home_then_its_updates_then_the_sleep_screen_then_sleep() {
    let time = Time { hour: 7, minute: 5 };
    let (screen, slept) = run(
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
    // The battery added ink; the sleep screen's moon is the most ink of all.
    let ink: Vec<_> = screen.shown.iter().map(|s| s.1).collect();
    assert!(ink[1] > ink[0]);
    assert!(ink[3] > ink[1]);
}

#[test]
fn views_that_come_faster_than_the_panel_are_drawn_once() {
    // Three changes before drawing runs again: one frame for them all.
    let (screen, slept) = run(
        &batches(&[&[Event::Battery(10), Event::Battery(20), Event::Battery(30)]]),
        Recorder::default(),
    );
    assert!(!slept);
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
    let (screen, _) = run(&steps, slow);
    assert_eq!(
        refreshes(&screen),
        [Refresh::Full, Refresh::Fast, Refresh::Fast]
    );
    let (direct, _) = run(&batches(&[&[Event::Battery(30)]]), Recorder::default());
    assert_eq!(screen.shown[2].1, direct.shown[1].1);
}

#[test]
fn a_failed_frame_is_answered_and_shown_again_and_sleep_still_comes() {
    let (screen, slept) = run(
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
}

#[test]
fn a_panel_that_fails_the_sleep_screen_still_lets_the_device_sleep() {
    let (screen, slept) = run(
        &batches(&[&[Event::SleepRequested]]),
        Recorder {
            fail: vec![1],
            ..Recorder::default()
        },
    );
    assert_eq!(refreshes(&screen), [Refresh::Full]);
    assert!(slept);
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
