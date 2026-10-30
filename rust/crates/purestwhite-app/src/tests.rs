// The tests index what must be there and fail on what is not: a failure fails the test.
#![allow(clippy::indexing_slicing, clippy::panic)]

use super::view::Home;
use super::*;
use std::vec::Vec;

extern crate std;

mod idle;

fn list(commands: Commands) -> Vec<Command> {
    commands.into_iter().collect()
}

fn home(seq: u32, battery: Option<u8>, time: Option<Time>, refresh: Refresh) -> Command {
    Command::Show(Show {
        seq,
        view: View::Home(Home { battery, time }),
        refresh,
    })
}

/// The idle timer armed with `id`.
fn idle(id: u32) -> Command {
    Command::Arm(Arm {
        timer: Timer::Idle,
        after_ms: IDLE_MS,
        id,
    })
}

/// The refresh of the one show `commands` holds, besides any timer armed.
fn refresh(commands: Commands) -> Refresh {
    let shows: Vec<Show> = commands
        .into_iter()
        .filter_map(|command| match command {
            Command::Show(show) => Some(show),
            _ => None,
        })
        .collect();
    match shows.as_slice() {
        [show] => show.refresh,
        other => panic!("not one show: {other:?}"),
    }
}

#[test]
fn it_starts_on_the_home_screen_with_a_full_refresh_and_the_idle_timer() {
    let mut app = App::new();
    assert_eq!(
        list(app.start()),
        [home(0, None, None, Refresh::Full), idle(0)]
    );
}

#[test]
fn the_battery_and_the_time_redraw_home_only_when_they_change() {
    let mut app = App::new();
    let _ = app.start();
    assert_eq!(
        list(app.handle(Event::Battery(87))),
        [home(1, Some(87), None, Refresh::Fast)]
    );
    assert!(app.handle(Event::Battery(87)).is_empty());
    let time = Time {
        hour: 21,
        minute: 30,
    };
    assert_eq!(
        list(app.handle(Event::Time(time))),
        [home(2, Some(87), Some(time), Refresh::Fast)]
    );
    assert!(app.handle(Event::Time(time)).is_empty());
    // A gauge's reading past 100 is shown as 100.
    assert_eq!(
        list(app.handle(Event::Battery(130))),
        [home(3, Some(100), Some(time), Refresh::Fast)]
    );
}

#[test]
fn every_fifteenth_refresh_is_a_half_one_as_in_the_cpp_reader() {
    let mut app = App::new();
    assert_eq!(refresh(app.start()), Refresh::Full);
    let mut halves = Vec::new();
    for n in 1..=30u8 {
        if refresh(app.handle(Event::Battery(n))) == Refresh::Half {
            halves.push(n);
        }
    }
    assert_eq!(halves, [15, 30]);
}

#[test]
fn a_full_refresh_starts_the_count_again() {
    let mut app = App::new();
    let _ = app.start();
    for n in 1..=10 {
        let _ = app.handle(Event::Battery(n));
    }
    // A failure makes the next show a full refresh; 14 fast ones follow it.
    let _ = app.handle(Event::ScreenFailed(0));
    let refreshes: Vec<_> = (11..=26)
        .map(|n| refresh(app.handle(Event::Battery(n))))
        .collect();
    assert_eq!(refreshes[0], Refresh::Full);
    assert!(refreshes[1..15].iter().all(|r| *r == Refresh::Fast));
    assert_eq!(refreshes[15], Refresh::Half);
}

#[test]
fn power_held_shows_the_sleep_screen_and_sleeps_once_it_is_shown() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::Battery(50));
    assert_eq!(
        list(app.handle(Event::SleepRequested)),
        [Command::Show(Show {
            seq: 2,
            view: View::Sleep,
            refresh: Refresh::Full
        })]
    );
    // Nothing more is drawn; an answer for an earlier view does not sleep yet.
    assert!(app.handle(Event::Battery(10)).is_empty());
    assert!(app.handle(Event::SleepRequested).is_empty());
    assert!(app.handle(Event::Shown(1)).is_empty());
    assert!(!app.is_asleep());
    assert_eq!(list(app.handle(Event::Shown(2))), [Command::Sleep]);
    assert!(app.is_asleep());
    assert!(app.handle(Event::Shown(2)).is_empty());
    assert!(app.handle(Event::Battery(5)).is_empty());
}

#[test]
fn a_panel_that_fails_the_sleep_screen_does_not_keep_the_device_awake() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::SleepRequested);
    assert_eq!(list(app.handle(Event::ScreenFailed(1))), [Command::Sleep]);
}

#[test]
fn a_failed_screen_is_shown_again_with_a_full_refresh_twice_at_most() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::Battery(40));
    assert_eq!(
        list(app.handle(Event::ScreenFailed(1))),
        [home(2, Some(40), None, Refresh::Full)]
    );
    assert_eq!(
        list(app.handle(Event::ScreenFailed(2))),
        [home(3, Some(40), None, Refresh::Full)]
    );
    assert!(app.handle(Event::ScreenFailed(3)).is_empty());
    // The next change is still a full refresh; once the panel shows it, failures count afresh.
    assert_eq!(
        list(app.handle(Event::Battery(41))),
        [home(4, Some(41), None, Refresh::Full)]
    );
    let _ = app.handle(Event::Shown(4));
    assert_eq!(
        list(app.handle(Event::ScreenFailed(4))),
        [home(5, Some(41), None, Refresh::Full)]
    );
}

#[test]
fn a_failure_with_a_newer_view_on_its_way_shows_nothing_more() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::Battery(40));
    assert!(app.handle(Event::ScreenFailed(0)).is_empty());
    // The newer view still comes; the one after it is a full refresh.
    assert_eq!(refresh(app.handle(Event::Battery(41))), Refresh::Full);
}

#[test]
fn a_panel_that_recovers_with_a_newer_view_needs_no_more_full_refreshes() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::Battery(40));
    let _ = app.handle(Event::Battery(41));
    // The view before the latest failed; the latest was shown, so the panel works again.
    assert!(app.handle(Event::ScreenFailed(1)).is_empty());
    assert!(app.handle(Event::Shown(2)).is_empty());
    assert_eq!(refresh(app.handle(Event::Battery(42))), Refresh::Fast);
}

#[test]
fn the_stronger_refresh_clears_more() {
    use Refresh::{Fast, Full, Half};
    for (a, b, stronger) in [
        (Full, Fast, Full),
        (Fast, Full, Full),
        (Half, Fast, Half),
        (Fast, Half, Half),
        (Half, Full, Full),
        (Fast, Fast, Fast),
    ] {
        assert_eq!(a.stronger(b), stronger, "{a:?} {b:?}");
    }
}

#[test]
fn commands_keep_their_order() {
    let show = home(0, None, None, Refresh::Full);
    assert_eq!(list(Commands::none()), []);
    assert_eq!(list(Commands::one(show)), [show]);
    assert_eq!(
        list(Commands::two(show, Command::Sleep)),
        [show, Command::Sleep]
    );
}
