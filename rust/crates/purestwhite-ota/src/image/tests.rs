// The tests build images by index and unwrap what must be there: a failure fails the test.
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

extern crate std;

use super::*;
use crate::testing::{SLOT, descriptor, image};
use sha2::{Digest, Sha256};
use std::vec;
use std::vec::Vec;

fn good() -> Vec<u8> {
    let first = descriptor("0.0.3", "arduino-lib-builder");
    image(CHIP_ESP32S3, &[&first, &[1, 2, 3, 4, 5], &[0xAA; 1000]], 1)
}

/// The check of `bytes` fed `chunk` bytes at a time.
fn check(bytes: &[u8], chunk: usize, slot: usize) -> Result<Checked, ImageError> {
    let mut c = ImageCheck::new(slot);
    for piece in bytes.chunks(chunk) {
        c.feed(piece)?;
    }
    c.finish()
}

fn text(field: &[u8; 32]) -> &str {
    std::str::from_utf8(field).unwrap().trim_end_matches('\0')
}

#[test]
fn a_good_image_passes_in_pieces_of_any_size_and_its_digest_is_where_it_says() {
    let bytes = good();
    for chunk in [1, 3, 7, 16, 24, 4096, bytes.len()] {
        let checked = check(&bytes, chunk, SLOT).unwrap();
        assert_eq!(checked.len, bytes.len(), "{chunk}");
        let digest = Sha256::digest(&bytes[..checked.hash_at]);
        assert_eq!(&bytes[checked.hash_at..], digest.as_slice());
        assert_eq!(text(&checked.desc.version), "0.0.3");
        assert_eq!(text(&checked.desc.project), "arduino-lib-builder");
    }
}

#[test]
fn the_checksum_ends_a_block_whatever_the_segments_lengths() {
    // From no padding at all to fifteen bytes of it.
    for extra in 0..16 {
        let first = descriptor("v", "p");
        let bytes = image(CHIP_ESP32S3, &[&first, &vec![7; extra]], 1);
        assert!(check(&bytes, 5, SLOT).is_ok(), "{extra}");
    }
}

#[test]
fn what_is_not_an_esp32s3_image_with_a_digest_is_refused() {
    let first = descriptor("v", "p");
    let mut not_an_image = good();
    not_an_image[0] = 0x00;
    assert_eq!(check(&not_an_image, 64, SLOT), Err(ImageError::Magic));
    let esp32c3 = image(5, &[&first], 1);
    assert_eq!(check(&esp32c3, 64, SLOT), Err(ImageError::Chip(5)));
    let no_digest = image(CHIP_ESP32S3, &[&first], 0);
    assert_eq!(check(&no_digest, 64, SLOT), Err(ImageError::NoDigest));
    let none = image(CHIP_ESP32S3, &[], 1);
    assert_eq!(check(&none, 64, SLOT), Err(ImageError::Segments(0)));
    let seventeen: Vec<&[u8]> = vec![&first; 17];
    let too_many = image(CHIP_ESP32S3, &seventeen, 1);
    assert_eq!(check(&too_many, 64, SLOT), Err(ImageError::Segments(17)));
}

#[test]
fn a_damaged_image_is_refused() {
    let bytes = good();
    // A byte of the last segment's data changed: the checksum no longer holds.
    let mut changed = bytes.clone();
    changed[24 + 8 + 256 + 8 + 5 + 8 + 10] ^= 1;
    assert_eq!(check(&changed, 100, SLOT), Err(ImageError::Checksum));
    let short = &bytes[..bytes.len() - 1];
    assert_eq!(check(short, 100, SLOT), Err(ImageError::Truncated));
    let mut longer = bytes.clone();
    longer.push(0);
    assert_eq!(check(&longer, 100, SLOT), Err(ImageError::Trailing));
}

#[test]
fn an_image_without_its_descriptor_is_refused() {
    let first = vec![0u8; 256];
    let bytes = image(CHIP_ESP32S3, &[&first], 1);
    assert_eq!(check(&bytes, 64, SLOT), Err(ImageError::Descriptor));
    // A first segment shorter than the descriptor.
    let bytes = image(CHIP_ESP32S3, &[&descriptor("v", "p")[..40]], 1);
    assert_eq!(check(&bytes, 64, SLOT), Err(ImageError::Descriptor));
}

#[test]
fn an_image_larger_than_the_slot_is_refused_as_soon_as_it_shows() {
    let bytes = good();
    assert_eq!(
        check(&bytes, 64, bytes.len() - 1),
        Err(ImageError::TooLarge)
    );
    assert!(check(&bytes, 64, bytes.len()).is_ok());
    // A segment that says it is longer than the slot, before its data comes.
    let mut c = ImageCheck::new(SLOT);
    let mut head = bytes[..HEADER + SEGMENT_HEADER].to_vec();
    head[HEADER + 4..HEADER + 8].copy_from_slice(&(SLOT as u32 + 1).to_le_bytes());
    assert_eq!(c.feed(&head), Err(ImageError::TooLarge));
}

/// A real image, when `PW_IMAGE` names one (for example a release from the archive): it passes,
/// and its digest holds.
#[test]
fn a_real_image_when_one_is_given() {
    let Ok(path) = std::env::var("PW_IMAGE") else {
        return;
    };
    let bytes = std::fs::read(path).unwrap();
    let checked = check(&bytes, 4096, SLOT).unwrap();
    let digest = Sha256::digest(&bytes[..checked.hash_at]);
    assert_eq!(&bytes[checked.hash_at..], digest.as_slice());
    std::println!(
        "{} {} {} bytes",
        text(&checked.desc.project),
        text(&checked.desc.version),
        checked.len
    );
}
