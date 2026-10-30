//! The card's and the slot's images, on their own: what the updater's steps are built on.

use super::fixtures::{block_on, card_with, digest, slot_with};
use crate::Card;
use crate::images::{FileError, InstallError, identify_file, identify_slot, install_file};
use crate::releases::RECOVERY_FILE;
use purestwhite_ota::image::ImageError;
use purestwhite_ota::testing::{FakeFlash, Sha, good_image};

#[test]
fn a_file_is_identified_as_its_image() {
    let image = good_image(20_000);
    let Some(mut card) = card_with(Some(&image)) else {
        return;
    };
    let mut volume = card.open(&mut || {}).unwrap();
    let found = block_on(identify_file(
        &mut volume,
        RECOVERY_FILE,
        super::SLOT,
        Sha::default(),
        || {},
    ));
    let found = found.unwrap();
    assert_eq!(found.checked.len, image.len());
    assert_eq!(found.digest, digest(&image));
}

#[test]
fn bytes_after_the_image_refuse_the_file() {
    let mut image = good_image(20_000);
    image.extend_from_slice(&[0; 16]);
    let Some(mut card) = card_with(Some(&image)) else {
        return;
    };
    let mut volume = card.open(&mut || {}).unwrap();
    let found = block_on(identify_file(
        &mut volume,
        RECOVERY_FILE,
        super::SLOT,
        Sha::default(),
        || {},
    ));
    assert_eq!(found.unwrap_err(), FileError::Image(ImageError::Trailing));
}

/// The second pass writes the file as it is now: when that is not the image the first pass read
/// (`expected`), it says so, after the write.
#[test]
fn a_file_changed_between_the_passes_is_found_out() {
    let image = good_image(20_000);
    let Some(mut card) = card_with(Some(&image)) else {
        return;
    };
    let mut volume = card.open(&mut || {}).unwrap();
    let mut slot = FakeFlash::new(super::SLOT);
    let other = digest(&good_image(30_000));
    let written = block_on(install_file(
        &mut volume,
        RECOVERY_FILE,
        &mut slot,
        &other,
        Sha::default(),
        || {},
    ));
    assert_eq!(written.unwrap_err(), InstallError::Changed);
    assert_eq!(slot.mem[..image.len()], *image);
}

#[test]
fn a_slot_is_identified_where_it_lies() {
    let image = good_image(20_000);
    let mut slot = slot_with(&image);
    let mut pieces = 0;
    let found = identify_slot(&mut slot, Sha::default(), || pieces += 1).unwrap();
    assert_eq!(found.digest, digest(&image));
    assert_eq!(
        pieces,
        image.len().div_ceil(4096),
        "it reads no further than the image"
    );
}

#[test]
fn an_erased_slot_holds_no_image() {
    let mut slot = FakeFlash::new(super::SLOT);
    slot.mem.fill(0xFF);
    let found = identify_slot(&mut slot, Sha::default(), || {});
    assert!(found.is_err());
}
