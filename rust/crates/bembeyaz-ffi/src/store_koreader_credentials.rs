//! Exports for `lib/KOReaderSync/KOReaderCredentialStore` (see
//! `rust/include/bembeyaz/store_koreader_credentials.h`): `koreader.json` read and written by
//! `bembeyaz_store::koreader_credentials`, its password and header values as stored. What a file
//! holds reaches the caller in one callback call, and written text through a `bb_store_write`
//! callback, so no Rust buffer outlives a call.

use crate::store::{BbHeader, BbNumber, Text, Write, hand_over};
use bembeyaz_store::headers::HEADERS;
use bembeyaz_store::koreader_credentials::{Saved, read, write};
use core::ffi::{c_char, c_void};

/// store_koreader_credentials.h's `bb_store_koreader_stored`.
#[repr(C)]
pub struct BbStored {
    username: Text,
    password_obf: Text,
    password: Text,
    server_url: Text,
    headers: [BbHeader; HEADERS],
    header_count: usize,
    cfg_version: BbNumber,
    server_type: BbNumber,
    match_method: BbNumber,
    sync_behavior: BbNumber,
    send_metadata: bool,
    sync_clippings: bool,
}

/// store_koreader_credentials.h's `bb_store_koreader_saved`.
#[repr(C)]
pub struct BbSaved {
    username: Text,
    password_obf: Text,
    server_url: Text,
    headers: [BbHeader; HEADERS],
    header_count: usize,
    cfg_version: u8,
    server_type: u8,
    match_method: u8,
    sync_behavior: u8,
    send_metadata: bool,
    sync_clippings: bool,
}

// store_koreader_credentials.h's structs, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let (text, word) = (size_of::<Text>(), size_of::<usize>());
    assert!(
        offset_of!(BbStored, headers) == 4 * text && offset_of!(BbStored, header_count) == 8 * text
    );
    assert!(offset_of!(BbStored, cfg_version) == 8 * text + word);
    assert!(offset_of!(BbStored, sync_behavior) == 8 * text + word + 24);
    assert!(offset_of!(BbStored, sync_clippings) == 8 * text + word + 33);
    assert!(size_of::<BbStored>() == (8 * text + word + 34).div_ceil(word) * word);
    assert!(
        offset_of!(BbSaved, headers) == 3 * text && offset_of!(BbSaved, header_count) == 7 * text
    );
    assert!(offset_of!(BbSaved, cfg_version) == 7 * text + word);
    assert!(offset_of!(BbSaved, sync_clippings) == 7 * text + word + 5);
    assert!(size_of::<BbSaved>() == (7 * text + word + 6).div_ceil(word) * word);
};

/// store_koreader_credentials.h's `bb_store_koreader_stored_read`.
type ReadStored = unsafe extern "C" fn(*const BbStored, *mut c_void);

/// Hands `write_text` the `koreader.json` text for `saved`, in one call; false when it could not
/// be made (memory ran out) and `write_text` was not called.
///
/// # Safety
/// `saved` is NULL or readable, its texts NULL or readable; `write_text` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_koreader_credentials_write(
    saved: *const BbSaved,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let Some(s) = (unsafe { saved.as_ref() }) else {
        return false;
    };
    // SAFETY: as documented; the texts outlive this call.
    let view = unsafe {
        Saved {
            cfg_version: s.cfg_version,
            username: s.username.bytes(),
            password_obf: s.password_obf.bytes(),
            server_url: s.server_url.bytes(),
            server_type: s.server_type,
            match_method: s.match_method,
            send_metadata: s.send_metadata,
            sync_clippings: s.sync_clippings,
            sync_behavior: s.sync_behavior,
            headers: BbHeader::view(&s.headers, s.header_count),
        }
    };
    // SAFETY: as documented.
    unsafe { hand_over(write(&view).as_deref(), write_text, context) }
}

/// Reads `koreader.json`'s text; NULL when it was read (what it holds handed to `stored`), else
/// ArduinoJson's name for why not, and `stored` is not called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `stored` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_koreader_credentials_read(
    text: *const c_char,
    len: usize,
    stored: Option<ReadStored>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let s = match read(unsafe { crate::view(text.cast::<u8>(), len) }) {
        Ok(s) => s,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(stored) = stored {
        let entry = BbStored {
            username: Text::of(&s.username),
            password_obf: Text::of(&s.password_obf),
            password: Text::of(&s.password),
            server_url: Text::of(&s.server_url),
            headers: BbHeader::of(&s.headers),
            header_count: s.headers.count,
            cfg_version: BbNumber::of(s.cfg_version),
            server_type: BbNumber::of(s.server_type),
            match_method: BbNumber::of(s.match_method),
            sync_behavior: BbNumber::of(s.sync_behavior),
            send_metadata: s.send_metadata,
            sync_clippings: s.sync_clippings,
        };
        // SAFETY: `entry`'s texts are readable for the call; the caller vouches for stored and
        // context.
        unsafe { stored(&raw const entry, context) };
    }
    core::ptr::null()
}
