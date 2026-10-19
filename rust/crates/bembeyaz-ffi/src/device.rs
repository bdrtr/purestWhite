//! Runtime glue for the firmware build: Rust allocates from the ESP-IDF heap
//! the C++ code uses, and a panic aborts through ESP-IDF so it produces the
//! usual backtrace and core dump.

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_void};
use core::panic::PanicInfo;
use core::ptr::null_mut;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn esp_rom_printf(fmt: *const c_char, ...) -> i32;
    fn abort() -> !;
}

// ESP-IDF malloc guarantees 4-byte alignment on both the C3 and the S3.
const MALLOC_ALIGN: usize = 4;
// alloc_aligned keeps the malloc block's address in the bytes just before the result.
const _: () = assert!(core::mem::size_of::<*mut u8>() <= MALLOC_ALIGN);

struct IdfHeap;

unsafe impl GlobalAlloc for IdfHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.align() <= MALLOC_ALIGN {
            unsafe { malloc(layout.size()).cast() }
        } else {
            alloc_aligned(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            if layout.align() <= MALLOC_ALIGN {
                free(ptr.cast());
            } else {
                free_aligned(ptr);
            }
        }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            if layout.align() <= MALLOC_ALIGN {
                realloc(ptr.cast(), new_size).cast()
            } else {
                realloc_aligned(ptr, layout, new_size)
            }
        }
    }
}

// The allocator is inlined at every allocation site: the rare wider alignments stay out of line.

/// A block aligned above 4 bytes is cut from a malloc block `align` bytes larger, so it lands where
/// malloc puts that block: on the S3, when `size + align` is above 4096 bytes, in PSRAM first
/// (RadioPolicy.h). The word just before it holds the malloc block's address.
/// (heap_caps_aligned_alloc tries internal RAM first at any size; memalign adds 256 bytes to IRAM.)
#[inline(never)]
fn alloc_aligned(layout: Layout) -> *mut u8 {
    let Some(size) = layout.size().checked_add(layout.align()) else {
        return null_mut();
    };
    // SAFETY: the block holds `align` bytes more than the layout, and the result starts 4 to
    // `align` bytes in (the block is 4-aligned and `align` a larger power of two), so the word
    // before it is inside the block and 4-aligned.
    unsafe {
        let block = malloc(size).cast::<u8>();
        if block.is_null() {
            return block;
        }
        let aligned = block.add(layout.align() - (block.addr() & (layout.align() - 1)));
        aligned.cast::<*mut u8>().sub(1).write(block);
        aligned
    }
}

/// Frees what [`alloc_aligned`] returned.
#[inline(never)]
unsafe fn free_aligned(ptr: *mut u8) {
    unsafe { free(ptr.cast::<*mut u8>().sub(1).read().cast()) }
}

#[inline(never)]
unsafe fn realloc_aligned(ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    unsafe {
        let new_ptr = alloc_aligned(Layout::from_size_align_unchecked(new_size, layout.align()));
        if !new_ptr.is_null() {
            core::ptr::copy_nonoverlapping(ptr, new_ptr, layout.size().min(new_size));
            free_aligned(ptr);
        }
        new_ptr
    }
}

#[global_allocator]
static HEAP: IdfHeap = IdfHeap;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    // The ROM printf has no precision support, so copy the file name into a
    // NUL-terminated buffer instead of formatting the message.
    let mut file = [0u8; 64];
    let mut line = 0;
    if let Some(location) = info.location() {
        let name = location.file().as_bytes();
        let len = name.len().min(file.len() - 1);
        file[..len].copy_from_slice(&name[..len]);
        line = location.line();
    }
    unsafe {
        esp_rom_printf(c"Rust panic at %s:%u\n".as_ptr(), file.as_ptr(), line);
        abort()
    }
}
