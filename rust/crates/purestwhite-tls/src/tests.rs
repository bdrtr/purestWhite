//! Handshakes on the host with wolfSSL's own server and test certificates
//! (`rust/third_party/wolfssl/certs`, valid from December 2023 to September 2026): the chain and the
//! name checked, the dates against the clock set, and data both ways.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::test_server::Server;
use crate::{Connection, Context, Error, set_random, set_time};
use std::io::Read;
use std::sync::Mutex;
use std::vec::Vec;

/// 2025-01-01, inside the test certificates' dates; 2027-01-01, after them.
const INSIDE: u32 = 1_735_689_600;
const AFTER: u32 = 1_798_761_600;

/// The clock is one for the whole process: the tests take turns.
static CLOCK: Mutex<()> = Mutex::new(());

/// The clock's turn; a test that failed holding it does not fail the others.
fn clock() -> std::sync::MutexGuard<'static, ()> {
    CLOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn cert(name: &str) -> Vec<u8> {
    let path = std::format!(
        "{}/../../third_party/wolfssl/certs/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(path).unwrap()
}

fn urandom(bytes: &mut [u8]) {
    std::fs::File::open("/dev/urandom")
        .unwrap()
        .read_exact(bytes)
        .unwrap();
}

/// Moves the bytes between `client` and `server` until the client's handshake is done or fails.
fn shake(client: &mut Connection<'_>, server: &mut Server) -> Result<(), Error> {
    for _ in 0..50 {
        let done = client.handshake()?;
        let out = client.to_send().to_vec();
        client.sent(out.len());
        server.received(&out);
        let _ = server.accept();
        client.received(&server.take_out());
        if done {
            return Ok(());
        }
    }
    panic!("the handshake did not end");
}

/// A handshake with a server of `chain` and `key`, the client trusting `root`, for `host`, at
/// `now`.
fn handshake(root: &str, chain: &str, key: &str, host: &str, now: u32) -> Result<(), Error> {
    set_random(urandom);
    set_time(now);
    let context = Context::new(&cert(root))?;
    let mut server = Server::new(&cert(chain), &cert(key))?;
    let mut client = Connection::new(&context, host)?;
    shake(&mut client, &mut server)
}

#[test]
fn a_chain_signed_by_a_trusted_root_for_the_host_is_accepted_and_data_flows() {
    let _clock = clock();
    set_random(urandom);
    set_time(INSIDE);
    let context = Context::new(&cert("ca-cert.pem")).unwrap();
    let mut server = Server::new(&cert("server-cert.pem"), &cert("server-key.pem")).unwrap();
    let mut client = Connection::new(&context, "example.com").unwrap();
    shake(&mut client, &mut server).unwrap();
    assert_eq!(client.write(b"GET / HTTP/1.0\r\n\r\n").unwrap(), 18);
    server.received(client.to_send());
    let sent = client.to_send().len();
    client.sent(sent);
    let mut request = [0u8; 64];
    let n = server.read(&mut request).unwrap().unwrap();
    assert_eq!(&request[..n], b"GET / HTTP/1.0\r\n\r\n");
    server.write(b"HTTP/1.0 200 OK\r\n\r\nhello").unwrap();
    client.received(&server.take_out());
    let mut response = [0u8; 64];
    let n = client.read(&mut response).unwrap().unwrap();
    assert_eq!(&response[..n], b"HTTP/1.0 200 OK\r\n\r\nhello");
    assert_eq!(client.read(&mut response), Ok(None), "nothing more yet");
}

#[test]
fn an_ecc_chain_is_accepted() {
    let _clock = clock();
    let shaken = handshake(
        "ca-ecc-cert.pem",
        "server-ecc.pem",
        "ecc-key.pem",
        "www.wolfssl.com",
        INSIDE,
    );
    assert_eq!(shaken, Ok(()));
}

#[test]
fn another_hosts_certificate_is_refused() {
    let _clock = clock();
    let shaken = handshake(
        "ca-cert.pem",
        "server-cert.pem",
        "server-key.pem",
        "github.com",
        INSIDE,
    );
    assert_eq!(shaken, Err(Error::Wolf(-322)), "DOMAIN_NAME_MISMATCH");
}

#[test]
fn a_chain_no_trusted_root_signed_is_refused() {
    let _clock = clock();
    let shaken = handshake(
        "ca-ecc-cert.pem",
        "server-cert.pem",
        "server-key.pem",
        "example.com",
        INSIDE,
    );
    assert_eq!(shaken, Err(Error::Wolf(-188)), "ASN_NO_SIGNER_E");
}

#[test]
fn a_certificate_past_its_dates_is_refused() {
    let _clock = clock();
    let shaken = handshake(
        "ca-cert.pem",
        "server-cert.pem",
        "server-key.pem",
        "example.com",
        AFTER,
    );
    assert_eq!(shaken, Err(Error::Wolf(-151)), "ASN_AFTER_DATE_E");
}

#[test]
fn a_name_with_a_nul_or_none_is_refused() {
    let _clock = clock();
    set_random(urandom);
    let context = Context::new(&cert("ca-cert.pem")).unwrap();
    assert!(matches!(Connection::new(&context, ""), Err(Error::Name)));
    assert!(matches!(
        Connection::new(&context, "a\0b"),
        Err(Error::Name)
    ));
}

#[test]
fn a_tls_1_2_server_is_spoken_to_too() {
    let _clock = clock();
    set_random(urandom);
    set_time(INSIDE);
    let context = Context::new(&cert("ca-ecc-cert.pem")).unwrap();
    let mut server = Server::tls12(&cert("server-ecc.pem"), &cert("ecc-key.pem")).unwrap();
    let mut client = Connection::new(&context, "www.wolfssl.com").unwrap();
    assert_eq!(shake(&mut client, &mut server), Ok(()));
}
