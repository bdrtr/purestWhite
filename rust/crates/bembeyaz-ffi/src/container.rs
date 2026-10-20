//! Exports for `lib/Epub/Epub/parsers/ContainerParser` (see `rust/include/bembeyaz/container.h`).
//!
//! A reader lives in a Rust heap block that `bb_container_new` hands out and `bb_container_free`
//! takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use bembeyaz_opf::container::Container;
use core::ffi::{c_char, c_int};
use core::ptr;
use core::slice;

use crate::xml::code;

#[unsafe(no_mangle)]
pub extern "C" fn bb_container_new(limit: usize) -> *mut Container {
    // SAFETY: Container is not zero-sized.
    let block = unsafe { alloc(Layout::new::<Container>()) }.cast::<Container>();
    if !block.is_null() {
        // SAFETY: a fresh block of the right layout.
        unsafe { block.write(Container::new(limit)) };
    }
    block
}

/// # Safety
/// `container` is from `bb_container_new` and not freed yet, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_container_free(container: *mut Container) {
    if !container.is_null() {
        // SAFETY: from bb_container_new, not freed yet.
        unsafe {
            ptr::drop_in_place(container);
            dealloc(container.cast(), Layout::new::<Container>());
        }
    }
}

/// # Safety
/// `container` is from `bb_container_new` and not in use, or NULL (`BB_XML_NO_MEMORY`); `data`
/// points to `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_container_parse(
    container: *mut Container,
    data: *const c_char,
    len: usize,
    last: bool,
) -> c_int {
    // SAFETY: as documented.
    let Some(container) = (unsafe { container.as_mut() }) else {
        return code(bembeyaz_xml::Error::NoMemory);
    };
    let data = if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: len bytes at data.
        unsafe { slice::from_raw_parts(data.cast::<u8>(), len) }
    };
    match container.feed(data, last) {
        Ok(()) => 0,
        Err(e) => code(e),
    }
}

/// # Safety
/// `container` is from `bb_container_new`, or NULL; `len` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_container_full_path(
    container: *const Container,
    len: *mut usize,
) -> *const c_char {
    // SAFETY: as documented.
    let path: &[u8] = unsafe { container.as_ref() }.map_or(&[], Container::full_path);
    // SAFETY: as documented.
    if let Some(len) = unsafe { len.as_mut() } {
        *len = path.len();
    }
    path.as_ptr().cast()
}

/// # Safety
/// `container` is from `bb_container_new` and not in use, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_container_take_path_changed(container: *mut Container) -> bool {
    // SAFETY: as documented.
    unsafe { container.as_mut() }.is_some_and(Container::take_changed)
}

/// # Safety
/// `container` is from `bb_container_new`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_container_error_offset(container: *const Container) -> u64 {
    // SAFETY: as documented.
    unsafe { container.as_ref() }.map_or(0, Container::error_offset)
}
