//! What the tests share: images built to order, and SHA-256 from `sha2`.
#![allow(
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    reason = "test images are built by index, and are small"
)]

extern crate std;

use crate::digest::Sha256;
use crate::image::{CHIP_ESP32S3, DIGEST, MAGIC};
use sha2::Digest;
use std::vec;
use std::vec::Vec;

/// The X4 Pro's OTA app slot.
pub const SLOT: usize = 0x64_0000;

/// SHA-256 from `sha2`, as the device's SHA unit gives it.
#[derive(Clone, Default)]
pub struct Sha(sha2::Sha256);

impl Sha256 for Sha {
    fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }
    fn finish(self) -> [u8; DIGEST] {
        self.0.finalize().into()
    }
}

/// An app descriptor naming `version` and `project`, as the first segment's start.
pub fn descriptor(version: &str, project: &str) -> Vec<u8> {
    let mut d = vec![0u8; 256];
    d[..4].copy_from_slice(&0xABCD_5432u32.to_le_bytes());
    d[16..16 + version.len()].copy_from_slice(version.as_bytes());
    d[48..48 + project.len()].copy_from_slice(project.as_bytes());
    d
}

/// An image for `chip` with `segments`, its checksum, and its digest when `flag` is 1.
pub fn image(chip: u16, segments: &[&[u8]], flag: u8) -> Vec<u8> {
    let mut b = vec![0u8; 24];
    b[0] = MAGIC;
    b[1] = segments.len() as u8;
    b[12..14].copy_from_slice(&chip.to_le_bytes());
    b[23] = flag;
    let mut xor = 0xEF;
    for (i, data) in segments.iter().enumerate() {
        b.extend_from_slice(&(0x3C00_0020 + i as u32 * 0x1_0000).to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        xor = data.iter().fold(xor, |x, d| x ^ d);
    }
    while b.len() % 16 != 15 {
        b.push(0);
    }
    b.push(xor);
    let digest = sha2::Sha256::digest(&b);
    b.extend_from_slice(&digest);
    b
}

/// A good image of about `size` bytes, its data a pattern.
pub fn good_image(size: usize) -> Vec<u8> {
    let first = descriptor("0.0.3", "arduino-lib-builder");
    let data: Vec<u8> = (0..size).map(|i| (i * 7 + i / 251) as u8).collect();
    image(CHIP_ESP32S3, &[&first, &[1, 2, 3, 4, 5], &data], 1)
}
