//! The part of wolfSSL's C API the client uses (wolfssl/ssl.h, wolfssl/wolfio.h), declared by
//! hand; c/shim.c checks the constants against the headers at build time.

use core::ffi::{c_char, c_int, c_long, c_uchar, c_void};

/// `WOLFSSL_CTX`: opaque.
#[repr(C)]
pub struct WolfCtx {
    _opaque: [u8; 0],
}

/// `WOLFSSL`: opaque.
#[repr(C)]
pub struct Wolf {
    _opaque: [u8; 0],
}

/// `WOLFSSL_METHOD`: opaque.
#[repr(C)]
pub struct WolfMethod {
    _opaque: [u8; 0],
}

pub const SUCCESS: c_int = 1;
pub const FILETYPE_PEM: c_int = 1;
pub const VERIFY_PEER: c_int = 1;
pub const ERROR_WANT_READ: c_int = 2;
pub const ERROR_WANT_WRITE: c_int = 3;
pub const ERROR_ZERO_RETURN: c_int = 6;
pub const CBIO_ERR_WANT_READ: c_int = -2;
pub const CBIO_ERR_CONN_CLOSE: c_int = -5;
/// `WOLFSSL_LOAD_FLAG_DATE_ERR_OKAY`: a root out of its dates is still loaded (and only a chain
/// that needs it fails, at the handshake).
pub const LOAD_FLAG_DATE_ERR_OKAY: u32 = 0x2;
/// `WOLFSSL_SNI_HOST_NAME`.
pub const SNI_HOST_NAME: c_uchar = 0;

/// `VerifyCallback`: set only for a context trusting a bundle (`crate::bundle`).
pub type VerifyCallback = unsafe extern "C" fn(c_int, *mut c_void) -> c_int;

/// `CallbackIORecv` and `CallbackIOSend`.
pub type IoCallback = unsafe extern "C" fn(*mut Wolf, *mut c_char, c_int, *mut c_void) -> c_int;

unsafe extern "C" {
    pub fn wolfSSL_Init() -> c_int;
    pub fn wolfSSLv23_client_method() -> *mut WolfMethod;
    pub fn wolfSSL_CTX_new(method: *mut WolfMethod) -> *mut WolfCtx;
    pub fn wolfSSL_CTX_free(ctx: *mut WolfCtx);
    pub fn wolfSSL_CTX_load_verify_buffer_ex(
        ctx: *mut WolfCtx,
        input: *const c_uchar,
        size: c_long,
        format: c_int,
        user_chain: c_int,
        flags: u32,
    ) -> c_int;
    pub fn wolfSSL_CTX_set_verify(ctx: *mut WolfCtx, mode: c_int, callback: Option<VerifyCallback>);
    pub fn wolfSSL_new(ctx: *mut WolfCtx) -> *mut Wolf;
    pub fn wolfSSL_CTX_SetCertCbCtx(ctx: *mut WolfCtx, user: *mut c_void);
    pub fn wolfSSL_free(ssl: *mut Wolf);
    pub fn wolfSSL_UseSNI(ssl: *mut Wolf, kind: c_uchar, data: *const c_void, size: u16) -> c_int;
    pub fn wolfSSL_check_domain_name(ssl: *mut Wolf, name: *const c_char) -> c_int;
    pub fn wolfSSL_SSLSetIORecv(ssl: *mut Wolf, callback: IoCallback);
    pub fn wolfSSL_SSLSetIOSend(ssl: *mut Wolf, callback: IoCallback);
    pub fn wolfSSL_SetIOReadCtx(ssl: *mut Wolf, ctx: *mut c_void);
    pub fn wolfSSL_SetIOWriteCtx(ssl: *mut Wolf, ctx: *mut c_void);
    pub fn wolfSSL_connect(ssl: *mut Wolf) -> c_int;
    pub fn wolfSSL_read(ssl: *mut Wolf, data: *mut c_void, size: c_int) -> c_int;
    pub fn wolfSSL_write(ssl: *mut Wolf, data: *const c_void, size: c_int) -> c_int;
    pub fn wolfSSL_get_error(ssl: *mut Wolf, ret: c_int) -> c_int;
}

#[cfg(feature = "test-server")]
unsafe extern "C" {
    pub fn wolfSSLv23_server_method() -> *mut WolfMethod;
    pub fn wolfTLSv1_2_server_method() -> *mut WolfMethod;
    /// The server's certificate and the chain after it, PEM.
    pub fn wolfSSL_CTX_use_certificate_chain_buffer(
        ctx: *mut WolfCtx,
        input: *const c_uchar,
        size: c_long,
    ) -> c_int;
    pub fn wolfSSL_CTX_use_PrivateKey_buffer(
        ctx: *mut WolfCtx,
        input: *const c_uchar,
        size: c_long,
        format: c_int,
    ) -> c_int;
    pub fn wolfSSL_accept(ssl: *mut Wolf) -> c_int;
}
