//! The page loop: started by Right when the card holds pages, a page read and shown every
//! `PAGE_MS`, the next read only once the panel has answered, the sleep after `LOOP_PAGES`, and
//! Left back home.

use super::{idle, list};
use crate::view::{Home, View};
use crate::{
    App, Arm, Command, Commands, Event, Job, Key, LOOP_PAGES, PAGE_MS, Refresh, Show, Timer,
};
use std::vec::Vec;

fn load(n: u16) -> Command {
    Command::Run(Job::LoadPage(n))
}

/// The views `commands` shows, with their refresh.
fn shows(commands: Commands) -> Vec<(View, Refresh)> {
    commands
        .into_iter()
        .filter_map(|c| match c {
            Command::Show(Show { view, refresh, .. }) => Some((view, refresh)),
            _ => None,
        })
        .collect()
}

/// The page timer armed in `commands`.
fn page_timer(commands: Commands) -> Option<Arm> {
    commands.into_iter().find_map(|c| match c {
        Command::Arm(arm) if arm.timer == Timer::Page => Some(arm),
        _ => None,
    })
}

/// An app started, its first screen shown, `pages` pages on the card.
fn started(pages: u16) -> App {
    let mut app = App::new();
    let _ = app.start();
    let _ = app.handle(Event::Shown(0));
    let _ = app.handle(Event::Pages(pages));
    app
}

/// Page `n` read and shown; the page timer it armed.
fn show_page(app: &mut App, n: u16, seq: u32) -> Arm {
    let commands = app.handle(Event::PageLoaded(n));
    let arm = page_timer(commands).unwrap();
    assert_eq!(arm.after_ms, PAGE_MS);
    assert!(app.handle(Event::Shown(seq)).is_empty());
    arm
}

#[test]
fn right_without_pages_is_a_key_like_any_other() {
    let mut app = started(0);
    assert_eq!(list(app.handle(Event::Pressed(Key::Right))), [idle(1)]);
}

#[test]
fn right_starts_the_loop_with_page_one_and_the_idle_sleep_waits() {
    let mut app = started(3);
    assert_eq!(list(app.handle(Event::Pressed(Key::Right))), [load(1)]);
    let commands = app.handle(Event::PageLoaded(1));
    assert_eq!(shows(commands), [(View::Page(1), Refresh::Full)]);
    assert!(page_timer(commands).is_some());
    // The idle timer armed at the start no longer sleeps.
    assert!(app.handle(Event::Fired(Timer::Idle, 0)).is_empty());
    assert!(!app.is_asleep());
}

#[test]
fn pages_follow_each_other_and_wrap_with_fast_refreshes() {
    let mut app = started(2);
    let _ = app.handle(Event::Pressed(Key::Right));
    let mut arm = show_page(&mut app, 1, 1);
    for (seq, n) in (2..).zip([2, 1, 2]) {
        assert_eq!(
            list(app.handle(Event::Fired(Timer::Page, arm.id))),
            [load(n)]
        );
        let commands = app.handle(Event::PageLoaded(n));
        assert_eq!(shows(commands), [(View::Page(n), Refresh::Fast)]);
        arm = page_timer(commands).unwrap();
        let _ = app.handle(Event::Shown(seq));
    }
}

#[test]
fn the_next_page_waits_for_the_panels_answer() {
    let mut app = started(3);
    let _ = app.handle(Event::Pressed(Key::Right));
    let arm = page_timer(app.handle(Event::PageLoaded(1))).unwrap();
    // The panel has not answered for page 1 yet: page 2 is not read into the page it copies.
    assert!(app.handle(Event::Fired(Timer::Page, arm.id)).is_empty());
    assert_eq!(list(app.handle(Event::Shown(1))), [load(2)]);
}

#[test]
fn after_an_hour_of_pages_the_device_sleeps() {
    let mut app = started(5);
    let _ = app.handle(Event::Pressed(Key::Right));
    let mut arm = show_page(&mut app, 1, 1);
    for seq in 2..=u32::from(LOOP_PAGES) {
        let Some(Command::Run(Job::LoadPage(n))) = app
            .handle(Event::Fired(Timer::Page, arm.id))
            .into_iter()
            .next()
        else {
            unreachable!("a page is read");
        };
        arm = show_page(&mut app, n, seq);
    }
    let last = app.handle(Event::Fired(Timer::Page, arm.id));
    assert_eq!(shows(last), [(View::Sleep, Refresh::Full)]);
    let _ = app.handle(Event::Shown(u32::from(LOOP_PAGES) + 1));
    assert!(app.is_asleep());
}

#[test]
fn left_goes_back_home_with_what_changed_meanwhile() {
    let mut app = started(3);
    let _ = app.handle(Event::Pressed(Key::Right));
    let arm = show_page(&mut app, 1, 1);
    // Updates and other keys show nothing while the pages run.
    assert!(app.handle(Event::Battery(70)).is_empty());
    assert!(app.handle(Event::Pressed(Key::Right)).is_empty());
    let back = app.handle(Event::Pressed(Key::Left));
    let home = View::Home(Home {
        battery: Some(70),
        time: None,
    });
    assert_eq!(shows(back), [(home, Refresh::Full)]);
    assert!(
        back.into_iter()
            .any(|c| matches!(c, Command::Arm(a) if a.timer == Timer::Idle))
    );
    // The page timer armed before is out of date.
    assert!(app.handle(Event::Fired(Timer::Page, arm.id)).is_empty());
}

#[test]
fn a_page_that_cannot_be_read_ends_the_loop() {
    let mut app = started(3);
    let _ = app.handle(Event::Pressed(Key::Right));
    let commands = app.handle(Event::PageFailed(1));
    assert_eq!(
        shows(commands),
        [(View::Home(Home::default()), Refresh::Full)]
    );
    // A late answer for another page changes nothing.
    assert!(app.handle(Event::PageLoaded(2)).is_empty());
}

#[test]
fn right_waits_for_a_page_read_for_a_loop_that_ended() {
    let mut app = started(3);
    let _ = app.handle(Event::Pressed(Key::Right));
    // Left while page 1 is read: home; Right again before its answer starts nothing.
    let _ = app.handle(Event::Pressed(Key::Left));
    assert_eq!(list(app.handle(Event::Pressed(Key::Right))), [idle(2)]);
    assert!(app.handle(Event::PageLoaded(1)).is_empty());
    assert_eq!(list(app.handle(Event::Pressed(Key::Right))), [load(1)]);
}
