//! The file views the format exports share (see `rust/include/bembeyaz/io.h`).

use crate::set;
use bembeyaz_io::{Sink, Source};
use core::ffi::c_void;

/// io.h's `bb_io_file`.
#[repr(C)]
pub struct FileView {
    context: *mut c_void,
    size: u32,
    read_at: Option<unsafe extern "C" fn(*mut c_void, u32, *mut u8, usize) -> bool>,
}

impl Source for &FileView {
    fn size(&self) -> u32 {
        self.size
    }
    fn read_at(&mut self, offset: u32, buf: &mut [u8]) -> bool {
        // SAFETY: the caller's callback fills `buf`, which holds `buf.len()` bytes.
        self.read_at
            .is_some_and(|read| unsafe { read(self.context, offset, buf.as_mut_ptr(), buf.len()) })
    }
}

/// io.h's `bb_io_sink`.
#[repr(C)]
pub struct SinkView {
    context: *mut c_void,
    write_at: Option<unsafe extern "C" fn(*mut c_void, u32, *const u8, usize) -> bool>,
}

impl Sink for &SinkView {
    fn write_at(&mut self, offset: u32, bytes: &[u8]) -> bool {
        // SAFETY: the caller's callback reads `bytes`.
        self.write_at.is_some_and(|write| unsafe {
            write(self.context, offset, bytes.as_ptr(), bytes.len())
        })
    }
}

// Runs `lookup` on the caller's file and writes its answer.
//
// SAFETY: `file` is null or a `bb_io_file` as io.h describes; `out` is null or writable.
pub(crate) unsafe fn answer<T>(
    file: *const FileView,
    out: *mut T,
    lookup: impl FnOnce(&mut &FileView) -> Option<T>,
) -> bool {
    // SAFETY: as above.
    let Some(mut f) = (unsafe { file.as_ref() }) else {
        return false;
    };
    match lookup(&mut f) {
        Some(value) => {
            // SAFETY: as above.
            unsafe { set(out, value) };
            true
        }
        None => false,
    }
}

// Writes with `write` at `*at`, moving it on.
//
// SAFETY: `sink` is null or a `bb_io_sink` as io.h describes; `at` is null or readable and
// writable.
pub(crate) unsafe fn write_with(
    sink: *const SinkView,
    at: *mut u32,
    write: impl FnOnce(&mut &SinkView, u32) -> Option<u32>,
) -> bool {
    // SAFETY: as above.
    let (Some(mut s), Some(at)) = (unsafe { sink.as_ref() }, unsafe { at.as_mut() }) else {
        return false;
    };
    match write(&mut s, *at) {
        Some(end) => {
            *at = end;
            true
        }
        None => false,
    }
}
