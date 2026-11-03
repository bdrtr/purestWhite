//! A request written as the C++ firmware's HTTP client writes it (`SecureHttpClient`): the request
//! line, `Host` (with the port when it is not the scheme's own, `hostHeaderFor`), its
//! `User-Agent`, then the caller's headers, `Content-Length` with a body that is not empty, and the
//! body; HTTP/1.1,
//! but with `Connection: close`, so the server ends the answer by closing the connection. Nothing
//! is allocated: the request goes to a sink, piece by piece, so a caller can count it first and
//! then write it into memory it holds (a secret's).

use crate::url::Server;

/// The C++ client's `User-Agent`: a server (or a filter in front of one) sees the same client.
pub const USER_AGENT: &[u8] = b"FreeInk-ESP32";

/// A header: its name and value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
}

/// A request to write.
#[derive(Clone, Copy, Debug)]
pub struct Request<'a> {
    /// `GET`, `PUT`, `POST` and the like.
    pub method: &'a str,
    pub server: &'a Server<'a>,
    /// The path after the server's own.
    pub path: &'a [u8],
    pub headers: &'a [Header<'a>],
    pub body: Option<&'a [u8]>,
}

/// The request holds a byte that would break it: a control byte in a header value, a space,
/// control byte or `:` in a header name, a space or control byte in the host or the path (or no
/// host), a method that is not capital letters. The C++ client sends such requests as they are; this writes none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unsafe;

/// Whether `value` may be a header's value: no control byte but tab.
fn clean_value(value: &[u8]) -> bool {
    !value.iter().any(|&b| (b < b' ' && b != b'\t') || b == 0x7F)
}

/// Whether `name` may be a header's name: no space, control byte or `:`.
fn clean_name(name: &[u8]) -> bool {
    !name.is_empty() && name.iter().all(|&b| b > b' ' && b != b':' && b != 0x7F)
}

/// Whether `request` holds no byte that would break it.
fn clean(request: &Request<'_>) -> bool {
    let method =
        !request.method.is_empty() && request.method.bytes().all(|b| b.is_ascii_uppercase());
    let headers = request
        .headers
        .iter()
        .all(|h| clean_name(h.name) && clean_value(h.value));
    method
        && headers
        && !request.server.host.is_empty()
        && crate::word(request.server.host.as_bytes())
        && crate::word(request.server.path)
        && crate::word(request.path)
}

/// The request line, `Host`, `User-Agent` and `Connection`.
fn write_head(out: &mut impl FnMut(&[u8]), r: &Request<'_>) {
    let path: &[u8] = if r.server.path.is_empty() && r.path.is_empty() {
        b"/"
    } else {
        r.path
    };
    for piece in [
        r.method.as_bytes(),
        b" ",
        r.server.path,
        path,
        b" HTTP/1.1\r\nHost: ",
    ] {
        out(piece);
    }
    out(r.server.host.as_bytes());
    let port = r.server.port;
    if port != 0 && port != r.server.default_port() {
        out(b":");
        out(Digits::of(usize::from(port)).as_ref());
    }
    for piece in [
        &b"\r\nUser-Agent: "[..],
        USER_AGENT,
        b"\r\nConnection: close\r\n",
    ] {
        out(piece);
    }
}

/// Writes `request` to `out`, piece by piece.
///
/// # Errors
/// [`Unsafe`]: nothing is written then.
pub fn write(out: &mut impl FnMut(&[u8]), request: &Request<'_>) -> Result<(), Unsafe> {
    if !clean(request) {
        return Err(Unsafe);
    }
    write_head(out, request);
    for h in request.headers {
        for piece in [h.name, b": ", h.value, b"\r\n"] {
            out(piece);
        }
    }
    if let Some(body) = request.body.filter(|b| !b.is_empty()) {
        out(b"Content-Length: ");
        out(Digits::of(body.len()).as_ref());
        out(b"\r\n");
    }
    out(b"\r\n");
    if let Some(body) = request.body {
        out(body);
    }
    Ok(())
}

/// How many bytes `request` takes.
///
/// # Errors
/// [`Unsafe`], as [`write`].
pub fn len(request: &Request<'_>) -> Result<usize, Unsafe> {
    let mut size = 0;
    write(&mut |piece| size += piece.len(), request)?;
    Ok(size)
}

/// A number's decimal digits, on the stack.
struct Digits {
    text: [u8; 20],
    start: usize,
}

impl Digits {
    fn of(mut n: usize) -> Digits {
        let mut d = Digits {
            text: [0; 20],
            start: 20,
        };
        loop {
            d.start -= 1;
            if let Some(slot) = d.text.get_mut(d.start) {
                *slot = b'0' + u8::try_from(n % 10).unwrap_or(0);
            }
            n /= 10;
            if n == 0 || d.start == 0 {
                return d;
            }
        }
    }
}

impl AsRef<[u8]> for Digits {
    fn as_ref(&self) -> &[u8] {
        self.text.get(self.start..).unwrap_or_default()
    }
}
