//! Exports for `src/OpdsServerStore` (see `rust/include/bembeyaz/store_opds_servers.h`):
//! `opds.json` read and written by `bembeyaz_store::opds_servers`, its obfuscated texts as stored.
//! Read servers reach the caller one per callback call, and written text through a
//! `bb_store_write` callback, so no Rust buffer outlives a call.

use crate::store::{BbHeader, Text, Write, hand_over};
use bembeyaz_store::headers::HEADERS;
use bembeyaz_store::opds_servers::{Server, read, write};
use core::ffi::{c_char, c_void};

/// store_opds_servers.h's `bb_store_opds_server`.
#[repr(C)]
pub struct BbServer {
    name: Text,
    url: Text,
    username: Text,
    password_obf: Text,
    password: Text,
    headers: [BbHeader; HEADERS],
    header_count: usize,
}

// store_opds_servers.h's structs, on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    use core::mem::offset_of;
    let text = size_of::<Text>();
    assert!(
        size_of::<BbServer>() == 9 * text + size_of::<usize>()
            && offset_of!(BbServer, password) == 4 * text
            && offset_of!(BbServer, headers) == 5 * text
            && offset_of!(BbServer, header_count) == 9 * text
    );
};

/// store_opds_servers.h's `bb_store_opds_server_read`.
type ReadServer = unsafe extern "C" fn(*const BbServer, usize, *mut c_void);

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
                password: self.password.bytes(),
                headers: BbHeader::view(&self.headers, self.header_count),
            }
        }
    }

    fn of(s: &Server) -> BbServer {
        BbServer {
            name: Text::of(&s.name),
            url: Text::of(&s.url),
            username: Text::of(&s.username),
            password_obf: Text::of(&s.password_obf),
            password: Text::of(&s.password),
            headers: BbHeader::of(&s.headers),
            header_count: s.headers.count,
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

/// Reads `opds.json`'s text, at most `most` servers; NULL when it was read (each server handed to
/// `server` in order, with how many there are), else ArduinoJson's name for why not, and `server`
/// is not called.
///
/// # Safety
/// `text` is NULL or `len` readable bytes; `server` takes `context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_store_opds_servers_read(
    text: *const c_char,
    len: usize,
    most: usize,
    server: Option<ReadServer>,
    context: *mut c_void,
) -> *const c_char {
    // SAFETY: as documented.
    let servers = match read(unsafe { crate::view(text.cast::<u8>(), len) }, most) {
        Ok(servers) => servers,
        Err(error) => return error.c_name().as_ptr(),
    };
    if let Some(server) = server {
        for s in &servers {
            let entry = BbServer::of(s);
            // SAFETY: `entry`'s texts are readable for the call; the caller vouches for server and
            // context.
            unsafe { server(&raw const entry, servers.len(), context) };
        }
    }
    core::ptr::null()
}
