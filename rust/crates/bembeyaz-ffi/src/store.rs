//! What the settings stores' exports share (`rust/include/bembeyaz/store.h`).

use bembeyaz_store::headers::{HEADERS, Header, Headers};
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
