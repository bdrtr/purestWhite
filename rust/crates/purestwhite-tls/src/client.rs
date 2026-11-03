//! A TLS client over memory buffers: a `Context` holds the roots trusted, a `Connection` one
//! session with a server, its certificate chain and name checked against them.

use crate::ffi::{self, Wolf, WolfCtx};
use crate::pipe::{self, Pipe};
use crate::{Error, init};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{c_int, c_long, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

/// The certificates trusted to sign servers'.
pub struct Context {
    ctx: NonNull<WolfCtx>,
}

impl Context {
    /// A client context trusting `roots` (PEM, one or more certificates), checking each server's
    /// chain against them, and its dates against the time (`set_time`). A root out of its own
    /// dates is loaded all the same, so that one stale root does not take the others with it, and
    /// still anchors chains (wolfSSL does not check a trusted root's dates, as is usual).
    ///
    /// # Errors
    /// wolfSSL cannot start, or the roots cannot be read.
    pub fn new(roots: &[u8]) -> Result<Self, Error> {
        init()?;
        // SAFETY: wolfSSL_CTX_new takes the method (and frees it if it fails); the context it makes
        // is owned here.
        let ctx = NonNull::new(unsafe { ffi::wolfSSL_CTX_new(ffi::wolfSSLv23_client_method()) })
            .ok_or(Error::Memory)?;
        let context = Context { ctx };
        let size = c_long::try_from(roots.len()).map_err(|_| Error::Roots)?;
        // SAFETY: `roots` holds `size` bytes, read during the call only.
        let loaded = unsafe {
            ffi::wolfSSL_CTX_load_verify_buffer_ex(
                ctx.as_ptr(),
                roots.as_ptr(),
                size,
                ffi::FILETYPE_PEM,
                0,
                ffi::LOAD_FLAG_DATE_ERR_OKAY,
            )
        };
        if loaded != ffi::SUCCESS {
            return Err(Error::Roots);
        }
        // SAFETY: a valid context; no callback.
        unsafe { ffi::wolfSSL_CTX_set_verify(ctx.as_ptr(), ffi::VERIFY_PEER, None) };
        Ok(context)
    }

    pub(crate) fn as_ptr(&self) -> *mut WolfCtx {
        self.ctx.as_ptr()
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the context is owned here; every connection borrows it, so none outlives it.
        unsafe { ffi::wolfSSL_CTX_free(self.ctx.as_ptr()) };
    }
}

/// One session with a server. Its `read` and `write` may leave bytes to send too (a TLS 1.3 key
/// update's answer, say): send `to_send` after every call.
pub struct Connection<'c> {
    ssl: NonNull<Wolf>,
    /// The bytes on the wire, on the heap, from `Box::into_raw`: wolfSSL's callbacks and this side
    /// both reach it through this one pointer, never through a `Box` (whose moves and uses would
    /// invalidate the pointer wolfSSL holds). Freed in `drop`, after the session.
    pipe: NonNull<Pipe>,
    _context: PhantomData<&'c Context>,
}

impl<'c> Connection<'c> {
    /// A session with the server named `host` (sent as SNI, and matched against its
    /// certificate), over memory buffers.
    ///
    /// # Errors
    /// No memory, or a name wolfSSL refuses.
    pub fn new(context: &'c Context, host: &str) -> Result<Self, Error> {
        // SAFETY: a valid context, which outlives the session (`'c`).
        let ssl =
            NonNull::new(unsafe { ffi::wolfSSL_new(context.as_ptr()) }).ok_or(Error::Memory)?;
        let pipe = NonNull::from(Box::leak(Box::<Pipe>::default()));
        let mut connection = Connection {
            ssl,
            pipe,
            _context: PhantomData,
        };
        connection.wire();
        connection.name(host)?;
        Ok(connection)
    }

    /// The I/O callbacks, on this connection's pipe.
    fn wire(&mut self) {
        let ctx: *mut c_void = self.pipe.as_ptr().cast();
        // SAFETY: the pipe stays at this address, unmoved, as long as the session.
        unsafe {
            ffi::wolfSSL_SSLSetIORecv(self.ssl.as_ptr(), pipe::recv);
            ffi::wolfSSL_SSLSetIOSend(self.ssl.as_ptr(), pipe::send);
            ffi::wolfSSL_SetIOReadCtx(self.ssl.as_ptr(), ctx);
            ffi::wolfSSL_SetIOWriteCtx(self.ssl.as_ptr(), ctx);
        }
    }

