//! A file fetched over HTTPS, for gate 2: its host looked up (DNS), a TCP connection on port 443
//! (embassy-net), TLS over it (`purestwhite-tls`, the server's chain and name checked against the
//! roots given), a GET (`purestwhite-http`, HTTP/1.0) and the body hashed as it arrives. TLS runs
//! over memory buffers; `Wire` moves the bytes between them and the socket.

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

    async fn request(&mut self, host: &str, path: &str) -> Result<(), FetchError> {
        let mut request = String::new();
        write_get(&mut request, host, path).map_err(|_| FetchError::Request)?;
        let mut at = 0;
        while let Some(rest) = request.as_bytes().get(at..).filter(|r| !r.is_empty()) {
            let taken = self.tls.write(rest)?;
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

    /// The response, its body into `sha`: until its `Content-Length` when it gives one, else
    /// until the server ends it.
    async fn response(&mut self, sha: &mut impl Sha256) -> Result<Response, FetchError> {
        let mut response = Response::new();
        let mut piece = [0u8; PIECE];
        loop {
            match self.tls.read(&mut piece) {
                Ok(Some(0)) => return ended(response),
                Ok(Some(n)) => {
                    response
                        .feed(piece.get(..n).unwrap_or_default(), |b| sha.update(b))
                        .map_err(FetchError::Response)?;
                    if response.is_complete() {
                        return Ok(response);
                    }
                }
                Ok(None) if self.closed => return ended(response),
                Ok(None) => {
                    self.flush().await?;
                    self.fill().await?;
                }
                // A server may close the connection without TLS's goodbye: the body ends there.
                Err(TlsError::Wolf(PEER_CLOSED | SOCKET_ERROR)) if self.closed => {
                    return ended(response);
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

/// wolfSSL's codes for a connection closed under it: `SOCKET_PEER_CLOSED_E`, `SOCKET_ERROR_E`.
const PEER_CLOSED: i32 = -397;
const SOCKET_ERROR: i32 = -308;

/// The response the server ended: whole, unless its head never came or its `Content-Length` says
/// more was to come.
fn ended(response: Response) -> Result<Response, FetchError> {
    if response.status().is_none() {
        return Err(FetchError::NoResponse);
    }
    if response.content_length().is_some() && !response.is_complete() {
        return Err(FetchError::Cut);
    }
    Ok(response)
}

/// `host`'s first IPv4 address.
async fn resolve(stack: Stack<'_>, host: &str) -> Result<IpAddress, FetchError> {
    let found = stack.dns_query(host, DnsQueryType::A).await;
    found
        .ok()
        .and_then(|ips| ips.first().copied())
        .ok_or(FetchError::Dns)
}

/// Fetches `path` from `host` over HTTPS, trusting `roots` (PEM), its body hashed into `sha`.
///
/// # Errors
/// A step failed: DNS, TCP, TLS (a certificate that does not verify, say), or HTTP.
pub async fn get(
    stack: Stack<'_>,
    host: &str,
    path: &str,
    roots: &[u8],
    mut sha: impl Sha256,
) -> Result<Fetched, FetchError> {
    let began = Instant::now();
    let ip = resolve(stack, host).await?;
    let dns_ms = began.elapsed().as_millis();
    let (mut rx, mut tx) = ([0u8; RX], [0u8; TX]);
    let mut socket = TcpSocket::new(stack, &mut rx, &mut tx);
    socket.set_timeout(Some(SILENT_MOST));
    let at = Instant::now();
    socket
        .connect((ip, 443))
        .await
        .map_err(|_| FetchError::Tcp)?;
    let tcp_ms = at.elapsed().as_millis();
    let context = Context::new(roots)?;
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
    wire.request(host, path).await?;
    let response = wire.response(&mut sha).await?;
    let body_ms = at.elapsed().as_millis();
    // Closed in an orderly way (a dropped socket sends nothing), for a moment at most.
    socket.close();
    let _ = select(socket.flush(), Timer::after(CLOSE_MOST)).await;
    Ok(Fetched {
        status: response.status().ok_or(FetchError::NoResponse)?,
        body: response.body_len(),
        digest: sha.finish(),
        dns_ms,
        tcp_ms,
        tls_ms,
        body_ms,
    })
}
