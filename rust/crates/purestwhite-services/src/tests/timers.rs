//! The timer loop on a clock the test moves: a timer fires once its time has come, and only for
//! its latest arming.

use crate::{Clock, Links, run_timers};
use core::cell::Cell;
use embassy_futures::block_on;
use embassy_futures::select::{Either, select};
use embassy_futures::yield_now;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;
use purestwhite_app::{Arm, Event, Timer};

/// A clock that moves only when the test moves it.
#[derive(Default)]
struct TestClock {
    now: Cell<u64>,
    moved: Signal<NoopRawMutex, ()>,
}

impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.now.get()
    }

    async fn at(&self, at_ms: u64) {
        while self.now.get() < at_ms {
            self.moved.wait().await;
        }
    }
}

impl TestClock {
    /// Moves the clock to `ms`, then lets the timer loop take its turns (a timer armed just before
    /// is seen then, at `ms`).
    async fn move_to(&self, ms: u64) {
        self.now.set(ms);
        self.moved.signal(());
        for _ in 0..10 {
            yield_now().await;
        }
    }
}

fn idle(after_ms: u32, id: u32) -> Arm {
    Arm {
        timer: Timer::Idle,
        after_ms,
        id,
    }
}

/// Runs the timer loop beside `script`, which gets the clock and the links.
fn with_loop(script: impl AsyncFnOnce(&TestClock, &Links<NoopRawMutex>)) {
    let clock = TestClock::default();
    let links: Links<NoopRawMutex> = Links::new();
    let ended = block_on(select(
        run_timers(&clock, links.timer_links()),
        script(&clock, &links),
    ));
    match ended {
        Either::First(never) => match never {},
        Either::Second(()) => {}
    }
}

#[test]
fn a_timer_fires_once_its_time_has_come() {
    with_loop(async |clock, links| {
        clock.move_to(1_000).await;
        links.timers.arm(idle(100, 7));
        clock.move_to(1_000).await;
        assert!(links.timers.is_settled());
        assert_eq!(links.timers.next_due(), Some(1_100));
        clock.move_to(1_099).await;
        assert!(links.events.try_receive().is_err());
        clock.move_to(1_100).await;
        assert_eq!(links.events.try_receive(), Ok(Event::Fired(Timer::Idle, 7)));
        assert_eq!(links.timers.next_due(), None);
        clock.move_to(5_000).await;
        assert!(links.events.try_receive().is_err(), "it fires once");
    });
}

#[test]
fn a_timer_armed_again_fires_only_for_the_new_arming() {
    with_loop(async |clock, links| {
        links.timers.arm(idle(100, 1));
        clock.move_to(0).await;
        clock.move_to(50).await;
        // Seen by the loop at 50: due at 150, not at the first arming's 100.
        links.timers.arm(idle(100, 2));
        clock.move_to(50).await;
        clock.move_to(100).await;
        assert!(links.events.try_receive().is_err());
        clock.move_to(150).await;
        assert_eq!(links.events.try_receive(), Ok(Event::Fired(Timer::Idle, 2)));
        assert!(links.events.try_receive().is_err());
    });
}

#[test]
fn a_timer_not_yet_seen_by_the_loop_is_not_settled() {
    let links: Links<NoopRawMutex> = Links::new();
    assert!(links.timers.is_settled());
    links.timers.arm(idle(100, 1));
    assert!(!links.timers.is_settled());
    assert!(!links.is_idle());
}
