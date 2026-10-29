#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::cast_possible_truncation
)]

extern crate std;

use super::*;
use crate::image::{DIGEST, ImageError};
use crate::testing::{FakeError, FakeFlash, Sha, good_image};
use embedded_storage::nor_flash::ReadNorFlash;

/// Writes `bytes` in pieces of `chunk` into `flash`: the writer's end, and the flash.
fn write(
    bytes: &[u8],
    chunk: usize,
    flash: FakeFlash,
) -> (Result<Identified, WriteError<FakeError>>, FakeFlash) {
    let mut flash = flash;
    let result = {
        let mut writer = SlotWriter::new(flash.capacity(), Sha::default());
        let mut fed = Ok(());
        for piece in bytes.chunks(chunk) {
            fed = writer.write(&mut flash, piece);
            if fed.is_err() {
                break;
            }
        }
        fed.and_then(|()| writer.finish(&mut flash))
    };
    (result, flash)
}

#[test]
fn an_image_is_written_sector_by_sector_whatever_its_pieces() {
    let bytes = good_image(10_000);
    for chunk in [1, 100, 512, 4096, 5000] {
        let (result, flash) = write(&bytes, chunk, FakeFlash::new(64 * 1024));
        let written = result.unwrap();
        assert_eq!(&written.digest[..], &bytes[bytes.len() - DIGEST..]);
        assert_eq!(&flash.mem[..bytes.len()], &bytes[..], "{chunk}");
        // Each sector the image touches, once; nothing past them.
        let sectors = bytes.len().div_ceil(SECTOR);
        assert_eq!(flash.erases.len(), sectors);
        assert!(flash.mem[sectors * SECTOR..].iter().all(|&b| b == 0x5A));
    }
}

#[test]
fn a_refused_image_erases_and_writes_nothing() {
    let mut bytes = good_image(10_000);
    bytes[12] = 5;
    let (result, flash) = write(&bytes, 512, FakeFlash::new(64 * 1024));
    assert_eq!(result.err(), Some(WriteError::Image(ImageError::Chip(5))));
    assert!(flash.erases.is_empty());
    assert!(flash.mem.iter().all(|&b| b == 0x5A));
}

#[test]
fn an_image_larger_than_the_slot_is_refused_before_it_passes_the_slot() {
    let bytes = good_image(10_000);
    let (result, flash) = write(&bytes, 512, FakeFlash::new(8 * 1024));
    assert_eq!(result.err(), Some(WriteError::Image(ImageError::TooLarge)));
    assert!(flash.erases.iter().all(|&(_, to)| to as usize <= 8 * 1024));
}

#[test]
fn a_bit_that_does_not_take_is_found_when_read_back() {
    let bytes = good_image(10_000);
    let mut flash = FakeFlash::new(64 * 1024);
    // In the second sector, where the image is not 0 there.
    let stuck = 4096 + (0..4096).find(|&i| bytes[4096 + i] & 1 == 1).unwrap();
    flash.stuck = Some(stuck);
    let (result, _) = write(&bytes, 512, flash);
    assert_eq!(
        result.err(),
        Some(WriteError::ReadBack(
            4096 + (stuck as u32 - 4096) / 256 * 256
        ))
    );
}

#[test]
fn a_failed_write_is_reported() {
    let bytes = good_image(10_000);
    let mut flash = FakeFlash::new(64 * 1024);
    flash.fail_write_at = Some(4096);
    let (result, _) = write(&bytes, 512, flash);
    assert_eq!(result.err(), Some(WriteError::Flash(FakeError)));
}

#[test]
fn an_image_whose_appended_digest_is_wrong_is_written_but_refused() {
    let mut bytes = good_image(10_000);
    let last = bytes.len() - 1;
    bytes[last] ^= 1;
    let (result, flash) = write(&bytes, 512, FakeFlash::new(64 * 1024));
    assert_eq!(result.err(), Some(WriteError::Digest));
    // Written, so the caller must not select the slot.
    assert_eq!(&flash.mem[..bytes.len()], &bytes[..]);
}

#[test]
fn the_two_passes_give_the_same_digest() {
    let bytes = good_image(20_000);
    let mut identify = crate::digest::Identify::new(64 * 1024, Sha::default());
    identify.feed(&bytes).unwrap();
    let first = identify.finish().unwrap();
    let (second, _) = write(&bytes, 4096, FakeFlash::new(64 * 1024));
    assert_eq!(second.unwrap().digest, first.digest);
}

#[test]
fn a_flash_not_the_slots_size_is_refused_before_anything_is_written() {
    let bytes = good_image(10_000);
    let mut flash = FakeFlash::new(8 * 1024);
    let mut writer = SlotWriter::new(64 * 1024, Sha::default());
    assert_eq!(
        writer.write(&mut flash, &bytes[..4096]),
        Err(WriteError::SlotSize)
    );
    assert!(flash.erases.is_empty());
    // A larger flash too: it is not the slot (for example the whole partition for a window).
    let mut larger = FakeFlash::new(128 * 1024);
    assert_eq!(
        writer.write(&mut larger, &bytes[..4096]),
        Err(WriteError::SlotSize)
    );
    assert!(larger.erases.is_empty());
    assert_eq!(writer.finish(&mut flash).err(), Some(WriteError::SlotSize));
}
