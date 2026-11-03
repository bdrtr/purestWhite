//! The bundle rebuilt from the roots it came from, each root found under its own name, broken
//! bundles refused, dates read as mbedTLS reads them, and broken certificates read without a
//! fault.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use crate::{
    Bundle, BundleError, Certificate, ESP_IDF_BUNDLE, KeyKind, ROOTS_MOST, Time, key_kind,
};
use std::vec::Vec;

const PEM: &str = crate::testing::CACRT_ALL;

use crate::testing::{bundle as build, certificates};

#[test]
fn the_c_firmwares_bundle_is_its_roots_rebuilt_byte_for_byte() {
    let ders = certificates(PEM);
    assert_eq!(ders.len(), 150);
    let certs: Vec<Certificate<'_>> = ders.iter().map(|d| Certificate::read(d).unwrap()).collect();
    let roots = certs.iter().map(|c| (c.subject, c.public_key)).collect();
    assert_eq!(build(roots), ESP_IDF_BUNDLE);
    let bundle = Bundle::new(ESP_IDF_BUNDLE).unwrap();
    assert_eq!(bundle.len(), 150);
}

#[test]
fn every_root_is_found_under_its_own_name_with_its_own_key() {
    let bundle = Bundle::new(ESP_IDF_BUNDLE).unwrap();
    for der in certificates(PEM) {
        let cert = Certificate::read(&der).unwrap();
        assert_eq!(cert.issuer, cert.subject, "a root signs itself");
        let root = bundle.find(cert.issuer).unwrap();
        assert_eq!(root.key, cert.public_key);
        assert!(key_kind(root.key).is_some(), "an RSA or EC key");
        let algorithm = cert.signature_algorithm().unwrap();
        assert_eq!(Some(algorithm.key), key_kind(root.key));
    }
    assert!(bundle.find(&[]).is_none());
    assert!(bundle.find(b"\x30\x03\x31\x01\x00").is_none());
}

#[test]
fn a_name_found_is_compared_over_the_shorter_length_as_esp_idf_compares() {
    // Bytes that are not DER, where one is the start of the other.
    let (a, b) = (&b"ab"[..], &b"b"[..]);
    let bundle = build(std::vec![(a, &b"k1"[..]), (b, &b"k2"[..])]);
    let bundle = Bundle::new(&bundle).unwrap();
    assert_eq!(
        bundle.find(b"abc").unwrap().key,
        b"k1",
        "\"ab\" is the start of \"abc\""
    );
    assert_eq!(bundle.find(b"a").unwrap().key, b"k1", "and the other way");
    assert_eq!(bundle.find(b"c"), None);
    assert_eq!(bundle.get(2), None);
}

#[test]
fn a_broken_bundle_is_refused() {
    let good = ESP_IDF_BUNDLE.to_vec();
    assert_eq!(Bundle::new(&good[..8]).unwrap_err(), BundleError::Short);
    let mut odd = good.clone();
    odd[0] = 3;
    assert_eq!(Bundle::new(&odd).unwrap_err(), BundleError::Offsets);
    let mut many = good.clone();
    many[..4].copy_from_slice(&u32::try_from(4 * (ROOTS_MOST + 1)).unwrap().to_le_bytes());
    assert_eq!(Bundle::new(&many).unwrap_err(), BundleError::TooMany);
    let mut moved = good.clone();
    moved[4] ^= 1;
    assert_eq!(Bundle::new(&moved).unwrap_err(), BundleError::Root);
    assert_eq!(
        Bundle::new(&good[..good.len() - 1]).unwrap_err(),
        BundleError::Root,
        "the last root past the end"
    );
    for len in 0..=good.len().min(2000) {
        let _ = Bundle::new(&good[..len]);
    }
}

#[test]
fn a_certificates_dates_are_read_as_mbedtls_reads_them() {
    let der = &certificates(PEM)[0];
    let cert = Certificate::read(der).unwrap();
    assert!(cert.not_before < cert.not_after);
    let read = |tag: u8, text: &[u8]| {
        let mut der = std::vec![tag, u8::try_from(text.len()).unwrap()];
        der.extend_from_slice(text);
        let (e, _) = crate::der::next(&der).map(|(e, r)| (e, r.len())).unwrap();
        Time::read(&e)
    };
    let t = |year, month, day, hour, minute, second| Time {
        year,
        month,
        day,
        hour,
        minute,
        second,
    };
    assert_eq!(
        read(0x17, b"491231235959Z"),
        Some(t(2049, 12, 31, 23, 59, 59))
    );
    assert_eq!(read(0x17, b"500101000000Z"), Some(t(1950, 1, 1, 0, 0, 0)));
    assert_eq!(
        read(0x17, b"240229120000"),
        Some(t(2024, 2, 29, 12, 0, 0)),
        "no Z"
    );
    assert_eq!(read(0x18, b"21000301000000Z"), Some(t(2100, 3, 1, 0, 0, 0)));
    for bad in [
        &b"230229120000Z"[..],
        b"2402291200Z",
        b"240229120060Z",
        b"241301000000Z",
        b"240229120000X",
        b"24022912000aZ",
    ] {
        assert_eq!(read(0x17, bad), None, "{bad:?}");
    }
    assert_eq!(
        read(0x18, b"21000229000000Z"),
        None,
        "2100 is not a leap year"
    );
}

#[test]
fn unix_time_reads_as_the_calendar_says() {
    let t = Time::from_unix(0);
    assert_eq!((t.year, t.month, t.day, t.hour), (1970, 1, 1, 0));
    let t = Time::from_unix(1_709_208_000); // 2024-02-29 12:00:00
    assert_eq!(
        (t.year, t.month, t.day, t.hour, t.minute),
        (2024, 2, 29, 12, 0)
    );
    let t = Time::from_unix(4_102_444_799); // 2099-12-31 23:59:59
    assert_eq!((t.year, t.month, t.day, t.second), (2099, 12, 31, 59));
    let ders = certificates(PEM);
    let cert = Certificate::read(&ders[0]).unwrap();
    assert!(!cert.is_current(Time::from_unix(0)));
}

#[test]
fn a_broken_certificate_is_read_without_a_fault() {
    let der = certificates(PEM)[3].clone();
    for len in 0..der.len() {
        assert!(Certificate::read(&der[..len]).is_none(), "cut at {len}");
    }
    for at in 0..der.len() {
        let mut bad = der.clone();
        bad[at] ^= 0xFF;
        let _ = Certificate::read(&bad);
    }
    let mut longer = der.clone();
    longer.push(0);
    assert!(Certificate::read(&longer).is_none(), "one certificate only");
}

#[test]
fn the_roots_keys_are_rsa_and_ec() {
    let bundle = Bundle::new(ESP_IDF_BUNDLE).unwrap();
    let kinds: Vec<_> = (0..bundle.len())
        .map(|i| key_kind(bundle.get(i).unwrap().key).unwrap())
        .collect();
    assert!(kinds.contains(&KeyKind::Rsa) && kinds.contains(&KeyKind::Ec));
}
