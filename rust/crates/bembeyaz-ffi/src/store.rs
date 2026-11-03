//! What the settings stores' exports share (`rust/include/bembeyaz/store.h`).

use alloc::vec::Vec;
use bembeyaz_store::Error;
use bembeyaz_store::headers::{HEADERS, Header, Headers};
use bembeyaz_store::password::{Decoded, Secret};
use core::ffi::{c_char, c_void};

/// store.h's `bb_store_text`.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct Text {
    data: *const c_char,
    len: usize,
}

// store.h's `bb_store_text` on the ESP32-S3 (32-bit) and the host (64-bit).
const _: () = {
    assert!(size_of::<Text>() == 2 * size_of::<usize>());
    assert!(core::mem::offset_of!(Text, len) == size_of::<usize>());
};

impl Text {
    /// A view of `bytes`, valid while they are; NULL when they are empty.
    pub(crate) fn of(bytes: &[u8]) -> Text {
        Text {
            data: if bytes.is_empty() {
                core::ptr::null()
            } else {
                bytes.as_ptr().cast()
            },
            len: bytes.len(),
        }
    }

    /// Its bytes' start: NULL for empty text.
    pub(crate) fn data(self) -> *const c_char {
        self.data
    }

    /// How many bytes it has.
    pub(crate) fn len(self) -> usize {
        self.len
    }

    /// # Safety
    /// `data` is NULL or `len` readable bytes that outlive `'a`.
    pub(crate) unsafe fn bytes<'a>(self) -> &'a [u8] {
        // SAFETY: as documented.
        unsafe { crate::view(self.data.cast(), self.len) }
    }
}

/// store.h's `bb_store_header`.
#[repr(C)]
pub(crate) struct BbHeader {
    name: Text,
    value_obf: Text,
}

const _: () = {
    assert!(size_of::<BbHeader>() == 2 * size_of::<Text>());
    assert!(core::mem::offset_of!(BbHeader, value_obf) == size_of::<Text>());
};

impl BbHeader {
    /// Views of `headers`' slots, valid while they are.
    pub(crate) fn of(headers: &Headers) -> [BbHeader; HEADERS] {
        headers.slots.each_ref().map(|h| BbHeader {
            name: Text::of(&h.name),
            value_obf: Text::of(&h.value_obf),
        })
    }

    /// The first `count` of `slots` (at most all of them).
    ///
    /// # Safety
    /// The texts are NULL or readable and outlive `'a`.
    pub(crate) unsafe fn view<'a>(slots: &[BbHeader; HEADERS], count: usize) -> Headers<&'a [u8]> {
        Headers {
            // SAFETY: as documented.
            slots: slots.each_ref().map(|h| unsafe {
                Header {
                    name: h.name.bytes(),
                    value_obf: h.value_obf.bytes(),
                }
            }),
            count: count.min(HEADERS),
        }
    }
}

/// store.h's `bb_store_write`.
pub(crate) type Write = unsafe extern "C" fn(*const c_char, usize, *mut c_void);

/// Hands `json` to `write` in one call; false when there is no text (memory ran out) or no
/// callback.
///
/// # Safety
/// `write` takes `context`.
pub(crate) unsafe fn hand_over(
    json: Option<&[u8]>,
    write: Option<Write>,
    context: *mut c_void,
) -> bool {
    let (Some(json), Some(write)) = (json, write) else {
        return false;
    };
    // SAFETY: `json` is readable for the call; the caller vouches for write and context.
    unsafe { write(json.as_ptr().cast(), json.len(), context) };
    true
}

/// store.h's `bb_store_decode`.
pub(crate) type Decode = unsafe extern "C" fn(
    *const c_char,
    usize,
    usize,
    Option<Write>,
    *mut c_void,
    *mut bool,
    *mut c_void,
) -> bool;

/// The plain text a decode callback hands over, and whether memory ran out holding it.
#[derive(Default)]
struct Plain {
    text: Vec<u8>,
    no_memory: bool,
}

/// A `bb_store_write` that appends to the [`Plain`] at `context`, without aborting when memory
/// runs out.
///
/// # Safety
/// `data` is NULL or `len` readable bytes; `context` is a `Plain` no one else uses for the call.
unsafe extern "C" fn append(data: *const c_char, len: usize, context: *mut c_void) {
    // SAFETY: as documented.
    let (bytes, plain) = unsafe {
        (
            crate::view(data.cast::<u8>(), len),
            &mut *context.cast::<Plain>(),
        )
    };
    if plain.text.try_reserve_exact(bytes.len()).is_ok() {
        plain.text.extend_from_slice(bytes);
    } else {
        plain.no_memory = true;
    }
}

/// Wraps `decode` as a `bembeyaz_store::password` decoder: ok with the plain text it hands over,
/// too long when it says so (it is given `most`) or past `most`, [`Error::NoMemory`] when holding
/// the text ran out of memory; with no `decode`, nothing decodes. The text is a [`Secret`] from
/// the start, so it is wiped however it
/// ends.
///
/// # Safety
/// `decode` takes `context` for as long as the decoder is used.
pub(crate) unsafe fn decoder(
    decode: Option<Decode>,
    context: *mut c_void,
) -> impl Fn(&[u8], usize) -> Result<Decoded, Error> {
    move |text, most| {
        let Some(decode) = decode else {
            return Ok(Decoded::default());
        };
        let mut plain = Plain::default();
        let mut too_long = false;
        // SAFETY: `text` is readable for the call, `plain` is only appended to, `too_long` is
        // writable; the caller vouches for decode and context.
        let ok = unsafe {
            decode(
                text.as_ptr().cast(),
                text.len(),
                most,
                Some(append),
                (&raw mut plain).cast(),
                &raw mut too_long,
                context,
            )
        };
        let secret = Secret::new(plain.text);
        if plain.no_memory {
            return Err(Error::NoMemory);
        }
        // Past `most` is too long, whatever the callback said.
        Ok(match (ok, too_long || secret.len() > most) {
            (true, false) => Decoded {
                text: secret,
                ok: true,
                too_long: false,
            },
            (_, true) => Decoded {
                too_long: true,
                ..Decoded::default()
            },
            (false, false) => Decoded::default(),
        })
    }
}
