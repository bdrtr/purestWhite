//! `pw_i2c` (see `rust/include/purestwhite/i2c.h`): the C++ side's I2C bus, for the purestwhite
//! drivers.

use core::ffi::c_void;
use purestwhite_i2c::Bus;

/// `pw_i2c`.
#[repr(C)]
pub struct CBus {
    context: *mut c_void,
    write: Option<unsafe extern "C" fn(*mut c_void, u8, *const u8, usize) -> bool>,
    write_read:
        Option<unsafe extern "C" fn(*mut c_void, u8, *const u8, usize, *mut u8, usize) -> bool>,
    pause_ms: Option<unsafe extern "C" fn(*mut c_void, u32)>,
}

// As i2c.h's, which lib/hal/WireBus.h asserts too.
const _: () = assert!(size_of::<CBus>() == 4 * size_of::<usize>());

/// A `pw_i2c` whose callbacks may be called with its context and buffers valid for the call.
pub(crate) struct Callbacks<'a>(pub(crate) &'a CBus);

impl Bus for Callbacks<'_> {
    fn write(&mut self, addr: u8, bytes: &[u8]) -> bool {
        // SAFETY: `bytes` is valid for the call, as the callback expects.
        self.0
            .write
            .is_some_and(|f| unsafe { f(self.0.context, addr, bytes.as_ptr(), bytes.len()) })
    }
    fn write_read(&mut self, addr: u8, out: &[u8], buf: &mut [u8]) -> bool {
        // SAFETY: as `write`.
        self.0.write_read.is_some_and(|f| unsafe {
            f(
                self.0.context,
                addr,
                out.as_ptr(),
                out.len(),
                buf.as_mut_ptr(),
                buf.len(),
            )
        })
    }
    fn pause_ms(&mut self, ms: u32) {
        if let Some(f) = self.0.pause_ms {
            // SAFETY: as `write`.
            unsafe { f(self.0.context, ms) }
        }
    }
}
