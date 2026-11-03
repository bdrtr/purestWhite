//! A `KOSync` request (`KOReaderSyncClient`): the extra headers the account names, then the sign-in
//! (`x-auth-user`, and `x-auth-key`, the MD5 of the password in hex) and an HTTP Basic one (for
//! Calibre-Web-Automated), as `applyCustomHeaders` and `applyAuthHeaders` add them, written by
//! `purestwhite_http::request` as the C++ client writes a request; the account check
//! (`authenticate`: a GET of `/users/auth`), and what its status means.

use crate::Account;
use alloc::vec::Vec;
use bembeyaz_store::password::Secret;
use purestwhite_http::request::{self, Header, Request};
use purestwhite_http::url::Server;
use purestwhite_obfuscation::base64;

/// The path the account check asks for, after the server's.
pub const AUTH_PATH: &[u8] = b"/users/auth";
/// What a `KOSync` request accepts.
pub const ACCEPT: &[u8] = b"application/vnd.koreader.v1+json";

/// Why a request could not be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestError {
    /// No username or no password (`hasCredentials`): the C++ client sends nothing either.
    NoCredentials,
    /// A byte that would break the request (`purestwhite_http::request::Unsafe`: a line break in a
    /// header's value, a username with a control byte, a space in the path...). The C++ client
    /// would send it as it is.
    Unsafe,
    /// Memory ran out.
    Memory,
}

/// What the account check's status means (`authenticate`'s result).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Answer {
    /// Any 2xx: signed in.
    Ok,
    /// 401: the username or password is wrong.
    AuthFailed,
    /// Any other status (a redirect too: the C++ client does not follow one).
    ServerError,
}

/// What `status` means.
#[must_use]
pub fn answer(status: u16) -> Answer {
    match status {
        200..=299 => Answer::Ok,
        401 => Answer::AuthFailed,
        _ => Answer::ServerError,
    }
}

/// `text` up to its first NUL: what C++ hands on of a text it passes as a `const char*`
/// (`MD5Builder::add(password.c_str())`, `base64::encode(credentials.c_str())`).
fn c_str(text: &[u8]) -> &[u8] {
    text.split(|&b| b == 0).next().unwrap_or_default()
}

/// The sign-in's secret values, held while a request is written.
struct SignIn {
    /// The password's MD5 in lowercase hex (`MD5Builder::toString`).
    key: Secret,
    /// `Basic ` and `username:password` in base64.
    basic: Secret,
}

impl SignIn {
    fn of(account: &Account, md5: impl FnOnce(&[u8]) -> [u8; 16]) -> Result<SignIn, RequestError> {
        let settings = account.settings();
        let mut digest = md5(c_str(&settings.password));
        let key = hex(&digest);
        digest.fill(0);
        core::hint::black_box(&digest);
        let key = key?;
        Ok(SignIn {
            key,
            basic: basic(&settings.username, &settings.password)?,
        })
    }
}

/// `bytes` in lowercase hex, as a secret.
fn hex(bytes: &[u8; 16]) -> Result<Secret, RequestError> {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let digit = |n: u8| DIGITS.get(usize::from(n)).copied().unwrap_or(b'0');
    let mut out = Vec::new();
    out.try_reserve_exact(32)
        .map_err(|_| RequestError::Memory)?;
    for byte in bytes {
        out.push(digit(byte >> 4));
        out.push(digit(byte & 15));
    }
    Ok(Secret::new(out))
}

/// `Basic ` and `username:password` (up to a NUL, as C++ hands it to `base64::encode`) in base64.
fn basic(username: &[u8], password: &[u8]) -> Result<Secret, RequestError> {
    let mut credentials = Vec::new();
    credentials
        .try_reserve_exact(username.len() + 1 + password.len())
        .map_err(|_| RequestError::Memory)?;
    credentials.extend_from_slice(username);
    credentials.push(b':');
    credentials.extend_from_slice(password);
    let credentials = Secret::new(credentials);
    let encoded = base64::encode(c_str(&credentials)).map_err(|_| RequestError::Memory)?;
    let mut value = Vec::new();
    value
        .try_reserve_exact(6 + encoded.len())
        .map_err(|_| RequestError::Memory)?;
    value.extend_from_slice(b"Basic ");
    value.extend_from_slice(&encoded);
    Ok(Secret::new(value))
}

/// A `KOSync` request for `server` and `account`: `method` on `path` (after the server's own), the
/// account's extra headers (those with a name), the sign-in, then `extra` and `body`. `md5` gives
/// the MD5 of what it is handed (the password, up to a NUL, as C++ hashes it). The request holds
/// the password, so it is a [`Secret`].
///
/// # Errors
/// No account, a byte that would break the request, or memory ran out.
pub fn request(
    (server, account): (&Server<'_>, &Account),
    (method, path): (&str, &[u8]),
    extra: &[Header<'_>],
    body: Option<&[u8]>,
    md5: impl FnOnce(&[u8]) -> [u8; 16],
) -> Result<Secret, RequestError> {
    if !account.has_credentials() {
        return Err(RequestError::NoCredentials);
    }
    let settings = account.settings();
    let sign_in = SignIn::of(account, md5)?;
    let mut headers: Vec<Header<'_>> = Vec::new();
    headers
        .try_reserve_exact(settings.headers.len() + 4 + extra.len())
        .map_err(|_| RequestError::Memory)?;
    for h in settings.headers.iter().filter(|h| !h.name.is_empty()) {
        headers.push(Header {
            name: &h.name,
            value: &h.value,
        });
    }
    let auth: [(&[u8], &[u8]); 4] = [
        (b"Accept", ACCEPT),
        (b"x-auth-user", &settings.username),
        (b"x-auth-key", &sign_in.key),
        (b"Authorization", &sign_in.basic),
    ];
    headers.extend(auth.map(|(name, value)| Header { name, value }));
    headers.extend_from_slice(extra);
    let request = Request {
        method,
        server,
        path,
        headers: &headers,
        body,
    };
    written(&request)
}

/// `request` written into one secret buffer, sized first so nothing is moved.
fn written(request: &Request<'_>) -> Result<Secret, RequestError> {
    let size = request::len(request).map_err(|_| RequestError::Unsafe)?;
    let mut out = Vec::new();
    out.try_reserve_exact(size)
        .map_err(|_| RequestError::Memory)?;
    request::write(&mut |piece| out.extend_from_slice(piece), request)
        .map_err(|_| RequestError::Unsafe)?;
    Ok(Secret::new(out))
}

/// The account check (`authenticate`): a GET of [`AUTH_PATH`], signed in.
///
/// # Errors
/// As [`request`].
pub fn auth_request(
    server: &Server<'_>,
    account: &Account,
    md5: impl FnOnce(&[u8]) -> [u8; 16],
) -> Result<Secret, RequestError> {
    request((server, account), ("GET", AUTH_PATH), &[], None, md5)
}
