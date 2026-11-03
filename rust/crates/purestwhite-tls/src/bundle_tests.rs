//! The bundle check on the host: every current root of the C++ firmware's bundle accepted under
//! its own signature (RSA and ECDSA of each size the bundle holds), a changed byte refused, and
//! handshakes over wolfSSL's test chains (`rust/third_party/wolfssl/certs/intermediate`: a root,
//! two intermediates, the server) with a bundle of the test root, as the firmware makes them.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::bundle::accepts;
use crate::test_server::Server;
use crate::tests::{INSIDE, clock, shake, urandom};
use crate::{Connection, Context, Error, set_random, set_time};
use purestwhite_x509::{Bundle, Certificate, ESP_IDF_BUNDLE, Time, testing};
use std::vec::Vec;

/// 2025-01-01.
const NOW: u64 = 1_735_689_600;

fn file(path: &str) -> Vec<u8> {
    std::fs::read(std::format!("{}/../../{path}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The DER of every certificate in a PEM file.
fn ders(pem: &[u8]) -> Vec<Vec<u8>> {
    testing::certificates(std::str::from_utf8(pem).unwrap())
}

/// A bundle of the roots in the PEM files `pems`, leaked for the `'static` a context takes.
fn bundle_of(pems: &[&str]) -> &'static [u8] {
    let pem: std::string::String = pems
        .iter()
        .map(|p| std::string::String::from_utf8(file(p)).unwrap())
        .collect();
    std::boxed::Box::leak(testing::bundle_of_pem(&pem).into_boxed_slice())
}

#[test]
fn every_current_root_of_the_c_firmwares_bundle_is_accepted_under_its_own_signature() {
    let bundle = Bundle::new(ESP_IDF_BUNDLE).unwrap();
    let now = Time::from_unix(NOW);
    let roots = testing::certificates(testing::CACRT_ALL);
    let mut current = 0;
    for der in &roots {
        let cert = Certificate::read(der).unwrap();
        if !cert.is_current(now) {
            assert!(!accepts(&bundle, der, now), "out of its dates");
            continue;
        }
        current += 1;
        assert!(accepts(&bundle, der, now), "{:?}", cert.subject);
    }
    assert!(current > 140, "{current} current roots");
}

#[test]
fn a_changed_byte_is_refused() {
    let bundle = Bundle::new(ESP_IDF_BUNDLE).unwrap();
    let now = Time::from_unix(NOW);
    let roots = testing::certificates(testing::CACRT_ALL);
    for der in roots.iter().take(12) {
        let cert = Certificate::read(der).unwrap();
        let end = der.len() - 1;
        let signed_end = cert.signed.len();
        // The last byte of the signature, and a byte of the signed part past its names.
        for at in [end, signed_end - 1] {
            let mut bad = der.clone();
            bad[at] ^= 0x01;
            assert!(!accepts(&bundle, &bad, now), "byte {at}");
        }
    }
    assert!(!accepts(&bundle, b"", now));
}

/// A handshake with wolfSSL's test chain `chain` (in `certs/intermediate`; its key `key` in
/// `certs`), the client trusting `bundle`.
fn with_bundle(
    bundle: &'static [u8],
    chain: &str,
    key: &str,
    host: &str,
    now: u32,
) -> Result<(), Error> {
    set_random(urandom);
    set_time(now);
    let context = Context::bundle(bundle)?;
    let mut server = Server::new(
        &file(&std::format!(
            "third_party/wolfssl/certs/intermediate/{chain}"
        )),
        &file(&std::format!("third_party/wolfssl/certs/{key}")),
    )?;
    let mut client = Connection::new(&context, host)?;
    shake(&mut client, &mut server)
}

const RSA_ROOT: &str = "third_party/wolfssl/certs/ca-cert.pem";
const ECC_ROOT: &str = "third_party/wolfssl/certs/ca-ecc-cert.pem";

#[test]
fn a_chain_whose_top_a_bundle_root_signed_is_accepted() {
    let _clock = clock();
    let bundle = bundle_of(&[RSA_ROOT, ECC_ROOT]);
    let rsa = with_bundle(
        bundle,
        "server-chain.pem",
        "server-key.pem",
        "wolfSSL Server Chain",
        INSIDE,
    );
    assert_eq!(rsa, Ok(()));
    let ecc = with_bundle(
        bundle,
        "server-chain-ecc.pem",
        "ecc-key.pem",
        "wolfSSL Server Chain ECC",
        INSIDE,
    );
    assert_eq!(ecc, Ok(()));
}

#[test]
fn a_chain_no_bundle_root_signed_is_refused() {
    let _clock = clock();
    let ecc_only = bundle_of(&[ECC_ROOT]);
    let shaken = with_bundle(
        ecc_only,
        "server-chain.pem",
        "server-key.pem",
        "wolfSSL Server Chain",
        INSIDE,
    );
    assert_eq!(shaken, Err(Error::Wolf(-188)), "ASN_NO_SIGNER_E");
    let real = with_bundle(
        ESP_IDF_BUNDLE,
        "server-chain.pem",
        "server-key.pem",
        "wolfSSL Server Chain",
        INSIDE,
    );
    assert_eq!(real, Err(Error::Wolf(-188)), "a test root is not Mozilla's");
}

#[test]
fn the_rest_of_the_chain_and_the_name_are_still_checked() {
    let _clock = clock();
    let bundle = bundle_of(&[RSA_ROOT]);
    let shaken = with_bundle(
        bundle,
        "server-chain.pem",
        "server-key.pem",
        "github.com",
        INSIDE,
    );
    assert_eq!(shaken, Err(Error::Wolf(-322)), "DOMAIN_NAME_MISMATCH");
    let unset = with_bundle(
        bundle,
        "server-chain.pem",
        "server-key.pem",
        "wolfSSL Server Chain",
        0,
    );
    // The bundle check refuses the intermediate on its dates, so it is never a signer.
    assert_eq!(
        unset,
        Err(Error::Wolf(-188)),
        "no clock: the intermediate's dates are not met"
    );
}

#[test]
fn a_broken_bundle_is_refused_at_the_start() {
    assert!(matches!(Context::bundle(&[1, 2, 3]), Err(Error::Roots)));
}

#[test]
fn a_servers_own_certificate_signed_by_a_bundle_root_is_refused() {
    // As the C++ firmware: a root signs intermediates only, so the bundle is asked for the
    // topmost certificate below the server's own, never for the server's.
    let _clock = clock();
    set_random(urandom);
    set_time(INSIDE);
    let context = Context::bundle(bundle_of(&[RSA_ROOT])).unwrap();
    // server-cert.pem holds the root after the server's certificate: sent alone, it is the
    // server's certificate only.
    let pem = file("third_party/wolfssl/certs/server-cert.pem");
    let end = b"-----END CERTIFICATE-----";
    let cut = pem.windows(end.len()).position(|w| w == end).unwrap() + end.len();
    let key = file("third_party/wolfssl/certs/server-key.pem");
    let mut server = Server::new(&pem[..cut], &key).unwrap();
    let mut client = Connection::new(&context, "example.com").unwrap();
    assert_eq!(shake(&mut client, &mut server), Err(Error::Wolf(-188)));
}

#[test]
fn an_intermediate_out_of_its_dates_is_refused_by_the_bundle_check() {
    let bundle = Bundle::new(bundle_of(&[RSA_ROOT])).unwrap();
    let der = &ders(&file(
        "third_party/wolfssl/certs/intermediate/ca-int-cert.pem",
    ))[0];
    let cert = Certificate::read(der).unwrap();
    assert!(accepts(&bundle, der, cert.not_before));
    assert!(accepts(&bundle, der, cert.not_after));
    let before = Time {
        second: cert.not_before.second.wrapping_sub(1),
        ..cert.not_before
    };
    assert!(cert.not_before.second > 0 && !accepts(&bundle, der, before));
    assert!(
        !accepts(&bundle, der, Time::from_unix(4_102_444_800)),
        "2100"
    );
}

#[test]
fn each_connection_asks_the_bundle_again() {
    // As the C++ firmware, which makes a wolfSSL context per connection: an intermediate one
    // connection's check accepted does not sign for the next. The short chain (the server and
    // Intermediate2) needs Intermediate CA, which only the full chain brings.
    let _clock = clock();
    set_random(urandom);
    set_time(INSIDE);
    let context = Context::bundle(bundle_of(&[RSA_ROOT])).unwrap();
    let path = |name: &str| file(&std::format!("third_party/wolfssl/certs/{name}"));
    let host = "wolfSSL Server Chain";
    for (chain, want) in [
        ("intermediate/server-chain.pem", Ok(())),
        (
            "intermediate/server-chain-short.pem",
            Err(Error::Wolf(-188)),
        ),
    ] {
        let mut server = Server::new(&path(chain), &path("server-key.pem")).unwrap();
        let mut client = Connection::new(&context, host).unwrap();
        assert_eq!(shake(&mut client, &mut server), want, "{chain}");
    }
}
