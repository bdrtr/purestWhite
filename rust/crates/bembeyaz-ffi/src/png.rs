//! Exports for `lib/Epub/Epub/converters/PngToFramebufferConverter` (see
//! `rust/include/bembeyaz/png.h`).
//!
//! An image lives in one Rust heap block, with the C read callback it reads through, that
//! `bb_png_open` hands out and `bb_png_free` takes back.

use alloc::alloc::{Layout, alloc, dealloc};
use bembeyaz_png::{Error, MAX_PITCH, Png, Row, Source, pixel};
use core::ffi::{c_int, c_void};
use core::mem::{MaybeUninit, offset_of, size_of};
use core::ptr;

type ReadFn =
    unsafe extern "C" fn(context: *mut c_void, pos: u32, buf: *mut u8, len: usize) -> usize;
type DrawFn = unsafe extern "C" fn(user: *mut c_void, row: *const BbPngRow) -> c_int;

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

pub struct BbPng {
    png: MaybeUninit<Png>,
    source: CSource,
}

/// `bb_png_info`.
#[repr(C)]
pub struct BbPngInfo {
    width: i32,
    height: i32,
    pixel_type: u8,
    bpp: u8,
    has_alpha: bool,
}

/// `bb_png_row`.
#[repr(C)]
pub struct BbPngRow {
    pixels: *const u8,
    palette: *const u8,
    y: i32,
    width: i32,
    pitch: i32,
    transparent: u32,
    pixel_type: u8,
    bpp: u8,
    has_alpha: bool,
}

// What png.h spells out (PngToFramebufferConverter.cpp checks the C side of the layouts).
const P: usize = size_of::<*const u8>();
const _: () = assert!(MAX_PITCH == 16416);
const _: () = assert!(
    pixel::GRAYSCALE == 0
        && pixel::TRUECOLOR == 2
        && pixel::INDEXED == 3
        && pixel::GRAY_ALPHA == 4
        && pixel::TRUECOLOR_ALPHA == 6
);
const _: () = assert!(
    offset_of!(BbPngInfo, height) == 4
        && offset_of!(BbPngInfo, pixel_type) == 8
        && offset_of!(BbPngInfo, bpp) == 9
        && offset_of!(BbPngInfo, has_alpha) == 10
);
const _: () = assert!(
    offset_of!(BbPngRow, palette) == P
        && offset_of!(BbPngRow, y) == 2 * P
        && offset_of!(BbPngRow, width) == 2 * P + 4
        && offset_of!(BbPngRow, pitch) == 2 * P + 8
        && offset_of!(BbPngRow, transparent) == 2 * P + 12
        && offset_of!(BbPngRow, pixel_type) == 2 * P + 16
        && offset_of!(BbPngRow, bpp) == 2 * P + 17
        && offset_of!(BbPngRow, has_alpha) == 2 * P + 18
);

/// BB_PNG_* in png.h: PNGdec's return values.
const SUCCESS: c_int = 0;

fn code(error: Error) -> c_int {
    match error {
        Error::NotOpen | Error::FileEnded => 1,
        Error::Corrupt => 2,
        Error::Memory => 3,
        Error::Unsupported => 5,
        Error::InvalidFile => 6,
        Error::TooBig => 7,
        Error::QuitEarly => 8,
    }
}

fn status(result: Result<(), Error>) -> c_int {
    result.map_or_else(code, |()| SUCCESS)
}

/// Reads the header of the file (`size` bytes) through `read`, and writes the result to
/// `*status`. Returns the image even when its header fails (its info is still set), or null when
/// out of memory.
///
/// # Safety
/// `read` must be callable with `context` until `bb_png_free`; `status` is writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_open(
    read: Option<ReadFn>,
    context: *mut c_void,
    size: u32,
    status_out: *mut c_int,
) -> *mut BbPng {
    let report = |value: c_int| {
        if !status_out.is_null() {
            // SAFETY: per the contract.
            unsafe { status_out.write(value) };
        }
    };
    let Some(read) = read else {
        report(code(Error::InvalidFile));
        return ptr::null_mut();
    };
    // SAFETY: the layout is not zero-sized.
    let image = unsafe { alloc(Layout::new::<BbPng>()) }.cast::<BbPng>();
    if image.is_null() {
        report(code(Error::Memory));
        return ptr::null_mut();
    }
    // SAFETY: a fresh block of BbPng's layout; each field is written before any is read.
    unsafe {
        let source = &raw mut (*image).source;
        source.write(CSource { read, context });
        let png = Png::init(&mut (*image).png);
        report(status(png.open(&mut *source, size)));
    }
    image
}

fn png_of(image: &mut BbPng) -> (&mut Png, &mut CSource) {
    // SAFETY: `bb_png_open` initialised it.
    (unsafe { image.png.assume_init_mut() }, &mut image.source)
}

/// # Safety
/// `png` comes from `bb_png_open` and `info` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_get_info(png: *const BbPng, info: *mut BbPngInfo) {
    // SAFETY: per the contract.
    let (Some(image), Some(info)) = (unsafe { png.as_ref() }, unsafe { info.as_mut() }) else {
        return;
    };
    // SAFETY: `bb_png_open` initialised it.
    let png = unsafe { image.png.assume_init_ref() };
    *info = BbPngInfo {
        width: png.width(),
        height: png.height(),
        pixel_type: png.pixel_type(),
        bpp: png.bpp(),
        has_alpha: png.has_alpha(),
    };
}

/// Decodes the image, handing each row to `draw` (0 stops it). Returns `BB_PNG_SUCCESS` or an
/// error code.
///
/// # Safety
/// `png` comes from `bb_png_open`; `draw` must be callable with `user`. A row lives until `draw`
/// returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_decode(
    png: *mut BbPng,
    draw: Option<DrawFn>,
    user: *mut c_void,
) -> c_int {
    // SAFETY: per the contract.
    let (Some(image), Some(draw)) = (unsafe { png.as_mut() }, draw) else {
        return code(Error::NotOpen);
    };
    let (png, source) = png_of(image);
    status(png.decode(source, |row: &Row<'_>| {
        let row = BbPngRow {
            pixels: row.pixels.as_ptr(),
            palette: row.palette.as_ptr(),
            y: row.y,
            width: row.width,
            pitch: i32::try_from(row.pixels.len()).unwrap_or(i32::MAX),
            transparent: row.transparent,
            pixel_type: row.pixel_type,
            bpp: row.bpp,
            has_alpha: row.has_alpha,
        };
        // SAFETY: per the contract; the row outlives the call.
        unsafe { draw(user, &raw const row) != 0 }
    }))
}

/// # Safety
/// `png` comes from `bb_png_open` and is not used again; null is ignored.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bb_png_free(png: *mut BbPng) {
    if !png.is_null() {
        // SAFETY: per the contract; nothing in it needs dropping.
        unsafe { dealloc(png.cast(), Layout::new::<BbPng>()) };
    }
}

// `bb_png_free` frees the block without running a destructor.
const _: () = assert!(!core::mem::needs_drop::<BbPng>());
// On the device the block takes malloc's path into internal RAM, which RadioPolicy.h allows only
// up to 4096 bytes a request (device.rs routes blocks aligned above 4 bytes differently).
#[cfg(target_pointer_width = "32")]
const _: () = assert!(core::mem::align_of::<BbPng>() <= 4 && core::mem::size_of::<BbPng>() <= 4096);
