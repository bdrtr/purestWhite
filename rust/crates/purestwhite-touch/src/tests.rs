use super::*;
use purestwhite_gt911::{Contact, Frame, MAX_CONTACTS};

/// A frame with `points` as its contacts and `reported` as the controller's count.
fn frame(points: &[(u16, u16)], reported: u8, home_key: bool) -> Reading {
    let mut contacts = [Contact::default(); MAX_CONTACTS];
    for ((slot, &(x, y)), id) in contacts.iter_mut().zip(points).zip(0u8..) {
        *slot = Contact { id, x, y };
    }
    Reading::Frame(Frame {
        home_key,
        reported,
        count: u8::try_from(points.len().min(MAX_CONTACTS)).unwrap_or(0),
        contacts,
    })
}

fn at(x: u16, y: u16) -> Reading {
    frame(&[(x, y)], 1, false)
}

fn lift() -> Reading {
    frame(&[], 0, false)
}

fn home(down: bool) -> Reading {
    frame(&[], 0, down)
}

const P: Point = Point { x: 100, y: 200 };

#[test]
fn a_still_contact_taps_where_it_touched_down() {
    let mut t = Touch::new();
    let e = t.update(1000, &at(100, 200));
    assert_eq!(e.pressed, Some(P));
    assert!(e.activity);
    // Drifting less than a swipe still taps, at the touch-down point.
    assert_eq!(t.update(1020, &at(150, 200)).tap, None);
    assert_eq!(t.tap_candidate(1030), None, "beyond the hold slop");
    assert_eq!(t.held_at(), Some(Point { x: 150, y: 200 }));
    let e = t.update(1040, &lift());
    assert!(e.released && e.activity);
    assert_eq!(e.tap, Some(P));
    assert_eq!(e.swipe, None);
    assert_eq!(t.last_held_ms(), 40);
    assert!(!t.is_pressed());
}

#[test]
fn a_quick_long_move_swipes_and_a_slow_one_does_nothing() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    t.update(1100, &at(100, 260));
    let e = t.update(1200, &lift());
    assert_eq!(e.swipe, Some((P, Point { x: 100, y: 260 })));
    assert_eq!(e.tap, None);

    t.update(2000, &at(100, 200));
    t.update(2600, &at(100, 260));
    let e = t.update(2701, &lift());
    assert_eq!(e.swipe, None, "longer than SWIPE_MAX_MS");
    assert_eq!(e.tap, None);
    assert!(e.released);
}

#[test]
fn a_swipe_may_last_exactly_its_limit() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    t.update(1100, &at(160, 200));
    assert!(t.update(1700, &lift()).swipe.is_some());
    t.update(2000, &at(100, 200));
    t.update(2100, &at(160, 200));
    assert!(t.update(2701, &lift()).swipe.is_none());
}

#[test]
fn a_held_contact_long_presses_once_and_suppress_ends_it() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    assert_eq!(t.tap_candidate(1300), Some((P, 300)));
    // No new frame while held: the time alone fires it.
    assert_eq!(t.update(1499, &Reading::NotReady).long_press, None);
    assert_eq!(t.update(1500, &Reading::NotReady).long_press, Some(P));
    assert_eq!(t.update(1600, &Reading::NotReady).long_press, None);
    t.suppress();
    assert_eq!(t.held_at(), None);
    let e = t.update(1700, &lift());
    assert!(!e.released && e.tap.is_none());
    assert!(e.activity);
    // The next contact is delivered again.
    t.update(1800, &Reading::NotReady);
    assert_eq!(
        t.update(1900, &at(10, 10)).pressed,
        Some(Point { x: 10, y: 10 })
    );
}

#[test]
fn a_moved_contact_does_not_long_press() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    t.update(1100, &at(129, 200));
    assert_eq!(t.update(2000, &Reading::NotReady).long_press, None);
}

#[test]
fn a_second_finger_makes_a_sequence_without_single_gestures() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    let e = t.update(1020, &frame(&[(100, 200), (300, 300)], 2, false));
    assert_eq!(e.pressed, None);
    assert_eq!(t.held_at(), None);
    t.update(1040, &at(100, 200));
    assert_eq!(t.update(2000, &Reading::NotReady).long_press, None);
    let e = t.update(2010, &lift());
    assert!(e.released, "the raw release still comes");
    assert_eq!(e.tap, None);
    // A sequence that starts with two fingers gives no press edge either.
    t.update(2020, &Reading::NotReady);
    let e = t.update(2030, &frame(&[(1, 1), (2, 2), (3, 3), (4, 4)], 6, false));
    assert_eq!(e.pressed, None);
    assert!(e.activity);
}

#[test]
fn failed_reads_keep_the_contact() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    t.update(1010, &Reading::Failed);
    t.update(1020, &Reading::ContactsFailed { home_key: false });
    assert!(t.is_pressed());
    assert_eq!(t.update(1030, &lift()).tap, Some(P));
}

#[test]
fn the_home_key_taps_or_long_presses() {
    let mut t = Touch::new();
    let e = t.update(1000, &home(true));
    assert!(e.home_pressed && e.activity);
    let e = t.update(1100, &home(false));
    assert!(e.home_tapped && !e.home_pressed);

    t.update(2000, &home(true));
    // A failed status read does not time it; any other reading does.
    assert!(!t.update(2700, &Reading::Failed).home_long_press);
    assert!(t.update(2700, &Reading::NotReady).home_long_press);
    assert!(!t.update(2800, &Reading::NotReady).home_long_press);
    assert!(
        !t.update(2900, &home(false)).home_tapped,
        "a long press does not also tap"
    );
}

#[test]
fn home_key_activity_waits_for_a_held_contact() {
    let mut t = Touch::new();
    t.update(1000, &at(100, 200));
    let e = t.update(1010, &frame(&[(100, 200)], 1, true));
    assert!(e.home_pressed && !e.activity);
}

#[test]
fn times_wrap() {
    let mut t = Touch::new();
    t.update(u32::MAX - 100, &at(100, 200));
    assert_eq!(t.update(399, &Reading::NotReady).long_press, Some(P));
    t.update(450, &lift());
    assert_eq!(t.last_held_ms(), 551);
}

#[test]
fn normalize_divides_by_the_panel_size() {
    assert_eq!(normalize(Point { x: 240, y: 800 }, 480, 800), (0.5, 1.0));
    assert_eq!(normalize(Point { x: 900, y: 3 }, 480, 0), (1.0, 1.0));
}

#[test]
fn each_contact_has_its_number() {
    let mut t = Touch::new();
    let first = t.update(1000, &at(100, 200)).contact;
    assert_eq!(t.update(1600, &Reading::NotReady).contact, first);
    let lifted = t.update(1700, &lift());
    assert!(lifted.tap.is_some());
    assert_eq!(
        lifted.contact, first,
        "the tap is the long-pressed contact's"
    );
    assert_eq!(t.update(1800, &at(1, 1)).contact, first.wrapping_add(1));
}
