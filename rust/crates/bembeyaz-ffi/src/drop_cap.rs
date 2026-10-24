//! Exports for the chapter parser's drop caps (see `rust/include/bembeyaz/drop_cap.h`).

use bembeyaz_layout::drop_cap::{self, Body, DropCap, Fonts};
use core::ffi::{c_char, c_int, c_void};
use core::mem::offset_of;

// drop_cap.h's bb_drop_cap; the parser checks the same layout.
const _: () = assert!(
    size_of::<DropCap>() == 16
        && offset_of!(DropCap, width) == 4
        && offset_of!(DropCap, baseline) == 8
        && offset_of!(DropCap, height) == 12
);

type Metrics = unsafe extern "C" fn(*mut c_void, bool, u32, *mut i32, *mut c_int) -> bool;

struct Callback {
    metrics: Metrics,
    context: *mut c_void,
}

impl Fonts for Callback {
    fn metrics(&mut self, cap: bool, cp: u32) -> Option<(i32, i32)> {
        let (mut advance, mut top) = (0, 0);
        // SAFETY: the callback is the caller's and takes the arguments the header documents.
        unsafe { (self.metrics)(self.context, cap, cp, &raw mut advance, &raw mut top) }
            .then_some((advance, top))
    }
}

/// # Safety
/// `text` is NULL or holds `len` bytes; `metrics` follows drop_cap.h; `out` is NULL or writable.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn bb_drop_cap_place(
    lines: u8,
    text: *const c_char,
    len: usize,
    line_height: c_int,
    ascender: c_int,
    space_width: c_int,
    width: u16,
    metrics: Option<Metrics>,
    context: *mut c_void,
    out: *mut DropCap,
) {
    let Some(metrics) = metrics else {
        return;
    };
    let body = Body {
        line_height,
        ascender,
        space_width,
        width,
    };
    // SAFETY: as documented.
    let text = unsafe { crate::view(text.cast(), len) };
    let cap = drop_cap::place(lines, text, &body, &mut Callback { metrics, context });
    // SAFETY: as documented.
    unsafe { crate::set(out, cap) };
}
