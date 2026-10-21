//! Exports for `lib/Utf8` (see `rust/include/bembeyaz/utf8.h`).

use bembeyaz_utf8::{self as utf8, Compose};
use core::ffi::{c_char, c_int, c_void};

pub const BB_UTF8_COMPOSE_UNCHANGED: c_int = 0;
pub const BB_UTF8_COMPOSE_DONE: c_int = 1;
pub const BB_UTF8_COMPOSE_ABORTED: c_int = 2;

type AbsorbedFn = unsafe extern "C" fn(index: u32, context: *mut c_void) -> bool;
type EmitFn = unsafe extern "C" fn(data: *const c_char, len: usize, context: *mut c_void);

unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    if len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(data.cast(), len) }
    }
}

/// Decodes the codepoint at `*string` and advances past it; 0 at the terminator.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_utf8_next_codepoint(string: *mut *const u8) -> u32 {
    unsafe {
        let p = *string;
        // Never read past the terminator.
        let mut head = [0u8; 4];
        let mut n = 0;
        while n < head.len() {
            let b = *p.add(n);
            if b == 0 {
                break;
            }
            head[n] = b;
            n += 1;
        }
        let (cp, used) = utf8::decode(&head[..n]);
        *string = p.add(used);
        cp
    }
}

/// Composes `input` to NFC, writing the result through `emit` in chunks.
/// Returns one of the `BB_UTF8_COMPOSE_*` codes; `emit` is not called when
/// the input is unchanged.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_utf8_compose_nfc(
    input: *const c_char,
    len: usize,
    on_absorbed: Option<AbsorbedFn>,
    absorbed_context: *mut c_void,
    emit: EmitFn,
    emit_context: *mut c_void,
) -> c_int {
    let mut chunk = [0u8; 64];
    let mut used = 0;
    let flush = |chunk: &[u8]| unsafe { emit(chunk.as_ptr().cast(), chunk.len(), emit_context) };

    let result = utf8::compose_nfc(
        unsafe { bytes(input, len) },
        |index| on_absorbed.is_none_or(|f| unsafe { f(index, absorbed_context) }),
        |piece| {
            if used + piece.len() > chunk.len() {
                flush(&chunk[..used]);
                used = 0;
            }
            chunk[used..used + piece.len()].copy_from_slice(piece);
            used += piece.len();
        },
    );
    match result {
        Compose::Unchanged => BB_UTF8_COMPOSE_UNCHANGED,
        Compose::Aborted => BB_UTF8_COMPOSE_ABORTED,
        Compose::Composed => {
            if used > 0 {
                flush(&chunk[..used]);
            }
            BB_UTF8_COMPOSE_DONE
        }
    }
}

/// Composes a NUL-terminated buffer in place; it never grows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_utf8_compose_nfc_in_place(buffer: *mut c_char) {
    unsafe {
        let len = core::ffi::CStr::from_ptr(buffer).count_bytes();
        let text = core::slice::from_raw_parts_mut(buffer.cast::<u8>(), len + 1);
        let new_len = utf8::compose_nfc_in_place(&mut text[..len]);
        text[new_len] = 0;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_utf8_safe_truncate_len(buffer: *const c_char, len: usize) -> usize {
    utf8::safe_truncate_len(unsafe { bytes(buffer, len) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_utf8_truncate_chars_len(
    string: *const c_char,
    len: usize,
    count: usize,
) -> usize {
    utf8::truncate_chars_len(unsafe { bytes(string, len) }, count)
}

/// See utf8.h.
#[unsafe(no_mangle)]
pub extern "C" fn bb_utf8_small_caps_upper(cp: u32) -> u32 {
    utf8::small_caps_upper(cp)
}
