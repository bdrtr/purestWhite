//! C ABI over the Rust modules. Each C++ module that moves to Rust keeps its
//! header and becomes a thin shim over the `bb_*` functions exported here;
//! the matching declarations live in `rust/include/bembeyaz/`.
#![cfg_attr(target_os = "none", no_std)]

extern crate alloc;

mod bidi;
mod bmp;
mod book;
mod boxes;
mod carry;
mod clippings;
mod compressed;
mod container;
mod cpfont;
mod css;
#[cfg(target_os = "none")]
mod device;
mod dict;
mod dither;
mod drop_cap;
mod expat;
mod gfx;
mod html;
mod hyphenation;
mod image;
mod io;
mod jpeg;
mod json;
mod kosync;
mod layout;
mod library;
mod opf;
mod page;
mod path;
mod png;
mod png_cover;
mod section;
mod table;
mod toc;
mod utf8;
mod xml;
mod xtc;
mod zip;

// SAFETY: `data` is null or holds `len` values; an empty slice needs no memory.
pub(crate) unsafe fn view<'a, T>(data: *const T, len: usize) -> &'a [T] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(data, len) }
    }
}

// SAFETY: `data` is null or holds `len` writable values.
pub(crate) unsafe fn view_mut<'a, T>(data: *mut T, len: usize) -> &'a mut [T] {
    if data.is_null() || len == 0 {
        &mut []
    } else {
        unsafe { core::slice::from_raw_parts_mut(data, len) }
    }
}

// SAFETY: `ptr` is null or writable.
pub(crate) unsafe fn set<T>(ptr: *mut T, value: T) {
    if !ptr.is_null() {
        unsafe { ptr.write(value) };
    }
}

// The caller's room for `len` values, as a slice that lives as long as its owner is borrowed;
// empty for 0.
//
// SAFETY: `ptr` is null or holds `len` writable values until the next callback.
pub(crate) unsafe fn room<'a, T>(ptr: *mut T, len: usize) -> Option<&'a mut [T]> {
    if len == 0 {
        return Some(&mut []);
    }
    // SAFETY: as above.
    (!ptr.is_null()).then(|| unsafe { core::slice::from_raw_parts_mut(ptr, len) })
}
