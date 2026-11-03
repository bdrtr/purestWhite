//! A request over HTTPS, for gate 2: its host looked up (DNS), a TCP connection (embassy-net), TLS
//! over it (`purestwhite-tls`, the server's chain checked against the C++ firmware's roots,
//! ESP-IDF's certificate bundle, as the C++ firmware checks it, and its name against the host), the
//! request sent, and the answer read as it arrives (`purestwhite-http`): a file fetched with a GET
//! and hashed (`get`), or any request with its body handed on, or only its status (`exchange`).
//! TLS runs over memory buffers; `Wire` moves the bytes between them and the socket.

use alloc::string::String;
use core::fmt::{self, Display, Formatter};
use embassy_futures::select::select;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_net::{IpAddress, Stack};
use embassy_time::{Duration, Instant, Timer};
use purestwhite_http::{Response, ResponseError, write_get};
use purestwhite_ota::digest::Sha256;
use purestwhite_ota::image::DIGEST;
use purestwhite_tls::{Connection, Context, Error as TlsError};

/// How long the connection may stay silent before it is given up.
const SILENT_MOST: Duration = Duration::from_secs(10);
/// The TCP socket's buffers: a TLS record is up to 16 KB, read in pieces.
const RX: usize = 4096;
const TX: usize = 2048;
/// How long the orderly close may take.
const CLOSE_MOST: Duration = Duration::from_secs(1);
/// How much of the socket is read, and of the plain text, at a time.
const PIECE: usize = 1024;

/// What a fetch found, and how long each part took, in ms.
pub struct Fetched {
    pub status: u16,
    pub body: usize,
    pub digest: [u8; DIGEST],
    pub dns_ms: u64,
    pub tcp_ms: u64,
    pub tls_ms: u64,
    pub body_ms: u64,
}

/// `https: 200, 10246 B, sha c4e5578c, ms 40/80/900/120`: the status, the body's size and the
/// start of its SHA-256, and the time DNS, TCP, TLS and the body took.
impl Display for Fetched {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        write!(out, "https: {}, {} B, sha ", self.status, self.body)?;
        for byte in self.digest.iter().take(4) {
            write!(out, "{byte:02x}")?;
        }
        write!(
            out,
            ", ms {}/{}/{}/{}",
            self.dns_ms, self.tcp_ms, self.tls_ms, self.body_ms
        )
    }
}

/// Why a fetch failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The host's name was not found.
    Dns,
    /// The TCP connection was refused or timed out.
    Tcp,
    /// The connection broke or went silent.
    Wire,
    /// TLS failed: wolfSSL's code (a certificate that does not verify, say).
    Tls(TlsError),
    /// The request could not be written (a host or path HTTP refuses).
    Request,
    Response(ResponseError),
    /// The connection ended before the response's head.
    NoResponse,
    /// The connection ended before the body's `Content-Length`.
    Cut,
}

/// `https: tls (Wolf(-188))`, and the like.
impl Display for FetchError {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        match self {
            FetchError::Tls(e) => write!(out, "https: tls ({e:?})"),
            FetchError::Response(e) => write!(out, "https: response ({e:?})"),
            other => write!(out, "https: {other:?}"),
        }
    }
}

impl From<TlsError> for FetchError {
    fn from(error: TlsError) -> Self {
        FetchError::Tls(error)
    }
}

/// The socket and the TLS session over it.
struct Wire<'w, 's, 'c> {
    socket: &'w mut TcpSocket<'s>,
    tls: &'w mut Connection<'c>,
    /// The socket closed its side.
    closed: bool,
}

