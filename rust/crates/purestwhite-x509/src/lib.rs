//! The roots the bare-metal firmware trusts, as the C++ firmware trusts them: ESP-IDF's
//! certificate bundle (`x509_crt_bundle`, built from Mozilla's roots by ESP-IDF's
//! `gen_crt_bundle.py`, each root kept as its subject name and public key), looked up as ESP-IDF's
//! `esp_crt_bundle.c` looks it up; and a certificate read for what that check needs (its signed
//! part, issuer, dates, signature). The signature itself is checked by the TLS library
//! (`purestwhite-tls`). Nothing here allocates or panics (outside `testing`, for tests only).
#![no_std]
#![warn(clippy::pedantic)]
#![deny(clippy::indexing_slicing, clippy::panic, clippy::unwrap_used)]
#![forbid(unsafe_code)]

#[cfg(any(test, feature = "testing"))]
extern crate alloc;
#[cfg(test)]
extern crate std;

mod bundle;
mod cert;
mod der;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
#[cfg(test)]
mod tests;
mod time;

pub use bundle::{Bundle, BundleError, ROOTS_MOST, Root};
pub use cert::{Certificate, Hash, KeyKind, SignatureAlgorithm, key_kind};
pub use time::Time;

/// The bundle the C++ firmware links (ESP-IDF 5.5.5's, from `cacrt_all.pem`: 150 roots), copied
/// from its image; `roots/README.md` says how it was made and is checked.
pub static ESP_IDF_BUNDLE: &[u8] = include_bytes!("../roots/x509_crt_bundle");
