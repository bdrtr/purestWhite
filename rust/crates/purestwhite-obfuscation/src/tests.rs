// The tests index what must be there and fail on what is not: a failure fails the test.
#![allow(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]

use super::base64::{Invalid, decode_into, decoded_len, encode};
use super::{KEY_LEN, decoded};
use std::vec::Vec;

/// A made-up device key: no real device's MAC address.
const KEY: [u8; KEY_LEN] = [0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC];

fn decode(encoded: &str) -> Result<Vec<u8>, Invalid> {
    decoded_len(encoded.as_bytes())?;
    let mut out = Vec::new();
    decode_into(encoded.as_bytes(), &mut out);
    Ok(out)
}

#[test]
fn base64_decodes_rfc_4648s_vectors_and_refuses_the_rest() {
    let vectors = [
        ("", ""),
        ("Zg==", "f"),
        ("Zm8=", "fo"),
        ("Zm9v", "foo"),
        ("Zm9vYg==", "foob"),
        ("Zm9vYmE=", "fooba"),
        ("Zm9vYmFy", "foobar"),
    ];
    for (encoded, plain) in vectors {
        assert_eq!(decode(encoded).unwrap(), plain.as_bytes(), "{encoded}");
        assert_eq!(decoded_len(encoded.as_bytes()), Ok(plain.len()));
        assert_eq!(&*encode(plain.as_bytes()).unwrap(), encoded.as_bytes());
    }
    // mbedtls 3.6 skips line breaks anywhere, and spaces before one or at the end.
    let spaced = [
        ("Zm9v\n", "foo"),
        ("Zm\r\n9v", "foo"),
        ("Zm9v  ", "foo"),
        ("Zm  \n9v", "foo"),
        ("Zg=\n=", "f"),
        ("  ", ""),
        ("\n", ""),
    ];
    for (encoded, plain) in spaced {
        assert_eq!(decode(encoded).unwrap(), plain.as_bytes(), "{encoded:?}");
    }
    for bad in [
        "Zg=",
        "Z===",
        "Zg=a",
        "Z=g=",
        "Zm 9v",
        " Zm9v",
        "Zm9v\r",
        "Zm\r9v",
        "Z\u{80}9v",
        "Zm9vY",
    ] {
        assert_eq!(decode(bad), Err(Invalid), "{bad:?}");
    }
}

#[test]
fn a_secret_is_unmasked_with_the_key_and_kept_to_its_limit() {
    // "secret" masked with KEY, then base64: as the C++ firmware writes it.
    let masked: Vec<u8> = b"secret"
        .iter()
        .zip(KEY.iter().cycle())
        .map(|(b, k)| b ^ k)
        .collect();
    let stored = encode(&masked).unwrap();
    let got = decoded(&stored, 64, &KEY).unwrap();
    assert!(got.ok && !got.too_long);
    assert_eq!(&*got.text, b"secret");
    let short = decoded(&stored, 5, &KEY).unwrap();
    assert!(short.too_long && !short.ok && short.text.is_empty());
    for not_one in [&b""[..], b"!!!!", b"Zg="] {
        assert!(!decoded(not_one, 64, &KEY).unwrap().ok, "{not_one:?}");
    }
    // Only spaces: mbedtls decodes nothing, which is an empty secret.
    let blank = decoded(b"  ", 64, &KEY).unwrap();
    assert!(blank.ok && blank.text.is_empty());
    // Not base64 wins over too long, as in mbedtls.
    let long_bad = std::format!("!!!!{}", "A".repeat(84));
    assert!(!decoded(long_bad.as_bytes(), 4, &KEY).unwrap().too_long);
}

#[test]
fn encoding_pads_and_never_breaks_lines() {
    let long = encode(&[0xAB; 100]).unwrap();
    assert_eq!(long.len(), 136);
    assert!(!long.contains(&b'\n'));
    assert_eq!(&*encode(b"user:pass").unwrap(), b"dXNlcjpwYXNz");
}
