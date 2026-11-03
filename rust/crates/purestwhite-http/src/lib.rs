//! A small HTTP/1.0 client's two halves, over whatever carries the bytes (TLS, in the firmware):
//! the GET request (`write_get`), and the response read as it arrives (`Response`): its status
//! line and headers kept until they end, then the body handed on piece by piece, as long as its
//! `Content-Length` says when one is given. HTTP/1.0 keeps it simple: no chunked body, and the
//! server ends the body by closing the connection. Nothing here allocates or panics.
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;
#[cfg(test)]
mod tests;

use core::fmt;

/// The most a response's status line and headers may take.
pub const HEAD_MOST: usize = 4096;

/// Writes a GET request for `path` on `host` to `out`.
///
/// # Errors
/// `out` fails, or `host` or `path` is empty or holds a byte that would break the request (a space
/// or a control byte), or `path` does not start with `/`.
pub fn write_get(out: &mut impl fmt::Write, host: &str, path: &str) -> fmt::Result {
    let clean = |text: &str| !text.is_empty() && !text.bytes().any(|b| b <= b' ' || b == 0x7F);
    if !clean(host) || !clean(path) || !path.starts_with('/') {
        return Err(fmt::Error);
    }
    write!(
        out,
        "GET {path} HTTP/1.0\r\nHost: {host}\r\nUser-Agent: purestWhite\r\nAccept: */*\r\n\r\n"
    )
}

/// Why a response cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseError {
    /// The status line and headers take more than `HEAD_MOST` (or end their lines with LF alone,
    /// which this does not read).
    HeadTooLong,
    /// The status line is not `HTTP/1.x NNN ...`.
    StatusLine,
}

/// A response, read as it arrives.
pub struct Response {
    head: [u8; HEAD_MOST],
    head_len: usize,
    status: Option<u16>,
    /// `Content-Length`, when the head gives one.
    length: Option<usize>,
    body: usize,
}

impl Response {
    #[must_use]
    pub const fn new() -> Self {
        Response {
            head: [0; HEAD_MOST],
            head_len: 0,
            status: None,
            length: None,
            body: 0,
        }
    }

    /// The body's length the head gave (`Content-Length`), once it is read.
    #[must_use]
    pub fn content_length(&self) -> Option<usize> {
        self.length
    }

    /// Whether the whole body came: as long as `Content-Length` says. Without one, never: the
    /// server's close ends the body then.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.length.is_some_and(|length| self.body >= length)
    }

    /// The status code, once the status line and headers are read.
    #[must_use]
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    /// How many bytes of the body were handed on.
    #[must_use]
    pub fn body_len(&self) -> usize {
        self.body
    }

    /// The response's next bytes: the head's kept, the body's handed to `body` as they come.
    ///
    /// # Errors
    /// The head is too long, or its status line is not HTTP's.
    pub fn feed(&mut self, bytes: &[u8], mut body: impl FnMut(&[u8])) -> Result<(), ResponseError> {
        if self.status.is_some() {
            self.body += bytes.len();
            body(bytes);
            return Ok(());
        }
        let room = self.head.get_mut(self.head_len..).unwrap_or_default();
        let taken = bytes.len().min(room.len());
        if let (Some(to), Some(from)) = (room.get_mut(..taken), bytes.get(..taken)) {
            to.copy_from_slice(from);
        }
        self.head_len += taken;
        let Some(end) = find(
            self.head.get(..self.head_len).unwrap_or_default(),
            b"\r\n\r\n",
        ) else {
            return if taken < bytes.len() || self.head_len == HEAD_MOST {
                Err(ResponseError::HeadTooLong)
            } else {
                Ok(())
            };
        };
        let head = self.head.get(..end).unwrap_or_default();
        self.status = Some(status(head)?);
        self.length = content_length(head);
        // What came after the head, of what was kept and of what was not.
        let kept_body = self.head.get(end + 4..self.head_len).unwrap_or_default();
        let rest = bytes.get(taken..).unwrap_or_default();
        self.body = kept_body.len() + rest.len();
        body(kept_body);
        body(rest);
        Ok(())
    }
}

impl Default for Response {
    fn default() -> Self {
        Self::new()
    }
}

/// The `Content-Length` header's value in `head` (names are case-insensitive), if it is a number.
fn content_length(head: &[u8]) -> Option<usize> {
    const NAME: &[u8] = b"content-length:";
    head.split(|&b| b == b'\n').skip(1).find_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let value = line.get(NAME.len()..)?;
        if !line.get(..NAME.len())?.eq_ignore_ascii_case(NAME) {
            return None;
        }
        let value = value.trim_ascii();
        if value.is_empty() || !value.iter().all(u8::is_ascii_digit) {
            return None;
        }
        core::str::from_utf8(value).ok()?.parse().ok()
    })
}

/// Where `needle` first starts in `hay`.
fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// The status code of `head`'s first line, `HTTP/1.x NNN reason`.
fn status(head: &[u8]) -> Result<u16, ResponseError> {
    let line = head.split(|&b| b == b'\r').next().unwrap_or_default();
    let mut parts = line.split(|&b| b == b' ');
    let version = parts.next().unwrap_or_default();
    let code = parts.next().unwrap_or_default();
    if !version.starts_with(b"HTTP/1.") || code.len() != 3 || !code.iter().all(u8::is_ascii_digit) {
        return Err(ResponseError::StatusLine);
    }
    core::str::from_utf8(code)
        .ok()
        .and_then(|code| code.parse().ok())
        .ok_or(ResponseError::StatusLine)
}
