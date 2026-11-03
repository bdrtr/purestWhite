//! What the settings stores' exports share (`rust/include/bembeyaz/store.h`).

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
