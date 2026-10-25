//! Exports for test/gt911_touch (see `rust/include/purestwhite/gt911.h`).

use crate::i2c::{Bus, CBus, Delay, block_on};
use core::ffi::c_void;
use purestwhite_gt911::{Config, Reading, ResetPins};

/// `pw_gt911_config`.
#[repr(C)]
pub struct CConfig {
    addr: u8,
    alt_addr: u8,
    raw_min_x: u16,
    raw_max_x: u16,
    raw_min_y: u16,
    raw_max_y: u16,
    swap_xy: bool,
    flip_x: bool,
    flip_y: bool,
    coords_at_byte0: bool,
}

/// `pw_gt911_pins`.
#[repr(C)]
pub struct CPins {
    context: *mut c_void,
    set_reset: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    drive_int: Option<unsafe extern "C" fn(*mut c_void, bool)>,
    release_int: Option<unsafe extern "C" fn(*mut c_void)>,
}

/// `pw_gt911_contact`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CContact {
    id: u8,
    x: u16,
    y: u16,
}

/// `pw_gt911_frame`.
#[repr(C)]
pub struct CFrame {
    home_key: bool,
    reported: u8,
    count: u8,
    contacts: [CContact; purestwhite_gt911::MAX_CONTACTS],
}

// As gt911.h's.
const _: () = assert!(size_of::<CConfig>() == 14 && align_of::<CConfig>() == 2);
const _: () = assert!(size_of::<CPins>() == 4 * size_of::<usize>());
const _: () = assert!(size_of::<CContact>() == 6 && size_of::<CFrame>() == 28);

const FAILED: u8 = 0;
const NOT_READY: u8 = 1;
const FRAME: u8 = 2;
const CONTACTS_FAILED: u8 = 3;

impl From<&CConfig> for Config {
    fn from(c: &CConfig) -> Self {
        Config {
            addr: c.addr,
            alt_addr: c.alt_addr,
            raw_min_x: c.raw_min_x,
            raw_max_x: c.raw_max_x,
            raw_min_y: c.raw_min_y,
            raw_max_y: c.raw_max_y,
            swap_xy: c.swap_xy,
            flip_x: c.flip_x,
            flip_y: c.flip_y,
            coords_at_byte0: c.coords_at_byte0,
        }
    }
}

/// A `pw_gt911_pins` whose callbacks may be called with its context during the call.
struct Pins<'a>(&'a CPins);

impl ResetPins for Pins<'_> {
    fn set_reset(&mut self, high: bool) {
        if let Some(f) = self.0.set_reset {
            // SAFETY: the callback takes its context.
            unsafe { f(self.0.context, high) }
        }
    }
    fn drive_int(&mut self, high: bool) {
        if let Some(f) = self.0.drive_int {
            // SAFETY: as `set_reset`.
            unsafe { f(self.0.context, high) }
        }
    }
    fn release_int(&mut self) {
        if let Some(f) = self.0.release_int {
            // SAFETY: as `set_reset`.
            unsafe { f(self.0.context) }
        }
    }
}

/// # Safety
/// `bus` is NULL or a `pw_i2c`, and `pins` NULL or a `pw_gt911_pins`, whose callbacks may be
/// called during the call; `config` is NULL or a valid `pw_gt911_config`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_gt911_begin(
    bus: *const CBus,
    pins: *const CPins,
    config: *const CConfig,
) -> u8 {
    // SAFETY: as documented.
    let (Some(bus), Some(config)) = (unsafe { bus.as_ref() }, unsafe { config.as_ref() }) else {
        return 0;
    };
    // SAFETY: as documented.
    let mut pins = unsafe { pins.as_ref() }.map(Pins);
    let found = block_on(purestwhite_gt911::begin(
        &mut Bus(bus),
        &mut Delay(bus),
        pins.as_mut(),
        &Config::from(config),
    ));
    found.flatten().unwrap_or(0)
}

/// # Safety
/// `bus` is NULL or a `pw_i2c` whose callbacks may be called during the call; `config` is NULL
/// or a valid `pw_gt911_config`; `out` is NULL or a writable, aligned `pw_gt911_frame` that
/// nothing else uses during the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_gt911_read(
    bus: *const CBus,
    addr: u8,
    config: *const CConfig,
    out: *mut CFrame,
) -> u8 {
    // SAFETY: as documented.
    let (Some(bus), Some(config)) = (unsafe { bus.as_ref() }, unsafe { config.as_ref() }) else {
        return FAILED;
    };
    if out.is_null() {
        return FAILED;
    }
    let Some(reading) = block_on(purestwhite_gt911::read(
        &mut Bus(bus),
        addr,
        &Config::from(config),
    )) else {
        return FAILED;
    };
    // SAFETY: as documented; taken after the bus callbacks ran.
    let Some(out) = (unsafe { out.as_mut() }) else {
        return FAILED;
    };
    match reading {
        Reading::Failed => FAILED,
        Reading::NotReady => NOT_READY,
        Reading::ContactsFailed { home_key } => {
            out.home_key = home_key;
            CONTACTS_FAILED
        }
        Reading::Frame(frame) => {
            out.home_key = frame.home_key;
            out.reported = frame.reported;
            out.count = frame.count;
            for (to, from) in out.contacts.iter_mut().zip(frame.contacts) {
                *to = CContact {
                    id: from.id,
                    x: from.x,
                    y: from.y,
                };
            }
            FRAME
        }
    }
}
