//! Exports for the JPEG converters (`lib/Epub/Epub/converters/JpegToFramebufferConverter`,
//! `lib/JpegToBmpConverter`; see `rust/include/bembeyaz/jpeg.h`).
//!
//! An image lives in one Rust heap block, with the C read callback it reads through, that
//! `bb_jpeg_open` hands out and `bb_jpeg_free` takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use bembeyaz_jpeg::{Block, Error, Jpeg, Scale, Source};
use core::ffi::{c_int, c_void};
use core::mem::{MaybeUninit, offset_of, size_of};
use core::ptr;

type ReadFn =
    unsafe extern "C" fn(context: *mut c_void, pos: u32, buf: *mut u8, len: usize) -> usize;
type DrawFn = unsafe extern "C" fn(user: *mut c_void, block: *const BbJpegBlock) -> c_int;

struct CSource {
    read: ReadFn,
    context: *mut c_void,
}

impl Source for CSource {
    fn read_at(&mut self, pos: u32, buf: &mut [u8]) -> usize {
        // SAFETY: the caller's callback, given a buffer it may fill.
        unsafe { (self.read)(self.context, pos, buf.as_mut_ptr(), buf.len()) }.min(buf.len())
    }
}

pub struct BbJpeg {
    jpeg: MaybeUninit<Jpeg>,
    source: CSource,
}

/// `bb_jpeg_info`.
#[repr(C)]
pub struct BbJpegInfo {
    width: i32,
    height: i32,
    progressive: bool,
}

/// `bb_jpeg_block`.
#[repr(C)]
pub struct BbJpegBlock {
    pixels: *const u8,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    width_used: i32,
}

// What jpeg.h spells out (JpegToFramebufferConverter checks the C side of the layouts).
const P: usize = size_of::<*const u8>();
const _: () =
    assert!(offset_of!(BbJpegInfo, height) == 4 && offset_of!(BbJpegInfo, progressive) == 8);
const _: () = assert!(
    offset_of!(BbJpegBlock, x) == P
        && offset_of!(BbJpegBlock, y) == P + 4
        && offset_of!(BbJpegBlock, width) == P + 8
        && offset_of!(BbJpegBlock, height) == P + 12
        && offset_of!(BbJpegBlock, width_used) == P + 16
);
// `bb_jpeg_free` frees the block without running a destructor.
const _: () = assert!(!core::mem::needs_drop::<BbJpeg>());

/// BB_JPEG_* in jpeg.h: JPEGDEC's error numbers.
const SUCCESS: c_int = 0;
const INVALID_PARAMETER: c_int = 1;
const ERROR_MEMORY: c_int = 5;

fn code(error: Error) -> c_int {
    match error {
        Error::NotOpen => INVALID_PARAMETER,
        Error::Decode => 2,
        Error::Unsupported => 3,
        Error::InvalidFile => 4,
    }
}

fn status(result: Result<(), Error>) -> c_int {
    result.map_or_else(code, |()| SUCCESS)
}

/// Opens the file (`size` bytes) that `read` reads, as JPEGDEC's `open`, and writes the result to
/// `*status`. Returns the image even when its header fails, or null when `read` is null or memory
/// runs out.
///
/// # Safety
/// `read` must be callable with `context` until `bb_jpeg_free`; `status` is writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_jpeg_open(
    read: Option<ReadFn>,
    context: *mut c_void,
    size: i32,
    status_out: *mut c_int,
) -> *mut BbJpeg {
    let report = |value: c_int| {
        if !status_out.is_null() {
            // SAFETY: per the contract.
            unsafe { status_out.write(value) };
        }
    };
    let Some(read) = read else {
        report(INVALID_PARAMETER);
        return ptr::null_mut();
    };
    // SAFETY: the layout is not zero-sized.
    let image = unsafe { alloc(Layout::new::<BbJpeg>()) }.cast::<BbJpeg>();
    if image.is_null() {
        report(ERROR_MEMORY);
        return ptr::null_mut();
    }
    // SAFETY: a fresh block of BbJpeg's layout; each field is written before any is read.
    unsafe {
        let source = &raw mut (*image).source;
        source.write(CSource { read, context });
        let jpeg = Jpeg::init(&mut (*image).jpeg);
        report(status(jpeg.open(&mut *source, size)));
    }
    image
}

/// # Safety
/// `jpeg` comes from `bb_jpeg_open` and `info` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_jpeg_get_info(jpeg: *const BbJpeg, info: *mut BbJpegInfo) {
    // SAFETY: per the contract.
    let (Some(image), Some(info)) = (unsafe { jpeg.as_ref() }, unsafe { info.as_mut() }) else {
        return;
    };
    // SAFETY: `bb_jpeg_open` initialised it.
    let jpeg = unsafe { image.jpeg.assume_init_ref() };
    *info = BbJpegInfo {
        width: jpeg.width(),
        height: jpeg.height(),
        progressive: jpeg.is_progressive(),
    };
}

/// Decodes the image at `options` (0 or a BB_JPEG_SCALE_*), handing each block to `draw` (0
/// stops it, and the decode still succeeds). Returns `BB_JPEG_SUCCESS` or an error code.
///
/// # Safety
/// `jpeg` comes from `bb_jpeg_open`; `draw` must be callable with `user`. A block lives until
/// `draw` returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_jpeg_decode(
    jpeg: *mut BbJpeg,
    options: c_int,
    draw: Option<DrawFn>,
    user: *mut c_void,
) -> c_int {
    // SAFETY: per the contract.
    let (Some(image), Some(draw)) = (unsafe { jpeg.as_mut() }, draw) else {
        return INVALID_PARAMETER;
    };
    let size = match options {
        0 => Scale::Full,
        2 => Scale::Half,
        4 => Scale::Quarter,
        8 => Scale::Eighth,
        _ => return INVALID_PARAMETER,
    };
    // SAFETY: `bb_jpeg_open` initialised it.
    let jpeg = unsafe { image.jpeg.assume_init_mut() };
    status(jpeg.decode(&mut image.source, size, |block: &Block<'_>| {
        let block = BbJpegBlock {
            pixels: block.pixels.as_ptr(),
            x: block.x,
            y: block.y,
            width: block.width,
            height: block.height,
            width_used: block.width_used,
        };
        // SAFETY: per the contract; the block outlives the call.
        unsafe { draw(user, &raw const block) != 0 }
    }))
}

/// # Safety
/// `jpeg` comes from `bb_jpeg_open` and is not used again; null is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_jpeg_free(jpeg: *mut BbJpeg) {
    if !jpeg.is_null() {
        // SAFETY: per the contract; nothing in it needs dropping.
        unsafe { dealloc(jpeg.cast(), Layout::new::<BbJpeg>()) };
    }
}
