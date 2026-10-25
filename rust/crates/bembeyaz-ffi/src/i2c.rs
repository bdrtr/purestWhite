//! `pw_i2c` (see `rust/include/purestwhite/i2c.h`): the C++ side's I2C bus as embedded-hal's and
//! embedded-hal-async's `I2c` and `DelayNs`, for the purestwhite drivers. A bridge until the
//! drivers run on esp-hal: the C++ calls block (a pause yields to FreeRTOS), so the async
//! transfers are ready at once and [`block_on`] runs a driver's future to the end in one poll.
//!
//! Drivers may use only a write and a write then a read; other transfers fail.

use core::ffi::c_void;
use core::pin::pin;
use core::task::{Context, Poll, Waker};
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::{ErrorKind, ErrorType, I2c, Operation};
use embedded_hal_async::delay::DelayNs as AsyncDelayNs;
use embedded_hal_async::i2c::I2c as AsyncI2c;

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
pub(crate) struct Bus<'a>(pub(crate) &'a CBus);

/// The same `pw_i2c`'s pause, in whole milliseconds: shorter delays round up to one.
pub(crate) struct Delay<'a>(pub(crate) &'a CBus);

/// Runs a driver's future over the bridge, whose transfers and pauses are always ready, in one
/// poll; `None` for a future that is not (one that waits on something else), never a spin.
pub(crate) fn block_on<F: Future>(future: F) -> Option<F::Output> {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(value) => Some(value),
        Poll::Pending => None,
    }
}

fn result(ok: bool) -> Result<(), ErrorKind> {
    if ok { Ok(()) } else { Err(ErrorKind::Other) }
}

impl ErrorType for Bus<'_> {
    type Error = ErrorKind;
}

impl I2c for Bus<'_> {
    fn write(&mut self, addr: u8, bytes: &[u8]) -> Result<(), ErrorKind> {
        // SAFETY: `bytes` is valid for the call, as the callback expects.
        result(
            self.0
                .write
                .is_some_and(|f| unsafe { f(self.0.context, addr, bytes.as_ptr(), bytes.len()) }),
        )
    }

    fn write_read(&mut self, addr: u8, out: &[u8], buf: &mut [u8]) -> Result<(), ErrorKind> {
        // SAFETY: as `write`.
        result(self.0.write_read.is_some_and(|f| unsafe {
            f(
                self.0.context,
                addr,
                out.as_ptr(),
                out.len(),
                buf.as_mut_ptr(),
                buf.len(),
            )
        }))
    }

    /// The bridge makes the two transfers the drivers use: a write, and a write then a read.
    fn transaction(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
        match ops {
            [Operation::Write(bytes)] => I2c::write(self, addr, bytes),
            [Operation::Write(out), Operation::Read(buf)] => I2c::write_read(self, addr, out, buf),
            _ => Err(ErrorKind::Other),
        }
    }
}

impl DelayNs for Delay<'_> {
    fn delay_ns(&mut self, ns: u32) {
        DelayNs::delay_ms(self, ns.div_ceil(1_000_000));
    }

    fn delay_ms(&mut self, ms: u32) {
        if let Some(f) = self.0.pause_ms {
            // SAFETY: the callback takes its context.
            unsafe { f(self.0.context, ms) }
        }
    }
}

#[allow(
    clippy::unused_async,
    reason = "the C++ calls block; the futures are ready at once"
)]
impl AsyncI2c for Bus<'_> {
    async fn write(&mut self, addr: u8, bytes: &[u8]) -> Result<(), ErrorKind> {
        I2c::write(self, addr, bytes)
    }

    async fn write_read(&mut self, addr: u8, out: &[u8], buf: &mut [u8]) -> Result<(), ErrorKind> {
        I2c::write_read(self, addr, out, buf)
    }

    async fn transaction(&mut self, addr: u8, ops: &mut [Operation<'_>]) -> Result<(), ErrorKind> {
        I2c::transaction(self, addr, ops)
    }
}

impl AsyncDelayNs for Delay<'_> {
    async fn delay_ns(&mut self, ns: u32) {
        DelayNs::delay_ns(self, ns);
    }

    async fn delay_ms(&mut self, ms: u32) {
        DelayNs::delay_ms(self, ms);
    }
}
