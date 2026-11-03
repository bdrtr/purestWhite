//! A wolfSSL server over memory buffers, for the host tests to shake hands with (the
//! `test-server` feature; the firmware never builds it).

use crate::ffi::{self, Wolf, WolfCtx};
use crate::pipe::{self, Pipe};
use crate::{Error, init};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ffi::{c_int, c_long, c_void};
use core::ptr::NonNull;

/// A server with one certificate (and its chain's next certificates) and its key, PEM.
pub struct Server {
    ctx: NonNull<WolfCtx>,
    ssl: NonNull<Wolf>,
    /// As `Connection`'s: from `Box::leak`, reached only through this pointer.
    pipe: NonNull<Pipe>,
}

impl Server {
    /// A server speaking TLS 1.3 or 1.2.
    ///
    /// # Errors
    /// wolfSSL cannot start, or the certificate or key cannot be read.
    pub fn new(chain: &[u8], key: &[u8]) -> Result<Self, Error> {
        init()?;
        // SAFETY: a method table, which `Server::with` hands to `wolfSSL_CTX_new`.
        Self::with(unsafe { ffi::wolfSSLv23_server_method() }, chain, key)
    }

    /// A server speaking TLS 1.2 only.
    ///
    /// # Errors
    /// As `new`.
    pub fn tls12(chain: &[u8], key: &[u8]) -> Result<Self, Error> {
        init()?;
        // SAFETY: as `new`.
        Self::with(unsafe { ffi::wolfTLSv1_2_server_method() }, chain, key)
    }

    fn with(method: *mut ffi::WolfMethod, chain: &[u8], key: &[u8]) -> Result<Self, Error> {
        // SAFETY: as `Context::new`.
        let ctx = NonNull::new(unsafe { ffi::wolfSSL_CTX_new(method) }).ok_or(Error::Memory)?;
        let len = |b: &[u8]| c_long::try_from(b.len()).unwrap_or(c_long::MAX);
        // SAFETY: the buffers are read during the calls only.
        let loaded = unsafe {
            ffi::wolfSSL_CTX_use_certificate_chain_buffer(ctx.as_ptr(), chain.as_ptr(), len(chain))
                == ffi::SUCCESS
                && ffi::wolfSSL_CTX_use_PrivateKey_buffer(ctx.as_ptr(), key.as_ptr(), len(key), 1)
                    == ffi::SUCCESS
        };
        // SAFETY: a valid context.
        let ssl = NonNull::new(unsafe { ffi::wolfSSL_new(ctx.as_ptr()) });
        let (Some(ssl), true) = (ssl, loaded) else {
            // SAFETY: neither is used again; the session goes before its context.
            unsafe {
                if let Some(ssl) = ssl {
                    ffi::wolfSSL_free(ssl.as_ptr());
                }
                ffi::wolfSSL_CTX_free(ctx.as_ptr());
            }
            return Err(Error::Roots);
        };
        let server = Server {
            ctx,
            ssl,
            pipe: NonNull::from(Box::leak(Box::<Pipe>::default())),
        };
        let pipe_ptr: *mut c_void = server.pipe.as_ptr().cast();
        // SAFETY: the pipe stays at this address as long as the session.
        unsafe {
            ffi::wolfSSL_SSLSetIORecv(ssl.as_ptr(), pipe::recv);
            ffi::wolfSSL_SSLSetIOSend(ssl.as_ptr(), pipe::send);
            ffi::wolfSSL_SetIOReadCtx(ssl.as_ptr(), pipe_ptr);
            ffi::wolfSSL_SetIOWriteCtx(ssl.as_ptr(), pipe_ptr);
        }
        Ok(server)
    }

    fn pipe(&mut self) -> &mut Pipe {
        // SAFETY: as `Connection::pipe`.
        unsafe { self.pipe.as_mut() }
    }

    pub fn received(&mut self, bytes: &[u8]) {
        self.pipe().receive(bytes);
    }

    /// The bytes to send, taken.
    pub fn take_out(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.pipe().outbound)
    }

    /// `Ok(true)` once accepted, `Ok(false)` while it waits.
    ///
    /// # Errors
    /// wolfSSL's code.
    pub fn accept(&mut self) -> Result<bool, Error> {
        // SAFETY: a valid session.
        let ret = unsafe { ffi::wolfSSL_accept(self.ssl.as_ptr()) };
        self.result(ret).map(|n| n.is_some())
    }

    /// # Errors
    /// wolfSSL's code.
    pub fn write(&mut self, data: &[u8]) -> Result<usize, Error> {
        let size = c_int::try_from(data.len()).unwrap_or(c_int::MAX);
        // SAFETY: `data` is read during the call only.
        let ret = unsafe { ffi::wolfSSL_write(self.ssl.as_ptr(), data.as_ptr().cast(), size) };
        self.result(ret).map(Option::unwrap_or_default)
    }

    /// # Errors
    /// wolfSSL's code.
    pub fn read(&mut self, buf: &mut [u8]) -> Result<Option<usize>, Error> {
        let size = c_int::try_from(buf.len()).unwrap_or(c_int::MAX);
        // SAFETY: `buf` is written during the call only.
        let ret = unsafe { ffi::wolfSSL_read(self.ssl.as_ptr(), buf.as_mut_ptr().cast(), size) };
        self.result(ret)
    }

    fn result(&mut self, ret: c_int) -> Result<Option<usize>, Error> {
        if ret > 0 {
            return Ok(Some(usize::try_from(ret).unwrap_or(0)));
        }
        // SAFETY: a valid session.
        match unsafe { ffi::wolfSSL_get_error(self.ssl.as_ptr(), ret) } {
            ffi::ERROR_WANT_READ | ffi::ERROR_WANT_WRITE => Ok(None),
            code => Err(Error::Wolf(code)),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // SAFETY: owned here; the session goes before its context and its pipe.
        unsafe {
            ffi::wolfSSL_free(self.ssl.as_ptr());
            ffi::wolfSSL_CTX_free(self.ctx.as_ptr());
            drop(Box::from_raw(self.pipe.as_ptr()));
        }
    }
}
