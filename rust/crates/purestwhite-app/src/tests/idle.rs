//! The idle timer: armed at the start and again at each key press; the latest arming's firing
//! sleeps as Power held does.

use super::{idle, list};
use crate::view::View;
use crate::{App, Command, Event, Key, Refresh, Show, Timer};

/// The sleep screen, shown with `seq`.
fn sleep_screen(seq: u32) -> Command {
    Command::Show(Show {
        seq,
        view: View::Sleep,
        refresh: Refresh::Full,
    })
}

#[test]
fn each_key_press_arms_the_idle_timer_again_and_draws_nothing() {
    let mut app = App::new();
    let _ = app.start();
    for (id, key) in (1..).zip([Key::Left, Key::Right, Key::Power, Key::Home]) {
        assert_eq!(list(app.handle(Event::Pressed(key))), [idle(id)]);
    }
}

#[test]
fn the_idle_timer_sleeps_as_power_held_does() {
    let mut app = App::new();
    let _ = app.start();
    assert_eq!(
        list(app.handle(Event::Fired(Timer::Idle, 0))),
        [sleep_screen(1)]
    );
    assert!(app.handle(Event::Fired(Timer::Idle, 0)).is_empty());
    assert_eq!(list(app.handle(Event::Shown(1))), [Command::Sleep]);
    assert!(app.is_asleep());
}

#[test]
fn a_firing_of_an_earlier_arming_is_out_of_date() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::Pressed(Key::Left));
    assert!(app.handle(Event::Fired(Timer::Idle, 0)).is_empty());
    assert!(!app.is_asleep());
    assert_eq!(
        list(app.handle(Event::Fired(Timer::Idle, 1))),
        [sleep_screen(1)]
    );
}

#[test]
fn keys_on_the_way_to_sleep_arm_nothing() {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::SleepRequested);
    assert!(app.handle(Event::Pressed(Key::Left)).is_empty());
    assert!(app.handle(Event::Fired(Timer::Idle, 0)).is_empty());
}
