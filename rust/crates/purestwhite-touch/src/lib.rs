//! What one contact on a touch panel means, as freeink-sdk's `InputManager` reads it from a GT911
//! (its `serviceTouch` and `pollGt911`): the press and release edges, a tap, a swipe and a long
//! press of the first contact, and the GT911's capacitive Home key with its tap and long press.
//! A caller polls the controller (`purestwhite_gt911::read`) and passes each reading to
//! [`Touch::update`], which gives what that poll ended, as the SDK's `was*` queries do after an
//! `update()`.
//!
//! From the poll a second finger joins, a contact is a multi-contact sequence: as in the SDK, it
//! gives no further press edge, tap, swipe or long press (after `Touch::suppress`, only more than
//! `MAX_CONTACTS` fingers make it one). The SDK's two- to four-finger gestures (swipe, rotation,
//! pinch) are not here yet.
//!
//! Positions are panel coordinates as `purestwhite_gt911::read` gives them (swapped, mapped and
//! flipped as the SDK's `pollGt911` does); the SDK's queries
//! divide them by the panel's size (`normalize`). Times are milliseconds from any start, and
//! wrap as the SDK's `millis()` does.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
mod tests;

use purestwhite_gt911::{Frame, MAX_CONTACTS, Reading};

/// How far a contact may move and still be held in place, for a long press, in panel pixels:
/// the SDK's `TOUCH_TAP_SLOP_PX`. Farther counts once it is more than this on an axis.
pub const TAP_SLOP: i32 = 28;
/// How far a contact may move and still be a tap when it lifts: the SDK's
/// `TOUCH_TAP_RELEASE_SLOP_PX`, one less than `SWIPE_MIN`.
pub const TAP_RELEASE_SLOP: i32 = SWIPE_MIN - 1;
/// How far a swipe moves at least on one axis: the SDK's `TOUCH_SWIPE_MIN_PX`.
pub const SWIPE_MIN: i32 = 60;
/// How long a swipe lasts at most: the SDK's `TOUCH_SWIPE_MAX_MS`.
pub const SWIPE_MAX_MS: u32 = 700;
/// How long a contact held in place becomes a long press: the SDK's `TOUCH_LONG_PRESS_MS`.
pub const LONG_PRESS_MS: u32 = 500;
/// How long the Home key held becomes a long press: the SDK's `HOME_KEY_LONG_PRESS_MS`.
pub const HOME_LONG_PRESS_MS: u32 = 700;

/// A position on the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

impl Point {
    /// How far `to` is from here on each axis.
    fn distance(self, to: Point) -> (i32, i32) {
        (
            (i32::from(to.x) - i32::from(self.x)).abs(),
            (i32::from(to.y) - i32::from(self.y)).abs(),
        )
    }
}

/// What one poll ended. Positions are where the contact touched down, but a swipe's end.
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag for each of the SDK's, so the two can be compared"
)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Events {
    /// A contact touched down (the SDK's `wasTouchPressedAt`).
    pub pressed: Option<Point>,
    /// A contact lifted (`wasTouchReleased`): also after a multi-contact sequence, so that
    /// pressed-state feedback ends.
    pub released: bool,
    /// The contact lifted near where it touched down (`wasTouchTap`).
    pub tap: Option<Point>,
    /// The contact lifted far from where it touched down, soon enough (`wasSwipe`): start and
    /// end.
    pub swipe: Option<(Point, Point)>,
    /// The contact has been held in place long enough, and is still down (`wasTouchLongPress`).
    /// Once per contact.
    pub long_press: Option<Point>,
    /// The Home key went down (`wasHomeKeyPressed`).
    pub home_pressed: bool,
    /// The Home key came up before its long press (`wasHomeKeyTapped`).
    pub home_tapped: bool,
    /// The Home key has been held long enough, and is still down (`wasHomeKeyLongPressed`).
    pub home_long_press: bool,
    /// Something happened that ends an idle wait (`wasTouchActivity`): a contact's press or
    /// release, or the Home key's events while no contact is held.
    pub activity: bool,
    /// Which contact the contact's events are about: one more at each touch-down, wrapping. Not
    /// the SDK's; a caller across a channel uses it to drop the tap of a contact it already acted
    /// on as a long press, where the SDK's callers call `suppress`.
    pub contact: u32,
}

