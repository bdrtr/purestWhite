//! Exports for `src/WifiCredentialStore` (see `rust/include/bembeyaz/store_wifi_credentials.h`):
//! `wifi.json` read, decided on (`keep`, the passwords decoded through the caller's callback) and
//! written by `bembeyaz_store::wifi_credentials`. What a file gives reaches the caller one network
//! per callback call, and written text through a `bb_store_write` callback, so no Rust buffer
//! outlives a call.

use crate::store::{Decode, Text, Write, decoder, hand_over};
use bembeyaz_store::Error;
use bembeyaz_store::wifi_credentials::{
    Checksum, Dropped, Limits, MOST_NETWORKS, MOST_PASSWORD, Networks, Saved, Why, keep, write,
};
use core::ffi::{c_char, c_void};

/// store_wifi_credentials.h's `bb_store_wifi_kept`.
#[repr(C)]
pub struct BbKept {
    ssid: Text,
    password: Text,
}

/// store_wifi_credentials.h's `bb_store_wifi_dropped`.
#[repr(C)]
pub struct BbDropped {
    ssid: Text,
    why: u8,
    checksum: u8,
    length_mismatch: bool,
    length: u32,
    decoded: usize,
}

/// store_wifi_credentials.h's `bb_store_wifi_keep_calls`.
#[repr(C)]
pub struct BbKeepCalls {
    decode: Option<Decode>,
    last_connected: Option<Write>,
    kept: Option<unsafe extern "C" fn(*const BbKept, *mut c_void)>,
    dropped: Option<unsafe extern "C" fn(*const BbDropped, *mut c_void)>,
}

/// store_wifi_credentials.h's `bb_store_wifi_saved`.
#[repr(C)]
pub struct BbSaved {
    ssid: Text,
    password_obf: Text,
    password: Text,
}

// store_wifi_credentials.h's structs, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let (text, word) = (size_of::<Text>(), size_of::<usize>());
    assert!(size_of::<BbKept>() == 2 * text && offset_of!(BbKept, password) == text);
    assert!(offset_of!(BbDropped, why) == text && offset_of!(BbDropped, length) == text + 4);
    assert!(offset_of!(BbDropped, decoded) == text + 8);
    assert!(size_of::<BbDropped>() == text + 8 + word);
    assert!(size_of::<BbKeepCalls>() == 4 * word);
    assert!(size_of::<BbSaved>() == 3 * text && offset_of!(BbSaved, password) == 2 * text);
    // store_wifi_credentials.h's limits, and its `bb_store_wifi_dropped` codes.
    assert!(MOST_NETWORKS == 8 && MOST_PASSWORD == 64);
    assert!(INVALID_LENGTH == 0 && OVERSIZED_LENGTH == 1 && OVERSIZED_PASSWORD == 2);
    assert!(CORRUPTED == 3 && CHECKSUM_FINE == 0 && CHECKSUM_MISMATCH == 1);
    assert!(CHECKSUM_INVALID == 2);
};

/// Hands `write_text` the `wifi.json` text for `last_connected_ssid` and the `count` credentials
/// at `saved`, in one call; false when it could not be made (memory ran out) and `write_text` was
/// not called.
///
/// # Safety
/// The texts are NULL or readable; `saved` is NULL or `count` readable credentials; `write_text`
/// takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_wifi_credentials_write(
    last_connected_ssid: Text,
    saved: *const BbSaved,
    count: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let saved = unsafe { crate::view(saved, count) };
    let view: alloc::vec::Vec<Saved<&[u8]>> = saved
        .iter()
        .map(|s| Saved {
            // SAFETY: as documented; the texts outlive this call.
            ssid: unsafe { s.ssid.bytes() },
            // SAFETY: as above.
            password_obf: unsafe { s.password_obf.bytes() },
            // SAFETY: as above.
            password: unsafe { s.password.bytes() },
        })
        .collect();
    // SAFETY: as documented.
    let last = unsafe { last_connected_ssid.bytes() };
    // SAFETY: as documented.
    unsafe { hand_over(write(last, &view).as_deref(), write_text, context) }
}

/// store_wifi_credentials.h's `bb_store_wifi_dropped` codes.
const INVALID_LENGTH: u8 = 0;
const OVERSIZED_LENGTH: u8 = 1;
const OVERSIZED_PASSWORD: u8 = 2;
const CORRUPTED: u8 = 3;
const CHECKSUM_FINE: u8 = 0;
const CHECKSUM_MISMATCH: u8 = 1;
const CHECKSUM_INVALID: u8 = 2;

