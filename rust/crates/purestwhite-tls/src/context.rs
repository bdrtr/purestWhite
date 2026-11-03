//! What a client trusts: PEM roots loaded into one wolfSSL context, or ESP-IDF's certificate
//! bundle, checked by each connection's own wolfSSL context as the C++ firmware does.

use crate::ffi::{self, WolfCtx};
use crate::{Error, init};
use core::ffi::c_long;
use core::ptr::NonNull;
use purestwhite_x509::Bundle;

/// The certificates trusted to sign servers'.
pub struct Context {
    pub(crate) trust: Trust,
}

/// What a context trusts.
pub(crate) enum Trust {
    /// PEM roots, loaded into this wolfSSL context, which every connection shares.
    Roots(NonNull<WolfCtx>),
    /// A bundle: each connection makes a wolfSSL context of its own (`crate::bundle::Check`).
    Bundle(Bundle<'static>),
}

/// A wolfSSL client context, verifying the server (no callback yet), owned by the caller.
pub(crate) fn client_ctx() -> Result<NonNull<WolfCtx>, Error> {
    init()?;
    // SAFETY: wolfSSL_CTX_new takes the method (and frees it if it fails); the context it makes is
    // the caller's to free.
    let ctx = NonNull::new(unsafe { ffi::wolfSSL_CTX_new(ffi::wolfSSLv23_client_method()) })
        .ok_or(Error::Memory)?;
    // SAFETY: a valid context; no callback.
    unsafe { ffi::wolfSSL_CTX_set_verify(ctx.as_ptr(), ffi::VERIFY_PEER, None) };
    Ok(ctx)
}

impl Context {
    /// A client context trusting `roots` (PEM, one or more certificates), checking each server's
    /// chain against them, and its dates against the time (`set_time`). A root out of its own
    /// dates is loaded all the same, so that one stale root does not take the others with it, and
    /// still anchors chains (wolfSSL does not check a trusted root's dates, as is usual). The
    /// topmost certificate a server sends must be signed by a loaded root (or be one): a server
    /// that sends one more above it, signed by a root not loaded, is refused, as by the C++
    /// firmware's pinned roots.
    ///
    /// # Errors
    /// wolfSSL cannot start, or the roots cannot be read.
    pub fn new(roots: &[u8]) -> Result<Self, Error> {
        let ctx = client_ctx()?;
        let context = Context {
            trust: Trust::Roots(ctx),
        };
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
        Ok(context)
    }

    /// A client context trusting the roots of `bundle` (ESP-IDF's certificate bundle format,
    /// `purestwhite_x509::ESP_IDF_BUNDLE` for the C++ firmware's), checked as the C++ firmware
    /// checks them (`crate::bundle`). wolfSSL loads no root: the bundle stays where it is, in
    /// flash on the device. Each connection makes a wolfSSL context of its own, as the C++
    /// firmware makes one per connection: the intermediates one connection's check accepts go
    /// with it, and every connection asks the bundle again.
    ///
    /// # Errors
    /// wolfSSL cannot start, or the bundle is not one (`Error::Roots`).
    pub fn bundle(bundle: &'static [u8]) -> Result<Self, Error> {
        let bundle = Bundle::new(bundle).map_err(|_| Error::Roots)?;
        init()?;
        Ok(Context {
            trust: Trust::Bundle(bundle),
        })
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if let Trust::Roots(ctx) = self.trust {
            // SAFETY: the context is owned here; every connection borrows this `Context`, so none
            // outlives it.
            unsafe { ffi::wolfSSL_CTX_free(ctx.as_ptr()) };
        }
    }
}