/// The state of the touch panel between polls, as the SDK keeps it.
#[allow(
    clippy::struct_excessive_bools,
    reason = "one flag for each of the SDK's, so the two can be compared"
)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Touch {
    /// A contact is down.
    pressed: bool,
    /// The last poll's edges.
    pressed_edge: bool,
    released_edge: bool,
    long_press_edge: bool,
    /// Where and when the contact touched down, and its last position.
    down: Point,
    down_at: u32,
    last: Point,
    /// How long the last contact lasted, from its release.
    held_ms: u32,
    /// The contact moved farther than `TAP_SLOP`, and than `TAP_RELEASE_SLOP`.
    moved: bool,
    moved_far: bool,
    /// The contact's long press has been given, or cannot be.
    long_press_done: bool,
    /// The caller asked to ignore the rest of the contact (`suppress`).
    suppressed: bool,
    /// The contact has had a second finger.
    multi: bool,
    /// The number of the current or last contact (`Events::contact`).
    contact: u32,
    /// The SDK's multi-touch gesture state is not idle: after a second finger, or a `suppress`.
    /// A second finger then no longer makes the contact `multi`, unless there are more than
    /// `MAX_CONTACTS`.
    gesture: bool,
    /// The Home key: down, since when, and whether its long press has been given.
    home_down: bool,
    home_down_at: u32,
    home_long_done: bool,
    /// The last poll's Home key edges.
    home_pressed_edge: bool,
    home_tapped_edge: bool,
    home_long_edge: bool,
}

impl Touch {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes a poll's reading at time `now`: the SDK's `update()` with `serviceTouch()` and
    /// `pollGt911()`, then its queries.
    pub fn update(&mut self, now: u32, reading: &Reading) -> Events {
        self.pressed_edge = false;
        self.released_edge = false;
        self.long_press_edge = false;
        self.home_pressed_edge = false;
        self.home_tapped_edge = false;
        self.home_long_edge = false;

        // A new contact starts afresh once the last one is over: its release was an earlier poll.
        if !self.pressed {
            self.suppressed = false;
            self.long_press_done = false;
            self.multi = false;
            self.gesture = false;
        }

        self.poll(now, reading);

        // Long press, while the contact is still down; once per contact.
        if self.pressed
            && !self.multi
            && !self.moved
            && !self.long_press_done
            && !self.suppressed
            && now.wrapping_sub(self.down_at) >= LONG_PRESS_MS
        {
            self.long_press_done = true;
            self.long_press_edge = true;
        }
        self.events()
    }

    /// The SDK's `pollGt911`, after its read.
    fn poll(&mut self, now: u32, reading: &Reading) {
        let (home_key, frame) = match reading {
            Reading::Failed => return,
            Reading::NotReady => {
                self.time_home(now);
                return;
            }
            Reading::Frame(frame) => (frame.home_key, Some(frame)),
            Reading::ContactsFailed { home_key } => (*home_key, None),
        };
        self.time_home(now);
        self.home_edges(now, home_key);
        // A frame whose contacts could not be read keeps the contact as it was. As in the SDK,
        // the count the controller reports decides.
        match frame {
            None => {}
            Some(frame) if frame.reported == 0 => self.lift(now),
            Some(frame) => self.contact(now, frame),
        }
    }

    /// The Home key's press and tap, from a new frame's key state.
    fn home_edges(&mut self, now: u32, home_key: bool) {
        if home_key && !self.home_down {
            self.home_pressed_edge = true;
            self.home_down_at = now;
            self.home_long_done = false;
        } else if !home_key && self.home_down && !self.home_long_done {
            self.home_tapped_edge = true;
        }
        self.home_down = home_key;
    }

    /// A frame without contacts: every finger lifted.
    fn lift(&mut self, now: u32) {
        if self.pressed {
            self.released_edge = true;
            self.held_ms = now.wrapping_sub(self.down_at);
        }
        self.pressed = false;
    }

    /// A frame with contacts: the first one, as the SDK follows it; `read` has stored at least
    /// one when the controller reports any.
    fn contact(&mut self, now: u32, frame: &Frame) {
        let several = frame.reported > 1;
        if several {
            self.second_finger(frame.reported);
        }
        let first = frame.contacts.first().copied().unwrap_or_default();
        let point = Point {
            x: first.x,
            y: first.y,
        };
        if !self.pressed {
            self.touch_down(now, point);
        }
        self.last = point;
        let (dx, dy) = self.down.distance(point);
        self.moved |= several || dx > TAP_SLOP || dy > TAP_SLOP;
        self.moved_far |= several || dx > TAP_RELEASE_SLOP || dy > TAP_RELEASE_SLOP;
        self.pressed = true;
    }

