//! A TLS session over memory buffers, its server's certificate chain and name checked against
//! what its `Context` trusts.

use crate::Error;
use crate::bundle::{self, Check};
use crate::context::{Context, Trust, client_ctx};
use crate::ffi::{self, Wolf, WolfCtx};
use crate::pipe::{self, Pipe};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{c_int, c_void};
use core::marker::PhantomData;
use core::ptr::NonNull;

/// A bundle connection's own wolfSSL context and the check its verify callback reads, both freed
/// with it (after its session).
struct Own {
    ctx: NonNull<WolfCtx>,
    /// From `Box::leak`, reached only through this pointer (wolfSSL holds it too).
    check: NonNull<Check>,
}

impl Own {
    /// A wolfSSL context checking servers against `bundle`.
    fn new(bundle: purestwhite_x509::Bundle<'static>) -> Result<Own, Error> {
        let ctx = client_ctx()?;
        let check = NonNull::from(Box::leak(Box::new(Check::new(bundle))));
        // SAFETY: a valid context; the check outlives it (freed after it, in `drop`), and the
        // callback only reads it (its flag is a `Cell`).
        unsafe {
            ffi::wolfSSL_CTX_set_verify(
                ctx.as_ptr(),
                ffi::VERIFY_PEER,
                Some(bundle::pw_tls_verify),
            );
            ffi::wolfSSL_CTX_SetCertCbCtx(ctx.as_ptr(), check.as_ptr().cast());
        }
        Ok(Own { ctx, check })
    }

    fn check(&self) -> &Check {
        // SAFETY: the check lives as long as `self`; nothing takes `&mut` of it.
        unsafe { self.check.as_ref() }
    }
}

impl Drop for Own {
    fn drop(&mut self) {
        // SAFETY: owned here; the session made from the context is freed before (`Connection`'s
        // `drop` runs before its fields'), then nothing holds the check.
        unsafe {
            ffi::wolfSSL_CTX_free(self.ctx.as_ptr());
            drop(Box::from_raw(self.check.as_ptr()));
        }
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
    /// For a bundle context: the connection's own wolfSSL context and check.
    own: Option<Own>,
    _context: PhantomData<&'c Context>,
}

impl<'c> Connection<'c> {
    /// A session with the server named `host` (sent as SNI, and matched against its
    /// certificate), over memory buffers.
    ///
    /// # Errors
    /// No memory, or a name wolfSSL refuses.
    pub fn new(context: &'c Context, host: &str) -> Result<Self, Error> {
        let (ctx, own) = match &context.trust {
            Trust::Roots(ctx) => (*ctx, None),
            Trust::Bundle(bundle) => {
                let own = Own::new(*bundle)?;
                (own.ctx, Some(own))
            }
        };
        // SAFETY: a valid context, which outlives the session: the shared one lives `'c`, a
        // connection's own is freed after the session.
        let ssl = NonNull::new(unsafe { ffi::wolfSSL_new(ctx.as_ptr()) }).ok_or(Error::Memory)?;
        let pipe = NonNull::from(Box::leak(Box::<Pipe>::default()));
        let mut connection = Connection {
            ssl,
            pipe,
            own,
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
        let short = self
            .own
            .as_ref()
            .is_some_and(|own| own.check().short_of_memory());
        match code {
            ffi::ERROR_WANT_READ | ffi::ERROR_WANT_WRITE => Ok(None),
            // The bundle check ran out of memory: not a chain it refused.
            bundle::NO_SIGNER if short => Err(Error::Memory),
            code => Err(Error::Wolf(code)),
        }
    }
}

impl Drop for Connection<'_> {
    fn drop(&mut self) {
        // SAFETY: the session is owned here and freed first; then nothing holds the pipe, which
        // came from `Box::leak` in `new` (a bundle connection's own context and check go after,
        // with `own`).
        unsafe {
            ffi::wolfSSL_free(self.ssl.as_ptr());
            drop(Box::from_raw(self.pipe.as_ptr()));
        }
    }
}
