//! For tests here and in the crates that use this one (the `testing` feature): certificates out of
//! PEM, a bundle built as ESP-IDF's `gen_crt_bundle.py` builds one, and the roots the C++
//! firmware's bundle was built from.
#![allow(
    clippy::missing_panics_doc,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use alloc::vec::Vec;

/// ESP-IDF 5.5.5's `cacrt_all.pem`, from which `ESP_IDF_BUNDLE` was built.
pub const CACRT_ALL: &str = include_str!("../testdata/cacrt_all.pem");

/// The DER of every certificate in `pem`.
#[must_use]
pub fn certificates(pem: &str) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut body: Option<Vec<u8>> = None;
    for line in pem.lines() {
        match line.trim() {
            "-----BEGIN CERTIFICATE-----" => body = Some(Vec::new()),
            "-----END CERTIFICATE-----" => out.push(base64(&body.take().unwrap())),
            text => {
                if let Some(body) = body.as_mut() {
                    body.extend_from_slice(text.as_bytes());
                }
            }
        }
    }
    out
}

fn base64(text: &[u8]) -> Vec<u8> {
    const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let values: Vec<u32> = text
        .iter()
        .filter(|c| **c != b'=')
        .map(|c| {
            let at = DIGITS.iter().position(|d| d == c).unwrap();
            u32::try_from(at).unwrap()
        })
        .collect();
    let mut out = Vec::new();
    for chunk in values.chunks(4) {
        let word = chunk
            .iter()
            .enumerate()
            .fold(0, |w, (i, v)| w | v << (18 - 6 * i));
        out.extend_from_slice(&word.to_be_bytes()[1..chunk.len()]);
    }
    out
}

/// A bundle of `(name, key)` roots laid out as `gen_crt_bundle.py` lays it out: sorted by name.
#[must_use]
pub fn bundle(mut roots: Vec<(&[u8], &[u8])>) -> Vec<u8> {
    roots.sort_by(|a, b| a.0.cmp(b.0));
    let mut out = Vec::new();
    let mut body = Vec::new();
    for (name, key) in &roots {
        let at = u32::try_from(4 * roots.len() + body.len()).unwrap();
        out.extend_from_slice(&at.to_le_bytes());
        body.extend_from_slice(&u16::try_from(name.len()).unwrap().to_le_bytes());
        body.extend_from_slice(&u16::try_from(key.len()).unwrap().to_le_bytes());
        body.extend_from_slice(name);
        body.extend_from_slice(key);
    }
    out.extend(body);
    out
}

/// A bundle of the roots in `pem` (their subjects and keys).
#[must_use]
pub fn bundle_of_pem(pem: &str) -> Vec<u8> {
    let ders = certificates(pem);
    let roots = ders
        .iter()
        .map(|d| {
            let c = crate::Certificate::read(d).unwrap();
            (c.subject, c.public_key)
        })
        .collect();
    bundle(roots)
}
