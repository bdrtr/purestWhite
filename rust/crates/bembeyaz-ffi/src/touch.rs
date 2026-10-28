//! Exports for test/gt911_touch (see `rust/include/purestwhite/touch.h`).

use alloc::boxed::Box;
use purestwhite_gt911::{Contact, Frame, MAX_CONTACTS, Reading};
use purestwhite_touch::{Point, Touch};

/// `pw_touch_point`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CPoint {
    x: u16,
    y: u16,
}

/// `pw_touch_events`.
#[repr(C)]
#[derive(Default)]
#[allow(clippy::struct_excessive_bools, reason = "the C struct's flags")]
pub struct CEvents {
    pressed_at: CPoint,
    tap_at: CPoint,
    swipe_start: CPoint,
    swipe_end: CPoint,
    long_press_at: CPoint,
    pressed: bool,
    released: bool,
    tap: bool,
    swipe: bool,
    long_press: bool,
    home_pressed: bool,
    home_tapped: bool,
    home_long_press: bool,
    activity: bool,
}

/// `pw_gt911_contact` and `pw_gt911_frame`, as gt911.rs exports them.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CContact {
    id: u8,
    x: u16,
    y: u16,
}

#[repr(C)]
pub struct CFrame {
    home_key: bool,
    reported: u8,
    count: u8,
    contacts: [CContact; MAX_CONTACTS],
}

// As touch.h's and gt911.h's.
const _: () = assert!(size_of::<CPoint>() == 4 && size_of::<CEvents>() == 30);
const _: () = assert!(align_of::<CEvents>() == 2);
const _: () = assert!(size_of::<CContact>() == 6 && size_of::<CFrame>() == 28);

const NOT_READY: u8 = 1;
const FRAME: u8 = 2;
const CONTACTS_FAILED: u8 = 3;

impl From<Point> for CPoint {
    fn from(p: Point) -> Self {
        CPoint { x: p.x, y: p.y }
    }
}

/// A `pw_touch_point` out-parameter, written when not NULL.
///
/// # Safety
/// `to` is NULL or a writable, aligned `pw_touch_point`.
unsafe fn put(to: *mut CPoint, point: Point) {
    // SAFETY: as documented.
    if let Some(to) = unsafe { to.as_mut() } {
        *to = point.into();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn pw_touch_new() -> *mut Touch {
    Box::into_raw(Box::new(Touch::new()))
}

/// # Safety
/// `touch` is NULL or from `pw_touch_new`, not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_free(touch: *mut Touch) {
    if !touch.is_null() {
        // SAFETY: as documented.
        drop(unsafe { Box::from_raw(touch) });
    }
}

/// # Safety
/// `touch` is NULL or a live `pw_touch` that nothing else uses during the call; `frame` is NULL
/// or a valid `pw_gt911_frame` (read only for FRAME and CONTACTS_FAILED, where NULL is FAILED);
/// `out` is NULL or a writable, aligned `pw_touch_events`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_update(
    touch: *mut Touch,
    now: u32,
    kind: u8,
    frame: *const CFrame,
    out: *mut CEvents,
) {
    // SAFETY: as documented.
    let (Some(touch), Some(out)) = (unsafe { touch.as_mut() }, unsafe { out.as_mut() }) else {
        return;
    };
    // SAFETY: as documented.
    let frame = unsafe { frame.as_ref() };
    let reading = match (kind, frame) {
        (NOT_READY, _) => Reading::NotReady,
        (FRAME, Some(f)) => {
            let mut contacts = [Contact::default(); MAX_CONTACTS];
            for (to, from) in contacts.iter_mut().zip(f.contacts) {
                *to = Contact {
                    id: from.id,
                    x: from.x,
                    y: from.y,
                };
            }
            Reading::Frame(Frame {
                home_key: f.home_key,
                reported: f.reported,
                count: f.count,
                contacts,
            })
        }
        (CONTACTS_FAILED, Some(f)) => Reading::ContactsFailed {
            home_key: f.home_key,
        },
        _ => Reading::Failed,
    };
    let e = touch.update(now, &reading);
    let mut c = CEvents {
        pressed: e.pressed.is_some(),
        released: e.released,
        tap: e.tap.is_some(),
        swipe: e.swipe.is_some(),
        long_press: e.long_press.is_some(),
        home_pressed: e.home_pressed,
        home_tapped: e.home_tapped,
        home_long_press: e.home_long_press,
        activity: e.activity,
        ..CEvents::default()
    };
    if let Some(p) = e.pressed {
        c.pressed_at = p.into();
    }
    if let Some(p) = e.tap {
        c.tap_at = p.into();
    }
    if let Some((start, end)) = e.swipe {
        c.swipe_start = start.into();
        c.swipe_end = end.into();
    }
    if let Some(p) = e.long_press {
        c.long_press_at = p.into();
    }
    *out = c;
}

/// # Safety
/// `touch` is NULL or a live `pw_touch` that nothing else uses during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_suppress(touch: *mut Touch) {
    // SAFETY: as documented.
    if let Some(touch) = unsafe { touch.as_mut() } {
        touch.suppress();
    }
}

/// # Safety
/// `touch` is NULL or a live `pw_touch`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_is_pressed(touch: *const Touch) -> bool {
    // SAFETY: as documented.
    unsafe { touch.as_ref() }.is_some_and(Touch::is_pressed)
}

/// # Safety
/// `touch` is NULL or a live `pw_touch`; `at` is NULL or a writable, aligned `pw_touch_point`;
/// `held_ms` is NULL or a writable, aligned `uint32_t`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_tap_candidate(
    touch: *const Touch,
    now: u32,
    at: *mut CPoint,
    held_ms: *mut u32,
) -> bool {
    // SAFETY: as documented.
    let Some((point, held)) = unsafe { touch.as_ref() }.and_then(|t| t.tap_candidate(now)) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe {
        put(at, point);
        if let Some(h) = held_ms.as_mut() {
            *h = held;
        }
    }
    true
}

/// # Safety
/// `touch` is NULL or a live `pw_touch`; `at` is NULL or a writable, aligned `pw_touch_point`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_held_at(touch: *const Touch, at: *mut CPoint) -> bool {
    // SAFETY: as documented.
    let Some(point) = unsafe { touch.as_ref() }.and_then(Touch::held_at) else {
        return false;
    };
    // SAFETY: as documented.
    unsafe { put(at, point) };
    true
}

/// # Safety
/// `touch` is NULL or a live `pw_touch`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_touch_last_held_ms(touch: *const Touch) -> u32 {
    // SAFETY: as documented.
    unsafe { touch.as_ref() }.map_or(0, Touch::last_held_ms)
}