impl BbDropped {
    fn of(dropped: &Dropped<'_>) -> BbDropped {
        let mut out = BbDropped {
            ssid: Text::of(dropped.ssid),
            why: INVALID_LENGTH,
            checksum: CHECKSUM_FINE,
            length_mismatch: false,
            length: 0,
            decoded: 0,
        };
        match dropped.why {
            Why::InvalidLength => {}
            Why::OversizedLength(length) => (out.why, out.length) = (OVERSIZED_LENGTH, length),
            Why::OversizedPassword => out.why = OVERSIZED_PASSWORD,
            Why::Corrupted { length, checksum } => {
                out.why = CORRUPTED;
                if let Some((stored, decoded)) = length {
                    (out.length_mismatch, out.length, out.decoded) = (true, stored, decoded);
                }
                out.checksum = match checksum {
                    None => CHECKSUM_FINE,
                    Some(Checksum::Mismatch) => CHECKSUM_MISMATCH,
                    Some(Checksum::Invalid) => CHECKSUM_INVALID,
                };
            }
        }
        out
    }
}

/// Decides on `wifi.json`'s text as the store does (`bembeyaz_store::wifi_credentials::keep`, at
/// most `MOST_NETWORKS` networks, passwords of at most `MOST_PASSWORD` bytes): `calls`' `dropped`
/// hears of each network dropped as it is judged, then `last_connected` of the network connected
/// last and `kept` of each network kept, in the file's order; `*resave` is set. NULL when it was
/// read, else ArduinoJson's name for why not ("NoMemory" when memory ran out, decoding included;
/// "InvalidInput" when `calls` is NULL), and only `decode` and `dropped` may have been called. The
/// plain passwords are wiped after the calls.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `calls` is NULL or readable, and its callbacks take
/// `context`; `resave` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_wifi_credentials_keep(
    text: *const c_char,
    len: usize,
    calls: *const BbKeepCalls,
    context: *mut c_void,
    resave: *mut bool,
) -> *const c_char {
    // SAFETY: as documented.
    let (text, calls) = unsafe { (crate::view(text.cast::<u8>(), len), calls.as_ref()) };
    let Some(calls) = calls else {
        return Error::InvalidInput.c_name().as_ptr();
    };
    let limits = Limits {
        networks: MOST_NETWORKS,
        password: MOST_PASSWORD,
    };
    // SAFETY: the caller vouches for decode and context.
    let decode = unsafe { decoder(calls.decode, context) };
    let dropped = |one: &Dropped<'_>| {
        if let Some(dropped) = calls.dropped {
            let entry = BbDropped::of(one);
            // SAFETY: `entry`'s text is readable for the call; the caller vouches for the rest.
            unsafe { dropped(&raw const entry, context) };
        }
    };
    let networks = match keep(text, limits, decode, dropped) {
        Ok(networks) => networks,
        Err(error) => return error.c_name().as_ptr(),
    };
    // SAFETY: as documented; every view is readable for its call.
    unsafe { hand_over_networks(&networks, calls, context) };
    // SAFETY: as documented.
    if let Some(resave) = unsafe { resave.as_mut() } {
        *resave = networks.resave;
    }
    // `networks`' plain passwords are wiped as it is dropped.
    core::ptr::null()
}

/// `networks` handed to `calls`, as `bb_store_wifi_credentials_keep` documents.
///
/// # Safety
/// `calls`' callbacks take `context`.
unsafe fn hand_over_networks(networks: &Networks, calls: &BbKeepCalls, context: *mut c_void) {
    let last = Text::of(&networks.last_connected_ssid);
    if let Some(last_connected) = calls.last_connected {
        // SAFETY: `last` is NULL with 0 or readable for the call; the caller vouches for the rest.
        unsafe { last_connected(last.data(), last.len(), context) };
    }
    for one in &networks.kept {
        let entry = BbKept {
            ssid: Text::of(&one.ssid),
            password: Text::of(&one.password),
        };
        if let Some(kept) = calls.kept {
            // SAFETY: `entry`'s texts are readable for the call; the caller vouches for the rest.
            unsafe { kept(&raw const entry, context) };
        }
    }
}
