//! Exports for `lib/PngToBmpConverter` (see `rust/include/bembeyaz/png_cover.h`).
//!
//! A cover lives in one Rust heap block, with the C read callback it reads through, that
//! `bb_png_cover_open` hands out and `bb_png_cover_free` takes back.

use crate::png::{CSource, ReadFn};
use alloc::alloc::{Layout, alloc, dealloc};
use bembeyaz_png::cover::{Cover, Error};
use core::ffi::{c_int, c_void};
use core::mem::{MaybeUninit, offset_of};
use core::ptr;

type DrawFn = unsafe extern "C" fn(user: *mut c_void, gray: *const u8) -> c_int;

pub struct BbPngCover {
    cover: MaybeUninit<Cover>,
    source: CSource,
}

/// `bb_png_cover_info`.
#[repr(C)]
pub struct BbPngCoverInfo {
    width: u32,
    height: u32,
}

// What png_cover.h spells out (PngToBmpConverter.cpp checks the C side).
const _: () = assert!(offset_of!(BbPngCoverInfo, height) == 4);

/// BB_PNG_COVER_* in png_cover.h.
const OK: c_int = 0;

fn code(error: Error) -> c_int {
    match error {
        Error::NotOpen => 1,
        Error::HeaderEnded => 2,
        Error::NotPng => 3,
        Error::NoHeader => 4,
        Error::Unsupported => 5,
        Error::BadSize => 6,
        Error::NoImageData => 7,
        Error::Memory => 8,
        Error::Corrupt => 9,
        Error::UnknownFilter => 10,
        Error::QuitEarly => 11,
    }
}

fn status(result: Result<(), Error>) -> c_int {
    result.map_or_else(code, |()| OK)
}

/// Reads the header of the file (`size` bytes) through `read` and walks to its image data, and
/// writes the result to `*status_out`. Returns the cover even when that fails, or null when out of
/// memory or `read` is null.
///
/// # Safety
/// `read` must be callable with `context` until `bb_png_cover_free`; `status_out` is writable or
/// null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_cover_open(
    read: Option<ReadFn>,
    context: *mut c_void,
    size: u32,
    status_out: *mut c_int,
) -> *mut BbPngCover {
    let report = |value: c_int| {
        if !status_out.is_null() {
            // SAFETY: per the contract.
            unsafe { status_out.write(value) };
        }
    };
    let Some(read) = read else {
        report(code(Error::NotPng));
        return ptr::null_mut();
    };
    // SAFETY: the layout is not zero-sized.
    let block = unsafe { alloc(Layout::new::<BbPngCover>()) }.cast::<BbPngCover>();
    if block.is_null() {
        report(code(Error::Memory));
        return ptr::null_mut();
    }
    // SAFETY: a fresh block of BbPngCover's layout; each field is written before any is read.
    unsafe {
        let source = &raw mut (*block).source;
        source.write(CSource { read, context });
        let cover = Cover::init(&mut (*block).cover);
        report(status(cover.open(&mut *source, size)));
    }
    block
}

/// # Safety
/// `cover` comes from `bb_png_cover_open` and `info` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_cover_get_info(
    cover: *const BbPngCover,
    info: *mut BbPngCoverInfo,
) {
    // SAFETY: per the contract.
    let (Some(block), Some(info)) = (unsafe { cover.as_ref() }, unsafe { info.as_mut() }) else {
        return;
    };
    // SAFETY: `bb_png_cover_open` initialised it.
    let cover = unsafe { block.cover.assume_init_ref() };
    *info = BbPngCoverInfo {
        width: cover.width(),
        height: cover.height(),
    };
}

/// Decodes the cover, handing each row to `draw` as `width` gray bytes (0 stops it). Returns
/// `BB_PNG_COVER_OK` or an error code.
///
/// # Safety
/// `cover` comes from `bb_png_cover_open`; `draw` must be callable with `user`. A row lives until
/// `draw` returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_cover_decode(
    cover: *mut BbPngCover,
    draw: Option<DrawFn>,
    user: *mut c_void,
) -> c_int {
    // SAFETY: per the contract.
    let (Some(block), Some(draw)) = (unsafe { cover.as_mut() }, draw) else {
        return code(Error::NotOpen);
    };
    // SAFETY: `bb_png_cover_open` initialised it.
    let cover = unsafe { block.cover.assume_init_mut() };
    // SAFETY: per the contract; the row outlives the call.
    status(cover.decode(&mut block.source, |gray| unsafe {
        draw(user, gray.as_ptr()) != 0
    }))
}

/// # Safety
/// `cover` comes from `bb_png_cover_open` and is not used again; null is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_cover_free(cover: *mut BbPngCover) {
    if !cover.is_null() {
        // SAFETY: per the contract; nothing in it needs dropping.
        unsafe { dealloc(cover.cast(), Layout::new::<BbPngCover>()) };
    }
}

// `bb_png_cover_free` frees the block without running a destructor.
const _: () = assert!(!core::mem::needs_drop::<BbPngCover>());
// The block (about 2.3 KB, the read buffer in it) stays in internal RAM, where malloc keeps
// requests of up to 4096 bytes (RadioPolicy.h).
#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::size_of::<BbPngCover>() <= 4096);
