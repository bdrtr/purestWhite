//! Exports for `src/WifiCredentialStore` (see `rust/include/bembeyaz/store_wifi_credentials.h`):
//! `wifi.json` read and written by `bembeyaz_store::wifi_credentials`, its passwords as stored.
//! What a file holds reaches the caller one item per callback call, and written text through a
//! `bb_store_write` callback, so no Rust buffer outlives a call.

use crate::store::{BbNumber, Text, Write, hand_over};
use bembeyaz_store::wifi_credentials::{Saved, read_each, write};
use core::ffi::{c_char, c_void};
use core::ops::ControlFlow;

/// store_wifi_credentials.h's `bb_store_wifi_stored`.
#[repr(C)]
pub struct BbStored {
    ssid: Text,
    password_obf: Text,
    password: Text,
    password_len: BbNumber,
    password_crc32: BbNumber,
}

/// store_wifi_credentials.h's `bb_store_wifi_saved`.
#[repr(C)]
pub struct BbSaved {
    ssid: Text,
    password_obf: Text,
    password_len: u64,
    password_crc32: u32,
}

// store_wifi_credentials.h's structs, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let text = size_of::<Text>();
    assert!(
        size_of::<BbStored>() == 3 * text + 16
            && offset_of!(BbStored, password_len) == 3 * text
            && offset_of!(BbStored, password_crc32) == 3 * text + 8
    );
    assert!(
        size_of::<BbSaved>() == 2 * text + 16
            && offset_of!(BbSaved, password_len) == 2 * text
            && offset_of!(BbSaved, password_crc32) == 2 * text + 8
    );
};

/// store_wifi_credentials.h's `bb_store_wifi_stored_read`.
type ReadStored = unsafe extern "C" fn(*const BbStored, usize, *mut c_void);

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
            password_len: s.password_len,
            password_crc32: s.password_crc32,
        })
        .collect();
    // SAFETY: as documented.
    let last = unsafe { last_connected_ssid.bytes() };
    // SAFETY: as documented.
    unsafe { hand_over(write(last, &view).as_deref(), write_text, context) }
}

/// Reads `wifi.json`'s text; NULL when it was read (the network connected last handed to
/// `last_connected`, then every item of "credentials" to `stored`, with how many there are), else
/// ArduinoJson's name for why not, and neither is called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `last_connected` and `stored` take `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_wifi_credentials_read(
    text: *const c_char,
    len: usize,
    last_connected: Option<Write>,
    stored: Option<ReadStored>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let text = unsafe { crate::view(text.cast::<u8>(), len) };
    let handed = read_each(
        text,
        |last| {
            if let Some(last_connected) = last_connected {
                // SAFETY: `last` is readable for the call; the caller vouches for the rest.
                unsafe { last_connected(last.as_ptr().cast(), last.len(), context) };
            }
        },
        |c, count| {
            if let Some(stored) = stored {
                let entry = BbStored {
                    ssid: Text::of(c.ssid),
                    password_obf: Text::of(c.password_obf),
                    password: Text::of(c.password),
                    password_len: BbNumber::of(c.password_len),
                    password_crc32: BbNumber::of(c.password_crc32),
                };
                // SAFETY: `entry`'s texts are readable for the call; the caller vouches for stored
                // and context.
                unsafe { stored(&raw const entry, count, context) };
            }
            ControlFlow::Continue(())
        },
    );
    match handed {
        Ok(()) => core::ptr::null(),
        Err(error) => error.c_name().as_ptr(),
    }
}
