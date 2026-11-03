//! A URL split as the C++ firmware's HTTP client splits it (`lib/SecureNet`'s
//! `SecureHttpClient::parseUrl`): the scheme before `://` (`http` or `https`, any case), then the
//! host and an optional `:port` up to the first `/`, then the path. Userinfo and IPv6 brackets are
//! not understood, as there (`https://u@h` names the host `u@h`; the last `:` starts the port). The
//! host must also be text a name lookup takes (no space or control byte, UTF-8), which C++ does not
//! check.

/// Where a server is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Server<'a> {
    /// Whether it is reached over TLS (`https`).
    pub tls: bool,
    /// The name or address the URL gives.
    pub host: &'a str,
    /// The port: the scheme's own when none is given, 0 when the one given is not a number.
    pub port: u16,
    /// The path the API's paths go after: empty, or from a `/`.
    pub path: &'a [u8],
}

impl Server<'_> {
    /// The scheme's own port.
    #[must_use]
    pub fn default_port(&self) -> u16 {
        if self.tls { 443 } else { 80 }
    }
}

/// Why a URL is not a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UrlError {
    /// No `://`, or a scheme other than `http` and `https`.
    Scheme,
    /// No host, or one that is not text a name lookup takes.
    Host,
}

/// The server `url` names. The port is read as C's `atoi` reads it, then cut to 16 bits, as the
/// C++ client reads it.
///
/// # Errors
/// The scheme is not one the C++ client takes, or there is no host a name lookup takes.
pub fn split(url: &[u8]) -> Result<Server<'_>, UrlError> {
    let at = url
        .windows(3)
        .position(|w| w == b"://")
        .ok_or(UrlError::Scheme)?;
    let (scheme, rest) = (
        url.get(..at).unwrap_or_default(),
        url.get(at + 3..).unwrap_or_default(),
    );
    let tls = if scheme.eq_ignore_ascii_case(b"https") {
        true
    } else if scheme.eq_ignore_ascii_case(b"http") {
        false
    } else {
        return Err(UrlError::Scheme);
    };
    let slash = rest.iter().position(|&b| b == b'/').unwrap_or(rest.len());
    let (host_port, path) = rest.split_at(slash);
    let (host, port) = match host_port.iter().rposition(|&b| b == b':') {
        Some(colon) => (
            host_port.get(..colon).unwrap_or_default(),
            atoi(host_port.get(colon + 1..).unwrap_or_default()),
        ),
        None => (host_port, if tls { 443 } else { 80 }),
    };
    let clean = !host.is_empty() && crate::word(host);
    let host = core::str::from_utf8(host)
        .ok()
        .filter(|_| clean)
        .ok_or(UrlError::Host)?;
    Ok(Server {
        tls,
        host,
        port,
        path,
    })
}

/// C's `atoi` (newlib: `(int)strtol`, saturating), cut to `uint16_t`: spaces skipped, a sign, then
/// digits; 0 when there are none.
fn atoi(text: &[u8]) -> u16 {
    let text = text.trim_ascii_start();
    let (negative, digits) = match text.split_first() {
        Some((b'-', rest)) => (true, rest),
        Some((b'+', rest)) => (false, rest),
        _ => (false, text),
    };
    let mut value: i64 = 0;
    for &b in digits.iter().take_while(|b| b.is_ascii_digit()) {
        value = value.saturating_mul(10).saturating_add(i64::from(b - b'0'));
    }
    let value = if negative { -value } else { value };
    let int = i32::try_from(value).unwrap_or(if negative { i32::MIN } else { i32::MAX });
    let [low, high, ..] = int.to_le_bytes();
    u16::from_le_bytes([low, high])
}
