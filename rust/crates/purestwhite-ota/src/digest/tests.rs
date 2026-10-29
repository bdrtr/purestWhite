#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

extern crate std;

use super::*;
use crate::testing::{SLOT, Sha, descriptor, good_image, image};
use sha2::Digest;

#[test]
fn the_held_back_digest_is_of_all_but_the_last_32_bytes_however_they_come() {
    let bytes = good_image(3000);
    let expected: [u8; DIGEST] = sha2::Sha256::digest(&bytes[..bytes.len() - DIGEST]).into();
    for chunk in [1, 5, 31, 32, 33, 100, bytes.len()] {
        let mut held = HeldBack::new(Sha::default());
        for piece in bytes.chunks(chunk) {
            held.update(piece);
        }
        assert_eq!(held.finish(), Ok(expected), "{chunk}");
    }
}

#[test]
fn fewer_than_32_bytes_or_a_wrong_last_32_do_not_hold() {
    let mut held = HeldBack::new(Sha::default());
    held.update(&[1; 20]);
    assert_eq!(held.finish(), Err(DigestError::Mismatch));
    let mut bytes = good_image(100);
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    let mut held = HeldBack::new(Sha::default());
    held.update(&bytes);
    assert_eq!(held.finish(), Err(DigestError::Mismatch));
}

#[test]
fn an_image_is_identified_by_its_digest_without_anything_written() {
    let bytes = good_image(5000);
    let mut identify = Identify::new(SLOT, Sha::default());
    for piece in bytes.chunks(512) {
        identify.feed(piece).unwrap();
    }
    let identified = identify.finish().unwrap();
    assert_eq!(identified.checked.len, bytes.len());
    assert_eq!(&identified.digest[..], &bytes[bytes.len() - DIGEST..]);
}

#[test]
fn an_image_with_another_digest_appended_is_refused() {
    let mut bytes = good_image(100);
    let last = bytes.len() - 1;
    bytes[last] ^= 0x80;
    let mut identify = Identify::new(SLOT, Sha::default());
    identify.feed(&bytes).unwrap();
    assert_eq!(identify.finish(), Err(DigestError::Mismatch));
    let other_chip = image(5, &[&descriptor("v", "p")], 1);
    let mut identify = Identify::new(SLOT, Sha::default());
    assert_eq!(
        identify.feed(&other_chip),
        Err(crate::image::ImageError::Chip(5))
    );
}
