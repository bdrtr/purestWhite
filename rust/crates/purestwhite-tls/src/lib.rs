//! TLS for the bare-metal firmware (gate 2, rust/firmware/ROADMAP.md): wolfSSL 5.7.2, the C++
//! firmware's, built by `build.rs` from `rust/third_party/wolfssl` with `c/user_settings.h`, and a
//! small client over it. The client runs over memory buffers, not sockets: whoever owns the
//! socket feeds it the server's bytes (`Connection::received`) and sends what it wrote
//! (`Connection::to_send`), so wolfSSL's blocking calls fit an async executor. What an operating
//! system would give wolfSSL comes from Rust: memory from the global allocator, the time
//! (`set_time`, which certificates' dates are checked against) and random bytes (`set_random`).
//!
//! The server's chain is checked against the roots a `Context` trusts, and its name against the
//! host the `Connection` was made for.
//!
//! wolfSSL is built `SINGLE_THREADED`: its globals have no locks, so it is to be used from one
//! thread (one core) at a time. The types are neither `Send` nor `Sync`, which keeps a session on
//! its thread, but two threads must not make contexts at once either.
//!
//! wolfSSL is GPLv2 or any later version (or commercial): an image that links it is a combined
//! work distributed under the GPL's terms, with wolfSSL's source and this build's settings
//! (`build.rs`, `c/user_settings.h`; the submodule pins the source), as the C++ firmware already
//! is. The crate's own code stays MIT.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod client;
mod ffi;
mod hooks;
mod pipe;
#[cfg(feature = "test-server")]
#[doc(hidden)]
pub mod test_server;
#[cfg(test)]
mod tests;

use core::sync::atomic::{AtomicBool, Ordering};

pub use client::{Connection, Context};
pub use hooks::{Random, set_random, set_time};

/// Why a TLS step failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// wolfSSL could not start.
    Init,
    /// No memory for a context or a session.
    Memory,
    /// The trusted roots could not be read.
    Roots,
    /// The server's name is empty, too long, or holds a NUL.
    Name,
    /// wolfSSL's error code (`wolfssl/error-ssl.h`, `wolfssl/wolfcrypt/error-crypt.h`): -188 for a
    /// chain no trusted root signed, -322 for a name that does not match, -150 and -151 for a
    /// certificate before or after its dates, and so on.
    Wolf(i32),
}

/// Whether wolfSSL started.
static STARTED: AtomicBool = AtomicBool::new(false);

/// Starts wolfSSL, once (`Context::new` calls it).
///
/// # Errors
/// wolfSSL could not start.
pub fn init() -> Result<(), Error> {
    if STARTED.load(Ordering::Acquire) {
        return Ok(());
    }
    // SAFETY: one thread (SINGLE_THREADED); a second call is harmless in wolfSSL too.
    if unsafe { ffi::wolfSSL_Init() } != ffi::SUCCESS {
        return Err(Error::Init);
    }
    STARTED.store(true, Ordering::Release);
    Ok(())
}
