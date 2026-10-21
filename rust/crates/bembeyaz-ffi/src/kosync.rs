//! Exports for `lib/KOReaderSync/ChapterXPathResolver` (see `rust/include/bembeyaz/kosync.h`).
//!
//! A resolver lives in a Rust heap block that `bb_kosync_new` hands out and `bb_kosync_free` takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use bembeyaz_kosync::{Resolver, Target};
use core::ffi::{c_char, c_int};
use core::ptr;
use core::slice;

use crate::xml::code;

/// `BB_KOSYNC_PARAGRAPH` and the others in kosync.h.
const PARAGRAPH: c_int = 0;
const OFFSET: c_int = 1;
const OFFSET_INCLUSIVE: c_int = 2;
const COUNT: c_int = 3;

/// NULL for an unknown kind or when out of memory.
#[unsafe(no_mangle)]
pub extern "C" fn bb_kosync_new(
    kind: c_int,
    target: u64,
    spine_index: u32,
    limit: usize,
) -> *mut Resolver {
    let target = match kind {
        PARAGRAPH => Target::Paragraph(u32::try_from(target).unwrap_or(u32::MAX)),
        OFFSET => Target::Offset {
            target,
            inclusive: false,
        },
        OFFSET_INCLUSIVE => Target::Offset {
            target,
            inclusive: true,
        },
        COUNT => Target::Count,
        _ => return ptr::null_mut(),
    };
    // SAFETY: Resolver is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Resolver>()) }.cast::<Resolver>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe { block.write(Resolver::new(target, spine_index, limit)) };
    }
    block
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_free(resolver: *mut Resolver) {
    if !resolver.is_null() {
        // SAFETY: from bb_kosync_new, not freed yet.
        unsafe {
            ptr::drop_in_place(resolver);
            dealloc(resolver.cast(), Layout::new::<Resolver>());
        }
    }
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL (`BB_XML_NO_MEMORY`); `data` points
/// to `len` bytes; `last` ends the chapter.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_parse(
    resolver: *mut Resolver,
    data: *const c_char,
    len: usize,
    last: bool,
) -> c_int {
    // SAFETY: as documented.
    let Some(resolver) = (unsafe { resolver.as_mut() }) else {
        return code(bembeyaz_xml::Error::NoMemory);
    };
    let data = if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at data.
        unsafe { slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    let result = if last {
        resolver.finish()
    } else {
        resolver.feed(data)
    };
    result.map_or_else(code, |()| 0)
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_failed(resolver: *const Resolver) -> bool {
    // SAFETY: as documented.
    unsafe { resolver.as_ref() }.is_none_or(Resolver::failed)
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL; `len` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_xpath(
    resolver: *const Resolver,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: as documented.
    let xpath = unsafe { resolver.as_ref() }.and_then(Resolver::xpath);
    // SAFETY: as documented.
    if let Some(len) = unsafe { len.as_mut() } {
        *len = xpath.map_or(0, <[u8]>::len);
    }
    xpath.map_or(ptr::null(), |x| x.as_ptr().cast())
}

/// # Safety
/// `resolver` is from `bb_kosync_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_kosync_visible_codepoints(resolver: *const Resolver) -> u64 {
    // SAFETY: as documented.
    unsafe { resolver.as_ref() }.map_or(0, Resolver::visible_codepoints)
}
