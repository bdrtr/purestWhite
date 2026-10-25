//! Exports for lib/hal's HalClock (see `rust/include/purestwhite/rtc.h`).

use crate::i2c::{Bus, CBus, Delay};
use purestwhite_rtc::DateTime;

/// `pw_rtc_time`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CTime {
    year: u16,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    weekday: u8,
}

// As rtc.h's, which HalClock.cpp asserts too.
const _: () = assert!(size_of::<CTime>() == 8 && align_of::<CTime>() == 2);

impl From<&CTime> for DateTime {
    fn from(t: &CTime) -> Self {
        DateTime {
            year: t.year,
            month: t.month,
            day: t.day,
            hour: t.hour,
            minute: t.minute,
            second: t.second,
            weekday: t.weekday,
        }
    }
}

/// # Safety
/// `bus` is NULL or a `pw_i2c` whose callbacks may be called during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_rtc_begin(bus: *const CBus, addr: u8) -> bool {
    // SAFETY: as documented.
    unsafe { bus.as_ref() }
        .is_some_and(|b| purestwhite_rtc::begin(&mut Bus(b), &mut Delay(b), addr))
}

/// # Safety
/// As `pw_rtc_begin`'s; `out` is NULL or a writable, aligned `pw_rtc_time` that nothing else uses
/// during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_rtc_now(bus: *const CBus, addr: u8, out: *mut CTime) -> bool {
    // SAFETY: as documented.
    let (Some(b), Some(out)) = (unsafe { bus.as_ref() }, unsafe { out.as_mut() }) else {
        return false;
    };
    let Some(t) = purestwhite_rtc::now(&mut Bus(b), addr) else {
        return false;
    };
    *out = CTime {
        year: t.year,
        month: t.month,
        day: t.day,
        hour: t.hour,
        minute: t.minute,
        second: t.second,
        weekday: t.weekday,
    };
    true
}

/// # Safety
/// As `pw_rtc_begin`'s; `time` is NULL or a readable, aligned `pw_rtc_time`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_rtc_set(bus: *const CBus, addr: u8, time: *const CTime) -> bool {
    // SAFETY: as documented.
    let (Some(b), Some(t)) = (unsafe { bus.as_ref() }, unsafe { time.as_ref() }) else {
        return false;
    };
    purestwhite_rtc::set(&mut Bus(b), addr, &t.into())
}

/// # Safety
/// `time` is NULL or a readable, aligned `pw_rtc_time`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_rtc_epoch(time: *const CTime) -> i64 {
    // SAFETY: as documented.
    unsafe { time.as_ref() }.map_or(0, |t| purestwhite_rtc::epoch_from_utc(&t.into()))
}
