//! Exports for `lib/KOReaderSync/KOReaderCredentialStore` (see
//! `rust/include/bembeyaz/store_koreader_credentials.h`): `koreader.json` read and written by
//! `bembeyaz_store::koreader_credentials`, and what the store makes of it, by
//! `bembeyaz_store::koreader_settings`, its secrets decoded through the caller's callback. The
//! settings reach the caller in one callback call, and text through a `bb_store_write` callback,
//! so no Rust buffer outlives a call.

use crate::store::{BbHeader, Decode, Text, Write, decoder, hand_over};
use bembeyaz_store::headers::HEADERS;
use bembeyaz_store::koreader_credentials::{Saved, write};
use bembeyaz_store::koreader_settings::{
    Settings, base_url_parts, load, match_method, server_type, sync_behavior,
};
use core::ffi::{c_char, c_void};

// store_koreader_credentials.h's values.
const _: () = {
    assert!(server_type::CROSSPOINT == 0 && server_type::KOSYNC == 1 && server_type::OTHER == 2);
    assert!(match_method::FILENAME == 0 && match_method::BINARY == 1);
    assert!(sync_behavior::ASK_EVERY_TIME == 0 && sync_behavior::SMART == 1);
};

/// store_koreader_credentials.h's `bb_store_koreader_header`.
#[repr(C)]
struct BbPlainHeader {
    name: Text,
    value: Text,
}

/// store_koreader_credentials.h's `bb_store_koreader_settings`.
#[repr(C)]
pub struct BbSettings {
    username: Text,
    password: Text,
    server_url: Text,
    headers: [BbPlainHeader; HEADERS],
    header_count: usize,
    server_type: u8,
    match_method: u8,
    sync_behavior: u8,
    send_metadata: bool,
    sync_clippings: bool,
    resave: bool,
    pinned_legacy_url: bool,
    server_type_reset: u8,
    match_method_reset: u8,
    sync_behavior_reset: u8,
}

/// store_koreader_credentials.h's `bb_store_koreader_saved`.
#[repr(C)]
pub struct BbSaved {
    username: Text,
    password_obf: Text,
    server_url: Text,
    headers: [BbHeader; HEADERS],
    header_count: usize,
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
    assert!(size_of::<BbPlainHeader>() == 2 * text && offset_of!(BbPlainHeader, value) == text);
    assert!(offset_of!(BbSettings, headers) == 3 * text);
    assert!(offset_of!(BbSettings, header_count) == 7 * text);
    assert!(offset_of!(BbSettings, server_type) == 7 * text + word);
    assert!(offset_of!(BbSettings, sync_behavior_reset) == 7 * text + word + 9);
    assert!(size_of::<BbSettings>() == (7 * text + word + 10).div_ceil(word) * word);
    assert!(
        offset_of!(BbSaved, headers) == 3 * text && offset_of!(BbSaved, header_count) == 7 * text
    );
    assert!(offset_of!(BbSaved, server_type) == 7 * text + word);
    assert!(offset_of!(BbSaved, sync_clippings) == 7 * text + word + 4);
    assert!(size_of::<BbSaved>() == (7 * text + word + 5).div_ceil(word) * word);
};

/// store_koreader_credentials.h's `bb_store_koreader_settled`.
type Settled = unsafe extern "C" fn(*const BbSettings, *mut c_void);

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

/// Reads `koreader.json`'s text and hands `settled` the settings it gives, its secrets decoded with
/// `decode`; NULL when it was read, else ArduinoJson's name for why not, and `settled` is not
/// called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `decode` and `settled` take `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_koreader_credentials_settle(
    text: *const c_char,
    len: usize,
    decode: Option<Decode>,
    settled: Option<Settled>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let (text, decode) = unsafe {
        (
            crate::view(text.cast::<u8>(), len),
            decoder(decode, context),
        )
    };
    let settings = match load(text, decode) {
        Ok(settings) => settings,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(settled) = settled {
        let handed = BbSettings::of(&settings);
        // SAFETY: `handed`'s texts are readable for the call; the caller vouches for settled and
        // context.
        unsafe { settled(&raw const handed, context) };
    }
    // `settings`' plain secrets are wiped as it is dropped.
    core::ptr::null()
}

impl BbSettings {
    /// A view of `s`, valid while it is.
    fn of(s: &Settings) -> BbSettings {
        let header = |slot: usize| {
            s.headers.get(slot).map_or(
                BbPlainHeader {
                    name: Text::of(&[]),
                    value: Text::of(&[]),
                },
                |h| BbPlainHeader {
                    name: Text::of(&h.name),
                    value: Text::of(&h.value),
                },
            )
        };
        BbSettings {
            username: Text::of(&s.username),
            password: Text::of(&s.password),
            server_url: Text::of(&s.server_url),
            headers: core::array::from_fn(header),
            header_count: s.headers.len(),
            server_type: s.server_type,
            match_method: s.match_method,
            sync_behavior: s.sync_behavior,
            send_metadata: s.send_metadata,
            sync_clippings: s.sync_clippings,
            resave: s.resave,
            pinned_legacy_url: s.notes.pinned_legacy_url,
            server_type_reset: s.notes.server_type_reset.unwrap_or(0),
            match_method_reset: s.notes.match_method_reset.unwrap_or(0),
            sync_behavior_reset: s.notes.sync_behavior_reset.unwrap_or(0),
        }
    }
}

/// Hands `write_text` the base URL for the server URL `url` (`getBaseUrl`), in its pieces (one or
/// two calls).
///
/// # Safety
/// `url` is NULL or `len` readable bytes; `write_text` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_koreader_base_url(
    url: *const c_char,
    len: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) {
    // SAFETY: as documented.
    let url = unsafe { crate::view(url.cast::<u8>(), len) };
    let Some(write_text) = write_text else {
        return;
    };
    for piece in base_url_parts(url).into_iter().filter(|p| !p.is_empty()) {
        // SAFETY: `piece` is readable for the call; the caller vouches for write_text and context.
        unsafe { write_text(piece.as_ptr().cast(), piece.len(), context) };
    }
}
