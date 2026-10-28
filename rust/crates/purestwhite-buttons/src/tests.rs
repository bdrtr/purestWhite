// The cases follow freeink-sdk's `InputManager::update` for plain GPIO buttons step by step.
#![allow(clippy::unwrap_used)]

use super::*;
extern crate std;
use std::vec::Vec;

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

/// Polls `b` and `h` with `sampled` every millisecond from `from` to `to`, inclusive: the times
/// `h` asked to sleep.
fn hold(b: &mut Buttons, h: &mut PowerHold, sampled: u8, from: u32, to: u32) -> Vec<u32> {
    (from..=to)
        .filter(|&now| {
            b.update(now, sampled);
            h.update(b, sampled, now)
        })
        .collect()
}

#[test]
fn a_power_hold_asks_once_past_its_time() {
    let mut b = Buttons::new(2);
    let mut h = PowerHold::new(400, DOWN);
    assert!(hold(&mut b, &mut h, 0, 0, 50).is_empty());
    // Committed at 106: held more than 400 ms at 507.
    assert_eq!(hold(&mut b, &mut h, POWER, 100, 2000), [507]);
    assert!(hold(&mut b, &mut h, 0, 2001, 2100).is_empty());
    assert_eq!(hold(&mut b, &mut h, POWER, 3000, 4000), [3407]);
}

#[test]
fn the_press_that_woke_the_device_does_not_count() {
    let mut b = Buttons::new(2);
    let mut h = PowerHold::new(400, DOWN);
    // Held from the first poll, before its debounce commits it.
    assert!(hold(&mut b, &mut h, POWER, 0, 3000).is_empty());
    assert!(hold(&mut b, &mut h, 0, 3001, 3100).is_empty());
    assert_eq!(hold(&mut b, &mut h, POWER, 3200, 4000), [3607]);
}

#[test]
fn a_blocker_holds_the_request_back_until_it_is_up() {
    let mut b = Buttons::new(2);
    let mut h = PowerHold::new(400, DOWN);
    hold(&mut b, &mut h, 0, 0, 10);
    assert!(hold(&mut b, &mut h, POWER | DOWN, 100, 1000).is_empty());
    // Down up while Power stays down: the request comes once Down's release commits.
    assert_eq!(hold(&mut b, &mut h, POWER, 1001, 2000), [1007]);
}

#[test]
fn a_power_hold_times_through_the_wrap() {
    let mut b = Buttons::new(2);
    let mut h = PowerHold::new(10, 0);
    let start = u32::MAX - 20;
    b.update(start, 0);
    h.update(&b, 0, start);
    let mut asked = Vec::new();
    for step in 0..40u32 {
        let now = start.wrapping_add(step);
        b.update(now, POWER);
        if h.update(&b, POWER, now) {
            asked.push(step);
        }
    }
    assert_eq!(asked, [17]);
}
