//! Exports for `lib/Epub/Epub/parsers/TocParser` (see `rust/include/bembeyaz/toc.h`).
//!
//! A reader lives in a Rust heap block that `bb_toc_new` hands out and `bb_toc_free` takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use bembeyaz_toc::{Kind, Sink, Toc};
use core::ffi::{c_char, c_int, c_void};
use core::mem::{offset_of, size_of};
use core::ptr;
use core::slice;

use crate::xml::code;

/// `bb_toc_entry`.
#[repr(C)]
pub struct Entry {
    label: *const c_char,
    label_len: usize,
    href: *const c_char,
    href_len: usize,
    anchor: *const c_char,
    anchor_len: usize,
    level: u8,
}

// Checked against bb_toc_entry in TocParser.cpp too.
const _: () = assert!(
    size_of::<Entry>() == 7 * size_of::<usize>()
        && offset_of!(Entry, level) == 6 * size_of::<usize>()
);

type EntryFn = Option<unsafe extern "C" fn(*mut c_void, *const Entry)>;

struct CSink {
    entry: EntryFn,
    context: *mut c_void,
}

impl Sink for CSink {
    fn entry(&mut self, label: &[u8], href: &[u8], anchor: &[u8], level: u8) {
        if let Some(f) = self.entry {
            let entry = Entry {
                label: label.as_ptr().cast(),
                label_len: label.len(),
                href: href.as_ptr().cast(),
                href_len: href.len(),
                anchor: anchor.as_ptr().cast(),
                anchor_len: anchor.len(),
                level,
            };
            // SAFETY: the caller's callback; the entry and its bytes stay valid for the call.
            unsafe { f(self.context, &raw const entry) };
        }
    }
}

// BB_TOC_* in toc.h.
const NCX: c_int = 0;
const NAV: c_int = 1;

unsafe fn bytes<'a>(data: *const c_char, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at data, as the caller promises.
        unsafe { slice::from_raw_parts(data.cast(), len) }
    }
}

/// # Safety
/// `base` points to `base_len` bytes (or `base_len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_toc_new(
    kind: c_int,
    base: *const c_char,
    base_len: usize,
    limit: usize,
) -> *mut Toc {
    let kind = match kind {
        NCX => Kind::Ncx,
        NAV => Kind::Nav,
        _ => return ptr::null_mut(),
    };
    // SAFETY: as documented.
    let base = unsafe { bytes(base, base_len) };
    let mut base_copy = Vec::new();
    if base_copy.try_reserve_exact(base.len()).is_err() {
        return ptr::null_mut();
    }
    base_copy.extend_from_slice(base);
    // SAFETY: Toc is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Toc>()) }.cast::<Toc>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe { block.write(Toc::new(kind, base_copy, limit)) };
    }
    block
}

/// # Safety
/// `toc` is from `bb_toc_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_toc_free(toc: *mut Toc) {
    if !toc.is_null() {
        // SAFETY: from bb_toc_new, not freed yet.
        unsafe {
            ptr::drop_in_place(toc);
            dealloc(toc.cast(), Layout::new::<Toc>());
        }
    }
}

/// # Safety
/// `toc` is from `bb_toc_new` and not in use; `data` points to `len` bytes; `entry` does not call
/// back into this reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_toc_parse(
    toc: *mut Toc,
    data: *const c_char,
    len: usize,
    last: bool,
    entry: EntryFn,
    context: *mut c_void,
) -> c_int {
    // SAFETY: as documented.
    let Some(toc) = (unsafe { toc.as_mut() }) else {
        return code(bembeyaz_xml::Error::NoMemory);
    };
    // SAFETY: as documented.
    let data = unsafe { bytes(data, len) };
    match toc.feed(data, last, &mut CSink { entry, context }) {
        Ok(()) => 0,
        Err(e) => code(e),
    }
}

/// # Safety
/// `toc` is from `bb_toc_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_toc_error_offset(toc: *const Toc) -> u64 {
    // SAFETY: as documented.
    unsafe { toc.as_ref() }.map_or(0, Toc::error_offset)
}
