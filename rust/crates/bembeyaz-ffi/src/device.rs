//! Runtime glue for the firmware build: Rust allocates from the ESP-IDF heap
//! the C++ code uses, and a panic aborts through ESP-IDF so it produces the
//! usual backtrace and core dump.

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::{c_char, c_void};
use core::panic::PanicInfo;

unsafe extern "C" {
    fn malloc(size: usize) -> *mut c_void;
    fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void;
    fn free(ptr: *mut c_void);
    fn heap_caps_aligned_alloc(alignment: usize, size: usize, caps: u32) -> *mut c_void;
    fn esp_rom_printf(fmt: *const c_char, ...) -> i32;
    fn abort() -> !;
}

// ESP-IDF malloc guarantees 4-byte alignment on both the C3 and the S3.
const MALLOC_ALIGN: usize = 4;
const MALLOC_CAP_DEFAULT: u32 = 1 << 12;

struct IdfHeap;

unsafe impl GlobalAlloc for IdfHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            if layout.align() <= MALLOC_ALIGN {
                malloc(layout.size()).cast()
            } else {
                heap_caps_aligned_alloc(layout.align(), layout.size(), MALLOC_CAP_DEFAULT).cast()
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        // ESP-IDF 5 frees aligned allocations with plain free().
        unsafe { free(ptr.cast()) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            if layout.align() <= MALLOC_ALIGN {
                return realloc(ptr.cast(), new_size).cast();
            }
            let new_layout = Layout::from_size_align_unchecked(new_size, layout.align());
            let new_ptr = self.alloc(new_layout);
            if !new_ptr.is_null() {
                core::ptr::copy_nonoverlapping(ptr, new_ptr, layout.size().min(new_size));
                self.dealloc(ptr, layout);
            }
            new_ptr
        }
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
