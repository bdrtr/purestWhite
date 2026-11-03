//! What an operating system would give wolfSSL, from Rust: its memory (`XMALLOC`, `XFREE`,
//! `XREALLOC`, through the global allocator), the time (`XTIME`, from `set_time`) and random
//! bytes (`pw_tls_random`, from `set_random`).

use alloc::alloc::{Layout, alloc, dealloc, realloc};
use core::ffi::{c_int, c_uchar, c_uint, c_void};
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

/// Each block starts with its size, in a header that keeps the rest 8-byte aligned (wolfSSL
/// stores 64-bit words).
const HEADER: usize = 8;
const ALIGN: usize = 8;
const _: () = assert!(core::mem::size_of::<usize>() <= HEADER);

fn layout(size: usize) -> Option<Layout> {
    Layout::from_size_align(size.checked_add(HEADER)?, ALIGN).ok()
}

/// # Safety
/// Called by wolfSSL: returns a block of `size` bytes or null.
#[allow(
    clippy::cast_ptr_alignment,
    reason = "every block is allocated `ALIGN`-aligned, the header first"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XMALLOC(size: usize, _heap: *mut c_void, _kind: c_int) -> *mut c_void {
    let Some(layout) = layout(size) else {
        return core::ptr::null_mut();
    };
    // SAFETY: the layout is not zero-sized (it holds the header).
    let block = unsafe { alloc(layout) };
    if block.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the block holds the header, aligned for a usize.
    unsafe {
        block.cast::<usize>().write(size);
        block.add(HEADER).cast()
    }
}

/// # Safety
/// `ptr` is null or a block from `XMALLOC` or `XREALLOC`, not freed yet.
#[allow(
    clippy::cast_ptr_alignment,
    reason = "every block is allocated `ALIGN`-aligned, the header first"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XFREE(ptr: *mut c_void, _heap: *mut c_void, _kind: c_int) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the header sits just before the block, as `XMALLOC` wrote it.
    unsafe {
        let block = ptr.cast::<u8>().sub(HEADER);
        let size = block.cast::<usize>().read();
        if let Some(layout) = layout(size) {
            dealloc(block, layout);
        }
    }
}

/// # Safety
/// `ptr` is null or a block from `XMALLOC` or `XREALLOC`, not freed yet.
#[allow(
    clippy::cast_ptr_alignment,
    reason = "every block is allocated `ALIGN`-aligned, the header first"
)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XREALLOC(
    ptr: *mut c_void,
    size: usize,
    heap: *mut c_void,
    kind: c_int,
) -> *mut c_void {
    if ptr.is_null() {
        // SAFETY: as `XMALLOC`.
        return unsafe { XMALLOC(size, heap, kind) };
    }
    let Some(new_layout) = layout(size) else {
        return core::ptr::null_mut();
    };
    // SAFETY: the header sits just before the block; the old layout is the one it was made with.
    unsafe {
        let block = ptr.cast::<u8>().sub(HEADER);
        let Some(old) = layout(block.cast::<usize>().read()) else {
            return core::ptr::null_mut();
        };
        let moved = realloc(block, old, new_layout.size());
        if moved.is_null() {
            return core::ptr::null_mut();
        }
        moved.cast::<usize>().write(size);
        moved.add(HEADER).cast()
    }
}

/// The time, in seconds since 1970; 0 until `set_time`.
static NOW: AtomicU32 = AtomicU32::new(0);

/// Sets the time certificates are checked against, in seconds since 1970 (UTC).
pub fn set_time(unix_seconds: u32) {
    NOW.store(unix_seconds, Ordering::Relaxed);
}

/// The time certificates are checked against (0 until it is set).
pub(crate) fn now() -> u32 {
    NOW.load(Ordering::Relaxed)
}

/// # Safety
/// `now` is null or points to a `time_t` (64 bits, checked by c/shim.c).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn XTIME(now: *mut i64) -> i64 {
    let seconds = i64::from(NOW.load(Ordering::Relaxed));
    if !now.is_null() {
        // SAFETY: `now` points to a `time_t`.
        unsafe { now.write(seconds) };
    }
    seconds
}

/// Fills a buffer with random bytes.
pub type Random = fn(&mut [u8]);

/// The source of random bytes; until `set_random`, none (wolfSSL then fails to make keys).
static RANDOM: AtomicPtr<()> = AtomicPtr::new(core::ptr::null_mut());

/// Sets the source of random bytes: on the device its hardware generator, with the radio on.
pub fn set_random(random: Random) {
    RANDOM.store(random as *mut (), Ordering::Release);
}

/// # Safety
/// `output` points to `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_tls_random(output: *mut c_uchar, size: c_uint) -> c_int {
    let random = RANDOM.load(Ordering::Acquire);
    if random.is_null() || output.is_null() {
        return -1;
    }
    // SAFETY: only `set_random` stores here, from a `Random`.
    let random: Random = unsafe { core::mem::transmute::<*mut (), Random>(random) };
    // SAFETY: wolfSSL gives `size` writable bytes.
    let bytes = unsafe { core::slice::from_raw_parts_mut(output, size as usize) };
    random(bytes);
    0
}
