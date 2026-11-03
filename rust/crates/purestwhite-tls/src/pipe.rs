//! The bytes on the wire, in memory: wolfSSL's I/O callbacks read what came in and add what is to
//! go out; whoever owns the socket moves them (`Connection::received`, `Connection::to_send`).

use crate::ffi::{CBIO_ERR_CONN_CLOSE, CBIO_ERR_WANT_READ, Wolf};
use alloc::vec::Vec;
use core::ffi::{c_char, c_int, c_void};

/// Both directions' bytes.
#[derive(Default)]
pub(crate) struct Pipe {
    /// Received, not yet read by wolfSSL; `read` bytes of it are.
    inbound: Vec<u8>,
    read: usize,
    /// Written by wolfSSL, not yet sent.
    pub(crate) outbound: Vec<u8>,
    /// The peer closed its side.
    pub(crate) closed: bool,
}

impl Pipe {
    pub(crate) fn receive(&mut self, bytes: &[u8]) {
        // What wolfSSL read goes first, so the buffer holds only what is still to read.
        self.inbound.drain(..self.read);
        self.read = 0;
        self.inbound.extend_from_slice(bytes);
    }

    /// Up to `into.len()` received bytes, taken.
    fn take(&mut self, into: &mut [u8]) -> usize {
        let waiting = self.inbound.get(self.read..).unwrap_or_default();
        let n = waiting.len().min(into.len());
        if let (Some(to), Some(from)) = (into.get_mut(..n), waiting.get(..n)) {
            to.copy_from_slice(from);
        }
        self.read += n;
        n
    }
}

/// `CallbackIORecv`: received bytes, or "wait for more", or "closed".
///
/// # Safety
/// `ctx` is the connection's `Pipe` (`Connection::new` sets it), and `buf` holds `sz` bytes.
pub(crate) unsafe extern "C" fn recv(
    _ssl: *mut Wolf,
    buf: *mut c_char,
    sz: c_int,
    ctx: *mut c_void,
) -> c_int {
    // SAFETY: as documented above; nothing else touches the pipe while wolfSSL runs.
    let (pipe, into) = unsafe {
        (
            &mut *ctx.cast::<Pipe>(),
            core::slice::from_raw_parts_mut(buf.cast::<u8>(), usize::try_from(sz).unwrap_or(0)),
        )
    };
    match pipe.take(into) {
        0 if pipe.closed => CBIO_ERR_CONN_CLOSE,
        0 => CBIO_ERR_WANT_READ,
        n => c_int::try_from(n).unwrap_or(c_int::MAX),
    }
}

/// `CallbackIOSend`: the bytes kept to send; never waits.
///
/// # Safety
/// As `recv`.
pub(crate) unsafe extern "C" fn send(
    _ssl: *mut Wolf,
    buf: *mut c_char,
    sz: c_int,
    ctx: *mut c_void,
) -> c_int {
    // SAFETY: as `recv`.
    let (pipe, from) = unsafe {
        (
            &mut *ctx.cast::<Pipe>(),
            core::slice::from_raw_parts(buf.cast::<u8>(), usize::try_from(sz).unwrap_or(0)),
        )
    };
    if pipe.closed {
        return CBIO_ERR_CONN_CLOSE;
    }
    pipe.outbound.extend_from_slice(from);
    sz
}
