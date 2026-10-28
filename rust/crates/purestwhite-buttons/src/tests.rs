// The cases follow freeink-sdk's `InputManager::update` for plain GPIO buttons step by step.
#![allow(clippy::unwrap_used)]

use super::*;

const UP: u8 = 1 << 0;
const DOWN: u8 = 1 << 1;
const POWER: u8 = 1 << 2;

fn pressed(bits: u8) -> Change {
    Change {
        pressed: bits,
        released: 0,
    }
}

fn released(bits: u8) -> Change {
    Change {
        pressed: 0,
        released: bits,
    }
}

#[test]
fn a_state_counts_once_more_than_the_debounce_time_has_passed() {
    let mut b = Buttons::new(2);
    assert!(b.update(100, UP).is_empty());
    // Exactly 5 ms is not more than 5.
    assert!(b.update(105, UP).is_empty());
    assert_eq!(b.update(106, UP), pressed(UP));
    assert_eq!(b.state(), UP);
    // Once committed, the same state reports nothing more.
    assert!(b.update(200, UP).is_empty());
    assert!(b.update(300, 0).is_empty());
    assert_eq!(b.update(306, 0), released(UP));
}

#[test]
fn bounces_restart_the_wait() {
    let mut b = Buttons::new(2);
    for (t, s) in [(0, UP), (2, 0), (4, UP), (6, 0), (8, UP)] {
        assert!(b.update(t, s).is_empty(), "{t}");
    }
    assert!(b.update(13, UP).is_empty());
    assert_eq!(b.update(14, UP), pressed(UP));
    // A bounce back to the committed state before the wait ends changes nothing.
    assert!(b.update(20, 0).is_empty());
    assert!(b.update(22, UP).is_empty());
    assert!(b.update(40, UP).is_empty());
    assert_eq!(b.state(), UP);
}

#[test]
fn overlapping_presses_and_their_times() {
    let mut b = Buttons::new(2);
    b.update(0, UP);
    assert_eq!(b.update(10, UP), pressed(UP));
    // A second button while the first is down: only it is new, the press began with the first.
    b.update(50, UP | DOWN);
    assert_eq!(b.update(60, UP | DOWN), pressed(DOWN));
    assert_eq!(b.held_ms(100), 90);
    b.update(200, DOWN);
    assert_eq!(b.update(210, DOWN), released(UP));
    assert_eq!(b.held_ms(300), 290);
    b.update(400, 0);
    assert_eq!(b.update(410, 0), released(DOWN));
    // Released: the last press, from the first button down to the last up.
    assert_eq!(b.held_ms(10_000), 400);
}

#[test]
fn the_power_button_is_timed_on_its_own() {
    let mut b = Buttons::new(2);
    b.update(0, UP);
    b.update(10, UP);
    b.update(100, UP | POWER);
    assert_eq!(b.update(110, UP | POWER), pressed(POWER));
    assert_eq!(b.power_held_ms(1110), 1000);
    assert_eq!(b.held_ms(1110), 1100);
    b.update(1200, UP);
    assert_eq!(b.update(1210, UP), released(POWER));
    assert_eq!(b.power_held_ms(5000), 1100);
    // Another bit as the power button.
    let mut other = Buttons::new(0);
    other.update(0, UP);
    other.update(10, UP);
    assert_eq!(other.power_held_ms(510), 500);
}

#[test]
fn time_wraps_like_millis() {
    let start = u32::MAX - 3;
    let mut b = Buttons::new(2);
    b.update(start, POWER);
    assert!(b.update(start.wrapping_add(5), POWER).is_empty());
    assert_eq!(b.update(start.wrapping_add(6), POWER), pressed(POWER));
    assert_eq!(b.power_held_ms(start.wrapping_add(1006)), 1000);
    b.update(start.wrapping_add(2000), 0);
    assert_eq!(b.update(start.wrapping_add(2006), 0), released(POWER));
    assert_eq!(b.power_held_ms(7), 2000);
}

#[test]
fn a_new_set_has_nothing_pressed() {
    let b = Buttons::new(2);
    assert_eq!(b.state(), 0);
    assert_eq!(b.held_ms(123), 0);
    assert_eq!(b.power_held_ms(123), 0);
    // A state already held at the start still waits out the debounce from time 0.
    let mut c = Buttons::new(2);
    assert!(c.update(3, DOWN).is_empty());
    assert_eq!(c.update(9, DOWN), pressed(DOWN));
}

#[test]
fn the_deadline_is_when_a_waiting_state_commits() {
    let mut b = Buttons::new(2);
    assert_eq!(b.deadline(), None);
    b.update(100, UP);
    assert_eq!(b.deadline(), Some(106));
    // Sampled again at the deadline, it commits; nothing waits after.
    assert_eq!(b.update(106, UP), pressed(UP));
    assert_eq!(b.deadline(), None);
    // A bounce moves the deadline; going back to the committed state clears it.
    b.update(200, 0);
    assert_eq!(b.deadline(), Some(206));
    b.update(203, UP);
    assert_eq!(b.deadline(), None);
    // It wraps like the time.
    let mut w = Buttons::new(2);
    w.update(u32::MAX - 1, DOWN);
    assert_eq!(w.deadline(), Some(4));
}