    /// A second finger makes the contact a multi-contact sequence, which no single-contact
    /// gesture follows (but see `gesture`).
    fn second_finger(&mut self, reported: u8) {
        if !self.gesture || usize::from(reported) > MAX_CONTACTS {
            self.multi = true;
            self.long_press_done = true;
        }
        self.gesture = true;
    }

    /// A new contact's first frame.
    fn touch_down(&mut self, now: u32, point: Point) {
        self.contact = self.contact.wrapping_add(1);
        self.pressed_edge = true;
        self.down = point;
        self.down_at = now;
        self.moved = false;
        self.moved_far = false;
    }

    /// The Home key's long press, from the key's state and the time: a key held still gives no
    /// new frames.
    fn time_home(&mut self, now: u32) {
        if self.home_down
            && !self.home_long_done
            && now.wrapping_sub(self.home_down_at) >= HOME_LONG_PRESS_MS
        {
            self.home_long_edge = true;
            self.home_long_done = true;
        }
    }

    /// The SDK's queries after a poll.
    fn events(&self) -> Events {
        let single = !self.multi;
        let lifted = self.released_edge && !self.suppressed && single;
        let (dx, dy) = self.down.distance(self.last);
        let home = self.home_pressed_edge || self.home_tapped_edge || self.home_long_edge;
        Events {
            pressed: (self.pressed_edge && single).then_some(self.down),
            released: self.released_edge && (!self.suppressed || self.multi),
            tap: (lifted && !self.moved_far).then_some(self.down),
            swipe: (lifted && self.held_ms <= SWIPE_MAX_MS && (dx >= SWIPE_MIN || dy >= SWIPE_MIN))
                .then_some((self.down, self.last)),
            long_press: (self.long_press_edge && single).then_some(self.down),
            home_pressed: self.home_pressed_edge,
            home_tapped: self.home_tapped_edge,
            home_long_press: self.home_long_edge,
            activity: self.pressed_edge || self.released_edge || (!self.pressed && home),
            contact: self.contact,
        }
    }

    /// Ignores the rest of the current contact (the SDK's `suppressTouchContact`): no tap, swipe
    /// or long press follows, nor a release unless the contact is a multi-contact sequence, and
    /// `tap_candidate` and `held_at` give nothing, until it lifts. For a caller that acted on a
    /// long press, so the lift does not also tap. Nothing when no contact is down or just lifted.
    /// It changes only later polls: unlike the SDK's live queries, `Events` already returned
    /// keep their release, tap or swipe.
    pub fn suppress(&mut self) {
        if self.pressed || self.released_edge {
            self.suppressed = true;
        }
        self.gesture = self.pressed || self.released_edge;
    }

    /// A contact is down (`isTouchPressed`), also one of several fingers.
    #[must_use]
    pub fn is_pressed(&self) -> bool {
        self.pressed
    }

    /// While a single contact is down and still within `TAP_SLOP` of where it touched down
    /// (`isTouchTapCandidate`): that point, and how long it has been down at `now`.
    #[must_use]
    pub fn tap_candidate(&self, now: u32) -> Option<(Point, u32)> {
        (self.pressed && !self.moved && !self.suppressed && !self.multi)
            .then(|| (self.down, now.wrapping_sub(self.down_at)))
    }

    /// While a single contact is down: where it is now (`isTouchHeldAt`), for dragging.
    #[must_use]
    pub fn held_at(&self) -> Option<Point> {
        (self.pressed && !self.suppressed && !self.multi).then_some(self.last)
    }

    /// How long the last contact lasted, from touch-down to lift (`lastTouchHeldMs`).
    #[must_use]
    pub fn last_held_ms(&self) -> u32 {
        self.held_ms
    }
}

/// `point` as fractions of a panel `width` by `height` (the SDK's `normalizeTouchPoint`, its
/// sizes being `raw_max - raw_min`, at least 1), each from 0 to 1.
#[must_use]
#[allow(clippy::cast_precision_loss, reason = "u16 values fit an f32 exactly")]
pub fn normalize(point: Point, width: u16, height: u16) -> (f32, f32) {
    let fraction = |v: u16, size: u16| (f32::from(v) / f32::from(size.max(1))).clamp(0.0, 1.0);
    (fraction(point.x, width), fraction(point.y, height))
}
