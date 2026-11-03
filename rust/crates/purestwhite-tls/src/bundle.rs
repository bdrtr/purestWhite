//! The bundle check, as the C++ firmware's `SecureClient` makes it with ESP-IDF's
//! `esp_crt_verify_callback`: no root is loaded into wolfSSL, so it finds no signer for the
//! topmost certificate the server sent. That certificate's issuer is then looked up among the
//! bundle's roots, its signature checked with the root's key and its dates against the clock
//! (`set_time`): wolfSSL does not check the dates of a certificate it found no signer for. Once
//! it is accepted, wolfSSL adds it as a CA to the connection's own context and checks the rest of
//! the chain and the host name itself.
//!
//! Signatures checked: RSA (PKCS #1 v1.5) and ECDSA, over SHA-1, SHA-256, SHA-384 or SHA-512.
//! The bundle's roots sign themselves with these only (`bundle_tests`); an intermediate a root
//! signed with RSASSA-PSS or SHA-224 would be refused, where ESP-IDF's mbedTLS would check it.

use crate::hooks;
use core::cell::Cell;
use core::ffi::{c_int, c_uchar, c_uint, c_void};
use purestwhite_x509::{Bundle, Certificate, Hash, KeyKind, Time, key_kind};

/// wolfSSL's `ASN_NO_SIGNER_E`: a chain no trusted root signed (checked by c/shim.c).
pub(crate) const NO_SIGNER: i32 = -188;
/// wolfCrypt's `MEMORY_E` and `MP_MEM` (checked by c/shim.c): the check ran out of memory.
const OUT_OF_MEMORY: [c_int; 2] = [-125, -2];
/// The hash and key codes c/shim.c's `pw_tls_signature_check` takes (its `PW_*` enums).
const SHA1: c_int = 0;
const SHA256: c_int = 1;
const SHA384: c_int = 2;
const SHA512: c_int = 3;
const RSA: c_int = 0;
const EC: c_int = 1;

/// One connection's bundle check, reached by wolfSSL's verify callback through its wolfSSL
/// context's callback context.
pub(crate) struct Check {
    bundle: Bundle<'static>,
    /// A signature check ran out of memory: the handshake's failure is that, not a refusal.
    short_of_memory: Cell<bool>,
}

impl Check {
    pub(crate) fn new(bundle: Bundle<'static>) -> Check {
        Check {
            bundle,
            short_of_memory: Cell::new(false),
        }
    }

    pub(crate) fn short_of_memory(&self) -> bool {
        self.short_of_memory.get()
    }
}

/// What the check says of a certificate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Accepted,
    Refused,
    /// The signature could not be checked for want of memory.
    ShortOfMemory,
}

unsafe extern "C" {
    /// c/shim.c: wolfSSL's verify callback.
    pub(crate) fn pw_tls_verify(preverified: c_int, store: *mut c_void) -> c_int;
    fn pw_tls_signature_check(
        spki: *const c_uchar,
        spki_len: c_uint,
        kind: c_int,
        hash: c_int,
        data: *const c_uchar,
        data_len: c_uint,
        sig: *const c_uchar,
        sig_len: c_uint,
    ) -> c_int;
}

/// Called by c/shim.c's `pw_tls_verify` with a certificate wolfSSL found no signer for: 1 when
/// a root of the bundle signed it and it is within its dates, else 0.
///
/// # Safety
/// `check` is the `Check` a live connection's own wolfSSL context points at; `der` holds `len`
/// readable bytes for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pw_tls_bundle_accepts(
    check: *const c_void,
    der: *const c_uchar,
    len: c_uint,
) -> c_int {
    if check.is_null() || der.is_null() {
        return 0;
    }
    let Ok(len) = usize::try_from(len) else {
        return 0;
    };
    // SAFETY: as the contract says.
    let (check, der) = unsafe {
        (
            &*check.cast::<Check>(),
            core::slice::from_raw_parts(der, len),
        )
    };
    match verdict(&check.bundle, der, Time::from_unix(u64::from(hooks::now()))) {
        Verdict::Accepted => 1,
        Verdict::Refused => 0,
        Verdict::ShortOfMemory => {
            check.short_of_memory.set(true);
            0
        }
    }
}

/// Whether a root of `bundle` signed `der` and `now` is within its dates.
#[cfg(test)]
pub(crate) fn accepts(bundle: &Bundle<'_>, der: &[u8], now: Time) -> bool {
    verdict(bundle, der, now) == Verdict::Accepted
}

/// What the check says of `der` at `now`.
pub(crate) fn verdict(bundle: &Bundle<'_>, der: &[u8], now: Time) -> Verdict {
    let Some(cert) = Certificate::read(der) else {
        return Verdict::Refused;
    };
    if !cert.is_current(now) {
        return Verdict::Refused;
    }
    let Some(root) = bundle.find(cert.issuer) else {
        return Verdict::Refused;
    };
    let Some(algorithm) = cert.signature_algorithm() else {
        return Verdict::Refused;
    };
    // The root's key must be of the signature's kind (mbedTLS's `mbedtls_pk_can_do`).
    if key_kind(root.key) != Some(algorithm.key) {
        return Verdict::Refused;
    }
    signed_by(
        root.key,
        algorithm.key,
        algorithm.hash,
        cert.signed,
        cert.signature,
    )
}

/// Whether `signature` over `data` checks with the key `spki`.
fn signed_by(spki: &[u8], kind: KeyKind, hash: Hash, data: &[u8], signature: &[u8]) -> Verdict {
    let hash = match hash {
        Hash::Sha1 => SHA1,
        Hash::Sha256 => SHA256,
        Hash::Sha384 => SHA384,
        Hash::Sha512 => SHA512,
        Hash::Sha224 => return Verdict::Refused,
    };
    let kind = match kind {
        KeyKind::Rsa => RSA,
        KeyKind::Ec => EC,
    };
    let size = |bytes: &[u8]| c_uint::try_from(bytes.len()).ok();
    let (Some(spki_len), Some(data_len), Some(sig_len)) = (size(spki), size(data), size(signature))
    else {
        return Verdict::Refused;
    };
    // SAFETY: each pointer holds the length given with it, read during the call only.
    let ret = unsafe {
        pw_tls_signature_check(
            spki.as_ptr(),
            spki_len,
            kind,
            hash,
            data.as_ptr(),
            data_len,
            signature.as_ptr(),
            sig_len,
        )
    };
    match ret {
        0 => Verdict::Accepted,
        ret if OUT_OF_MEMORY.contains(&ret) => Verdict::ShortOfMemory,
        _ => Verdict::Refused,
    }
}