    /// The server's name, for SNI and for the certificate's name check.
    fn name(&mut self, host: &str) -> Result<(), Error> {
        let mut name: Vec<u8> = Vec::with_capacity(host.len() + 1);
        name.extend_from_slice(host.as_bytes());
        name.push(0);
        let size = u16::try_from(host.len()).map_err(|_| Error::Name)?;
        if host.is_empty() || host.bytes().any(|b| b == 0) {
            return Err(Error::Name);
        }
        // SAFETY: `name` is NUL-terminated and read during the calls only (wolfSSL copies it).
        let (sni, check) = unsafe {
            (
                ffi::wolfSSL_UseSNI(
                    self.ssl.as_ptr(),
                    ffi::SNI_HOST_NAME,
                    name.as_ptr().cast(),
                    size,
                ),
                ffi::wolfSSL_check_domain_name(self.ssl.as_ptr(), name.as_ptr().cast()),
            )
        };
        if sni == ffi::SUCCESS && check == ffi::SUCCESS {
            Ok(())
        } else {
            Err(Error::Name)
        }
    }

    /// The pipe, between wolfSSL's calls.
    fn pipe(&mut self) -> &mut Pipe {
        // SAFETY: the pipe lives as long as `self`; wolfSSL touches it only inside its calls,
        // which `&mut self` keeps from overlapping this borrow.
        unsafe { self.pipe.as_mut() }
    }

    /// Bytes received from the server, for wolfSSL to read.
    pub fn received(&mut self, bytes: &[u8]) {
        self.pipe().receive(bytes);
    }

    /// The server closed its side: what wolfSSL still wants to read fails.
    pub fn closed(&mut self) {
        self.pipe().closed = true;
    }

    /// The bytes to send to the server; `sent` takes them off once sent.
    #[must_use]
    pub fn to_send(&self) -> &[u8] {
        // SAFETY: as `pipe`; the slice cannot outlive a call that takes `&mut self`.
        unsafe { &self.pipe.as_ref().outbound }
    }

    /// The first `n` bytes of `to_send` were sent.
    pub fn sent(&mut self, n: usize) {
        let outbound = &mut self.pipe().outbound;
        let n = n.min(outbound.len());
        outbound.drain(..n);
    }

    /// Goes on with the handshake: `true` once it is done; `false` while it waits for the
    /// server's bytes (send `to_send` first).
    ///
    /// # Errors
    /// The handshake failed: wolfSSL's code (a certificate that does not verify, say).
    pub fn handshake(&mut self) -> Result<bool, Error> {
        // SAFETY: a valid session.
        let ret = unsafe { ffi::wolfSSL_connect(self.ssl.as_ptr()) };
        self.progress(ret).map(|done| done.is_some())
    }

    /// Writes `data` (after the handshake): how much was taken.
    ///
    /// # Errors
    /// As `handshake`.
    pub fn write(&mut self, data: &[u8]) -> Result<usize, Error> {
        if data.is_empty() {
            return Ok(0);
        }
        let size = c_int::try_from(data.len()).unwrap_or(c_int::MAX);
        // SAFETY: `data` holds at least `size` bytes, read during the call only.
        let ret = unsafe { ffi::wolfSSL_write(self.ssl.as_ptr(), data.as_ptr().cast(), size) };
        self.progress(ret).map(Option::unwrap_or_default)
    }

    /// Reads into `buf`: `Some(n)` bytes (`Some(0)`: the server closed the session cleanly, or
    /// `buf` is empty), or `None` while it waits for the server's bytes.
    ///
    /// # Errors
    /// As `handshake`.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Error> {
        if buf.is_empty() {
            return Ok(Some(0));
        }
        let size = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: `buf` holds at least `size` writable bytes.
        let ret = unsafe { ffi::wolfSSL_read(self.ssl.as_ptr(), buf.as_mut_ptr().cast(), size) };
        match self.progress(ret) {
            Err(Error::Wolf(code)) if code == ffi::ERROR_ZERO_RETURN => Ok(Some(0)),
            other => other,
        }
    }

    /// What a call's return means: `Some(n)` done with `n`, `None` waiting for bytes, or the error.
    fn progress(&mut self, ret: c_int) -> Result<Option<usize>, Error> {
        if ret > 0 {
            return Ok(Some(usize::try_from(ret).unwrap_or(0)));
        }
        // SAFETY: a valid session.
        let code = unsafe { ffi::wolfSSL_get_error(self.ssl.as_ptr(), ret) };
        match code {
            ffi::ERROR_WANT_READ | ffi::ERROR_WANT_WRITE => Ok(None),
            code => Err(Error::Wolf(code)),
        }
    }
}

impl Drop for Connection<'_> {
    fn drop(&mut self) {
        // SAFETY: the session is owned here and freed first; then nothing holds the pipe, which
        // came from `Box::leak` in `new`.
        unsafe {
            ffi::wolfSSL_free(self.ssl.as_ptr());
            drop(Box::from_raw(self.pipe.as_ptr()));
        }
    }
}