impl Wire<'_, '_, '_> {
    /// Sends what TLS wrote.
    async fn flush(&mut self) -> Result<(), FetchError> {
        while !self.tls.to_send().is_empty() {
            let n = self
                .socket
                .write(self.tls.to_send())
                .await
                .map_err(|_| FetchError::Wire)?;
            self.tls.sent(n);
        }
        self.socket.flush().await.map_err(|_| FetchError::Wire)
    }

    /// The server's next bytes, for TLS; at its end, TLS hears it closed.
    async fn fill(&mut self) -> Result<(), FetchError> {
        let mut piece = [0u8; PIECE];
        match self.socket.read(&mut piece).await {
            Ok(0) => {
                self.closed = true;
                self.tls.closed();
                Ok(())
            }
            Ok(n) => {
                self.tls.received(piece.get(..n).unwrap_or_default());
                Ok(())
            }
            Err(_) => Err(FetchError::Wire),
        }
    }

    async fn handshake(&mut self) -> Result<(), FetchError> {
        loop {
            let done = self.tls.handshake()?;
            self.flush().await?;
            if done {
                return Ok(());
            }
            if self.closed {
                return Err(FetchError::Wire);
            }
            self.fill().await?;
        }
    }

    async fn request(&mut self, request: &[u8]) -> Result<(), FetchError> {
        let mut at = 0;
        while let Some(rest) = request.get(at..).filter(|r| !r.is_empty()) {
            // A piece at a time, so TLS's outgoing buffer holds a record at most.
            let taken = self.tls.write(rest.get(..PIECE).unwrap_or(rest))?;
            at += taken;
            self.flush().await?;
            // TLS wants the server's bytes first (it never should here): wait for them.
            if taken == 0 {
                if self.closed {
                    return Err(FetchError::Wire);
                }
                self.fill().await?;
            }
        }
        Ok(())
    }

    /// The response, its body handed to `body`: until its `Content-Length` when it gives one, else
    /// until the server ends it; or, for [`Until::Status`], only until its status line is in.
    async fn response(
        &mut self,
        until: Until,
        body: &mut impl FnMut(&[u8]),
    ) -> Result<Answer, FetchError> {
        let mut response = Response::new();
        let mut piece = [0u8; PIECE];
        loop {
            match self.tls.read(&mut piece) {
                Ok(Some(0)) => return ended(&response),
                Ok(Some(n)) => {
                    let fed = response.feed(piece.get(..n).unwrap_or_default(), &mut *body);
                    if let (Until::Status, Some(status)) = (until, response.early_status()) {
                        return Ok(Answer { status, body: 0 });
                    }
                    fed.map_err(FetchError::Response)?;
                    if response.is_complete() {
                        return ended(&response);
                    }
                }
                Ok(None) if self.closed => return ended(&response),
                Ok(None) => {
                    self.flush().await?;
                    self.fill().await?;
                }
                // A server may close the connection without TLS's goodbye: the body ends there.
                Err(TlsError::Wolf(PEER_CLOSED | SOCKET_ERROR)) if self.closed => {
                    return ended(&response);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// wolfSSL's codes for a connection closed under it: `SOCKET_PEER_CLOSED_E`, `SOCKET_ERROR_E`.
const PEER_CLOSED: i32 = -397;
const SOCKET_ERROR: i32 = -308;

/// What a response came to: its status, and how much body was handed on.
#[derive(Clone, Copy, Debug)]
struct Answer {
    status: u16,
    body: usize,
}

/// The response the server ended: whole, unless its head never came or its `Content-Length` says
/// more was to come.
fn ended(response: &Response) -> Result<Answer, FetchError> {
    let status = response.status().ok_or(FetchError::NoResponse)?;
    if response.content_length().is_some() && !response.is_complete() {
        return Err(FetchError::Cut);
    }
    Ok(Answer {
        status,
        body: response.body_len(),
    })
}

/// How much of the answer an exchange reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Until {
    /// Only its status line: the rest (a long head, a body that never ends) is not waited for.
    Status,
    /// The whole answer, its body handed on.
    End,
}

/// What an exchange found, and how long each part took, in ms.
#[derive(Clone, Copy, Debug)]
pub struct Exchanged {
    pub status: u16,
    /// How many bytes of the body were handed on.
    pub body: usize,
    pub dns_ms: u64,
    pub tcp_ms: u64,
    pub tls_ms: u64,
    /// From the request sent to the answer read.
    pub answer_ms: u64,
}

/// `host`'s first IPv4 address.
async fn resolve(stack: Stack<'_>, host: &str) -> Result<IpAddress, FetchError> {
    let found = stack.dns_query(host, DnsQueryType::A).await;
    found
        .ok()
        .and_then(|ips| ips.first().copied())
        .ok_or(FetchError::Dns)
}

/// Fetches `path` from `host` over HTTPS, trusting the C++ firmware's roots
/// (`purestwhite_x509::ESP_IDF_BUNDLE`), its body hashed into `sha`.
///
/// # Errors
/// A step failed: DNS, TCP, TLS (a certificate that does not verify, say), or HTTP.
pub async fn get(
    stack: Stack<'_>,
    host: &str,
    path: &str,
    mut sha: impl Sha256,
) -> Result<Fetched, FetchError> {
    let mut request = String::new();
    write_get(&mut request, host, path).map_err(|_| FetchError::Request)?;
    let answer = exchange(stack, (host, 443), request.as_bytes(), Until::End, |b| {
        sha.update(b);
    })
    .await?;
    Ok(Fetched {
        status: answer.status,
        body: answer.body,
        digest: sha.finish(),
        dns_ms: answer.dns_ms,
        tcp_ms: answer.tcp_ms,
        tls_ms: answer.tls_ms,
        body_ms: answer.answer_ms,
    })
}

/// Sends `request` (a whole request, the connection closed after it) to `host` on `port` over TLS,
/// as [`get`] does, and reads the answer `until` its status or its end, its body handed to `body`.
///
/// # Errors
/// As [`get`].
pub async fn exchange(
    stack: Stack<'_>,
    (host, port): (&str, u16),
    request: &[u8],
    until: Until,
    mut body: impl FnMut(&[u8]),
) -> Result<Exchanged, FetchError> {
    let began = Instant::now();
    let ip = resolve(stack, host).await?;
    let dns_ms = began.elapsed().as_millis();
    let (mut rx, mut tx) = ([0u8; RX], [0u8; TX]);
    let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
    socket.set_timeout(Some(SILENT_MOST));
    let at = Instant::now();
    socket
        .connect((ip, port))
        .await
        .map_err(|_| FetchError::Tcp)?;
    let tcp_ms = at.elapsed().as_millis();
    let context = Context::bundle(purestwhite_x509::ESP_IDF_BUNDLE)?;
    let mut tls = Connection::new(&context, host)?;
    let mut wire = Wire {
        socket: &mut socket,
        tls: &mut tls,
        closed: false,
    };
    let at = Instant::now();
    wire.handshake().await?;
    let tls_ms = at.elapsed().as_millis();
    let at = Instant::now();
    wire.request(request).await?;
    let answer = wire.response(until, &mut body).await?;
    let answer_ms = at.elapsed().as_millis();
    // Closed in an orderly way (a dropped socket sends nothing), for a moment at most.
    socket.close();
    let _ = select(socket.flush(), Timer::after(CLOSE_MOST)).await;
    Ok(Exchanged {
        status: answer.status,
        body: answer.body,
        dns_ms,
        tcp_ms,
        tls_ms,
        answer_ms,
    })
}
