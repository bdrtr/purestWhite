//! Exports for `lib/Epub/Epub/parsers/ContentOpfParser` (see `rust/include/bembeyaz/opf.h`).
//!
//! A reader lives in a Rust heap block that `bb_opf_new` hands out and `bb_opf_free` takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use bembeyaz_opf::{Package, Sink};
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use core::slice;

use crate::xml::code;

/// `bb_opf_callbacks`.
#[repr(C)]
pub struct Callbacks {
    spine_item: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize)>,
    stylesheet: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize)>,
    parse_float: Option<unsafe extern "C" fn(*mut c_void, *const c_char, usize, *mut f32) -> bool>,
}

struct CSink<'a> {
    callbacks: &'a Callbacks,
    context: *mut c_void,
}

impl Sink for CSink<'_> {
    fn spine_item(&mut self, href: &[u8]) {
        if let Some(f) = self.callbacks.spine_item {
            // SAFETY: the caller's callback; href stays valid for the call.
            unsafe { f(self.context, href.as_ptr().cast(), href.len()) };
        }
    }

    fn stylesheet(&mut self, href: &[u8]) {
        if let Some(f) = self.callbacks.stylesheet {
            // SAFETY: as above.
            unsafe { f(self.context, href.as_ptr().cast(), href.len()) };
        }
    }

    fn parse_float(&mut self, text: &[u8]) -> Option<f32> {
        let f = self.callbacks.parse_float?;
        let mut value = 0.0;
        // SAFETY: as above; value is a valid float to write.
        unsafe {
            f(
                self.context,
                text.as_ptr().cast(),
                text.len(),
                &raw mut value,
            )
        }
        .then_some(value)
    }
}

// BB_OPF_* in opf.h.
const SPINE: u32 = 1;
const METADATA_ONLY: u32 = 2;
const TITLE: c_int = 0;
const AUTHOR: c_int = 1;
const LANGUAGE: c_int = 2;
const ISBN: c_int = 3;
const ASIN: c_int = 4;
const SERIES: c_int = 5;
const TOC_NCX: c_int = 6;
const TOC_NAV: c_int = 7;
const COVER: c_int = 8;
const GUIDE_COVER: c_int = 9;
const TEXT_REFERENCE: c_int = 10;

/// # Safety
/// `base` points to `base_len` bytes (or `base_len` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_opf_new(
    base: *const c_char,
    base_len: usize,
    flags: u32,
    limit: usize,
    manifest_limit: usize,
) -> *mut Package {
    let base = if base.is_null() || base_len == 0 {
        &[]
    } else {
        // SAFETY: base_len bytes at base.
        unsafe { slice::from_raw_parts(base.cast::<u8>(), base_len) }
    };
    let mut base_copy = Vec::new();
    if base_copy.try_reserve_exact(base.len()).is_err() {
        return ptr::null_mut();
    }
    base_copy.extend_from_slice(base);
    // SAFETY: Package is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Package>()) }.cast::<Package>();
    if !block.is_null() {
        let package = Package::new(
            base_copy,
            flags & SPINE != 0,
            flags & METADATA_ONLY != 0,
            limit,
            manifest_limit,
        );
        // SAFETY: a fresh block of the right layout.
        unsafe { block.write(package) };
    }
    block
}

/// # Safety
/// `opf` is from `bb_opf_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_opf_free(opf: *mut Package) {
    if !opf.is_null() {
        // SAFETY: from bb_opf_new, not freed yet.
        unsafe {
            ptr::drop_in_place(opf);
            dealloc(opf.cast(), Layout::new::<Package>());
        }
    }
}

/// # Safety
/// `opf` is from `bb_opf_new` and not in use; `data` points to `len` bytes; `callbacks` is a
/// `bb_opf_callbacks` that does not call back into this reader.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_opf_parse(
    opf: *mut Package,
    data: *const c_char,
    len: usize,
    last: bool,
    callbacks: *const Callbacks,
    context: *mut c_void,
) -> c_int {
    // SAFETY: as documented.
    let (Some(opf), Some(callbacks)) = (unsafe { opf.as_mut() }, unsafe { callbacks.as_ref() })
    else {
        return code(bembeyaz_xml::Error::NoMemory);
    };
    let data = if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at data.
        unsafe { slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    match opf.feed(data, last, &mut CSink { callbacks, context }) {
        Ok(()) => 0,
        Err(e) => code(e),
    }
}

/// # Safety
/// `opf` is from `bb_opf_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_opf_error_offset(opf: *const Package) -> u64 {
    // SAFETY: as documented.
    unsafe { opf.as_ref() }.map_or(0, Package::error_offset)
}

/// # Safety
/// `opf` is from `bb_opf_new`, or NULL; `len` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_opf_field(
    opf: *const Package,
    field: c_int,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: as documented.
    let value: &[u8] = unsafe { opf.as_ref() }.map_or(&[], |opf| {
        let fields = opf.fields();
        match field {
            TITLE => &fields.title,
            AUTHOR => &fields.author,
            LANGUAGE => &fields.language,
            ISBN => &fields.isbn,
            ASIN => &fields.asin,
            SERIES => &fields.series,
            TOC_NCX => &fields.toc_ncx,
            TOC_NAV => &fields.toc_nav,
            COVER => &fields.cover,
            GUIDE_COVER => &fields.guide_cover,
            TEXT_REFERENCE => &fields.text_reference,
            _ => &[],
        }
    });
    // SAFETY: as documented.
    if let Some(len) = unsafe { len.as_mut() } {
        *len = value.len();
    }
    value.as_ptr().cast()
}

/// # Safety
/// `opf` is from `bb_opf_new`, or NULL; `out` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_opf_series_index(opf: *const Package, out: *mut f32) -> bool {
    // SAFETY: as documented.
    let Some(index) = unsafe { opf.as_ref() }.and_then(|opf| opf.fields().series_index) else {
        return false;
    };
    // SAFETY: as documented.
    if let Some(out) = unsafe { out.as_mut() } {
        *out = index;
    }
    true
}
