//! Exports for `src/OpdsServerStore` (see `rust/include/bembeyaz/store_opds_servers.h`):
//! `opds.json` read, made into the store's servers (their secrets decoded through the caller's
//! callback) and written by `bembeyaz_store::opds_servers`. The servers reach the caller one per
//! callback call, and written text through a `bb_store_write` callback, so no Rust buffer outlives
//! a call.

use crate::store::{BbHeader, BbPlainHeader, Decode, Text, Write, decoder, hand_over};
use bembeyaz_store::headers::HEADERS;
use bembeyaz_store::opds_servers::{MOST_SERVERS, Server, load, write};
use core::ffi::{c_char, c_void};

/// store_opds_servers.h's `bb_store_opds_server`.
#[repr(C)]
pub struct BbServer {
    name: Text,
    url: Text,
    username: Text,
    password_obf: Text,
    headers: [BbHeader; HEADERS],
    header_count: usize,
}

/// store_opds_servers.h's `bb_store_opds_loaded`.
#[repr(C)]
pub struct BbLoaded {
    name: Text,
    url: Text,
    username: Text,
    password: Text,
    headers: [BbPlainHeader; HEADERS],
    header_count: usize,
}

// store_opds_servers.h's structs, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let text = size_of::<Text>();
    let word = size_of::<usize>();
    assert!(size_of::<BbServer>() == 8 * text + word && offset_of!(BbServer, headers) == 4 * text);
    assert!(offset_of!(BbServer, header_count) == 8 * text);
    assert!(size_of::<BbLoaded>() == 8 * text + word && offset_of!(BbLoaded, headers) == 4 * text);
    assert!(offset_of!(BbLoaded, header_count) == 8 * text);
    // store_opds_servers.h's BB_STORE_OPDS_MOST_SERVERS.
    assert!(MOST_SERVERS == 8);
};

/// store_opds_servers.h's `bb_store_opds_loaded_server`.
type LoadedServer = unsafe extern "C" fn(*const BbLoaded, usize, *mut c_void);

impl BbServer {
    /// # Safety
    /// The texts are NULL or readable and outlive `'a`.
    unsafe fn view<'a>(&self) -> Server<&'a [u8]> {
        // SAFETY: as documented.
        unsafe {
            Server {
                name: self.name.bytes(),
                url: self.url.bytes(),
                username: self.username.bytes(),
                password_obf: self.password_obf.bytes(),
                headers: BbHeader::view(&self.headers, self.header_count),
            }
        }
    }
}

/// Hands `write_text` the `opds.json` text for the `count` servers at `servers`, in one call;
/// false when it could not be made (memory ran out) and `write_text` was not called.
///
/// # Safety
/// `servers` is NULL or `count` readable servers whose texts are NULL or readable; `write_text`
/// takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_opds_servers_write(
    servers: *const BbServer,
    count: usize,
    write_text: Option<Write>,
    context: *mut c_void,
) -> bool {
    // SAFETY: as documented.
    let servers = unsafe { crate::view(servers, count) };
    // SAFETY: as documented; the texts outlive this call.
    let view: alloc::vec::Vec<Server<&[u8]>> =
        servers.iter().map(|s| unsafe { s.view() }).collect();
    // SAFETY: as documented.
    unsafe { hand_over(write(&view).as_deref(), write_text, context) }
}

/// Makes what the store holds of `opds.json`'s text (`bembeyaz_store::opds_servers::load`, at
/// most `MOST_SERVERS` servers, the secrets decoded with `decode`) and hands each server to
/// `server`, in order, with how many there are; sets `*resave`. NULL when it was read, else
/// ArduinoJson's name for why not ("NoMemory" when memory ran out, decoding included), and only
/// `decode` may have been called. The plain secrets are wiped after the calls.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `decode` and `server` take `context`; `resave` is NULL
/// or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_opds_servers_load(
    text: *const c_char,
    len: usize,
    decode: Option<Decode>,
    server: Option<LoadedServer>,
    context: *mut c_void,
    resave: *mut bool,
) -> *const c_char {
    // SAFETY: as documented.
    let (text, decode) = unsafe {
        (
            crate::view(text.cast::<u8>(), len),
            decoder(decode, context),
        )
    };
    let loaded = match load(text, decode) {
        Ok(loaded) => loaded,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(server) = server {
        for s in &loaded.servers {
            let entry = BbLoaded {
                name: Text::of(&s.name),
                url: Text::of(&s.url),
                username: Text::of(&s.username),
                password: Text::of(&s.password),
                headers: BbPlainHeader::of(&s.headers),
                header_count: s.headers.len(),
            };
            // SAFETY: `entry`'s texts are readable for the call; the caller vouches for server and
            // context.
            unsafe { server(&raw const entry, loaded.servers.len(), context) };
        }
    }
    // SAFETY: as documented.
    if let Some(resave) = unsafe { resave.as_mut() } {
        *resave = loaded.resave;
    }
    // `loaded`'s plain secrets are wiped as it is dropped.
    core::ptr::null()
}
