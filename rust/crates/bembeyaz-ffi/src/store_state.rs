//! Exports for `src/CrossPointState` (see `rust/include/bembeyaz/store_state.h`): `state.json`
//! read and written by `bembeyaz_store::state`, and its recent-image rings. Text goes to the
//! caller's `bb_store_write` callback, so no Rust buffer crosses the boundary.

use crate::store::{Write, hand_over};
use bembeyaz_store::state::{RECENT, Ring, State, read, write};
use core::ffi::{c_char, c_void};

/// store_state.h's `bb_store_state_ring`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BbRing {
    images: [u16; RECENT as usize],
    pos: u8,
    fill: u8,
}

/// store_state.h's `bb_store_state`: every field except the open book's path.
#[repr(C)]
pub struct BbState {
    recent_sleep: BbRing,
    recent_overlay_sleep: BbRing,
    reader_activity_load_count: u8,
    last_sleep_from_reader: bool,
    show_boot_screen: bool,
}

// The layout of store_state.h's structs. They hold no pointers, so it is the same on the
// ESP32-S3 and the host.
const _: () = {
    use core::mem::offset_of;
    assert!(size_of::<BbRing>() == 34 && offset_of!(BbRing, pos) == 32);
    assert!(
        size_of::<BbState>() == 72
            && offset_of!(BbState, recent_overlay_sleep) == 34
            && offset_of!(BbState, reader_activity_load_count) == 68
            && offset_of!(BbState, show_boot_screen) == 70
    );
};

impl From<BbRing> for Ring {
    fn from(r: BbRing) -> Ring {
        Ring {
            images: r.images,
            pos: r.pos,
            fill: r.fill,
        }
    }
}

impl From<Ring> for BbRing {
    fn from(r: Ring) -> BbRing {
        BbRing {
            images: r.images,
            pos: r.pos,
            fill: r.fill,
        }
    }
}

/// Hands `write_text` the `state.json` text for `state` and `path` (the open book), in one call;
/// false when it could not be made (memory ran out) and `write_text` was not called.
///
/// # Safety
/// `state` is NULL or readable; `path` is NULL or `path_len` readable bytes; `write_text` takes
/// `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_state_write(
    state: *const BbState,
    path: *const c_char,
    path_len: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let Some(s) = (unsafe { state.as_ref() }) else {
        return false;
    };
    let state = State {
        // SAFETY: as documented.
        open_epub_path: unsafe { crate::view(path.cast::<u8>(), path_len) }.to_vec(),
        recent_sleep: s.recent_sleep.into(),
        recent_overlay_sleep: s.recent_overlay_sleep.into(),
        reader_activity_load_count: s.reader_activity_load_count,
        last_sleep_from_reader: s.last_sleep_from_reader,
        show_boot_screen: s.show_boot_screen,
    };
    // SAFETY: as documented.
    unsafe { hand_over(write(&state).as_deref(), write_text, context) }
}

/// Reads `state.json`'s text; NULL when it was read (the path handed to `path`, the rest in
/// `out`), else ArduinoJson's name for why not, and `out` is left as it was.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `out` is NULL or writable; `path` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_state_read(
    text: *const c_char,
    len: usize,
    out: *mut BbState,
    path: Option<Write>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let state = match read(unsafe { crate::view(text.cast::<u8>(), len) }) {
        Ok(state) => state,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(path) = path {
        let bytes = &state.open_epub_path;
        // SAFETY: `bytes` is readable for the call; the caller vouches for path and context.
        unsafe { path(bytes.as_ptr().cast(), bytes.len(), context) };
    }
    // SAFETY: as documented.
    if let Some(out) = unsafe { out.as_mut() } {
        *out = BbState {
            recent_sleep: state.recent_sleep.into(),
            recent_overlay_sleep: state.recent_overlay_sleep.into(),
            reader_activity_load_count: state.reader_activity_load_count,
            last_sleep_from_reader: state.last_sleep_from_reader,
            show_boot_screen: state.show_boot_screen,
        };
    }
    core::ptr::null()
}

/// Whether `image` is among the ring's last `count` (at most its fill) images.
///
/// # Safety
/// `ring` is NULL or readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_state_is_recent(
    ring: *const BbRing,
    image: u16,
    count: u8,
) -> bool {
    // SAFETY: as documented.
    unsafe { ring.as_ref() }.is_some_and(|r| Ring::from(*r).is_recent(image, count))
}

/// Remembers `image` as the ring's last.
///
/// # Safety
/// `ring` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_state_push_recent(ring: *mut BbRing, image: u16) {
    // SAFETY: as documented.
    if let Some(r) = unsafe { ring.as_mut() } {
        let mut next = Ring::from(*r);
        next.push(image);
        *r = next.into();
    }
}
