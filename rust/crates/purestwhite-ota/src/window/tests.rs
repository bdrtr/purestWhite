#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation
)]

extern crate std;

use super::*;
use crate::image::DIGEST;
use crate::slot::SlotWriter;
use crate::testing::{FakeFlash, Sha, good_image};

#[test]
fn a_window_must_lie_within_the_flash_on_its_erase_size() {
    let mut flash = FakeFlash::new(64 * 1024);
    assert!(Window::new(&mut flash, 0, 64 * 1024).is_some());
    assert!(Window::new(&mut flash, 32 * 1024, 32 * 1024).is_some());
    assert!(Window::new(&mut flash, 32 * 1024, 36 * 1024).is_none());
    assert!(Window::new(&mut flash, 100, 4096).is_none());
    assert!(Window::new(&mut flash, 4096, 100).is_none());
    assert!(Window::new(&mut flash, u32::MAX - 4095, 8192).is_none());
}

#[test]
fn reads_writes_and_erases_land_inside_the_window_only() {
    let mut flash = FakeFlash::new(64 * 1024);
    {
        let mut w = Window::new(&mut flash, 48 * 1024, 16 * 1024).unwrap();
        assert_eq!(w.capacity(), 16 * 1024);
        w.erase(0, 4096).unwrap();
        w.write(8, &[1, 2, 3, 4]).unwrap();
        let mut back = [0u8; 4];
        w.read(8, &mut back).unwrap();
        assert_eq!(back, [1, 2, 3, 4]);
        assert_eq!(
            w.read(16 * 1024 - 2, &mut back),
            Err(WindowError::OutOfBounds)
        );
        assert_eq!(w.write(16 * 1024, &[0; 4]), Err(WindowError::OutOfBounds));
        assert_eq!(w.erase(12 * 1024, 20 * 1024), Err(WindowError::OutOfBounds));
        assert_eq!(w.erase(4096, 0), Err(WindowError::OutOfBounds));
    }
    assert_eq!(flash.erases, [(48 * 1024, 52 * 1024)]);
    assert_eq!(&flash.mem[48 * 1024 + 8..48 * 1024 + 12], &[1, 2, 3, 4]);
    // Nothing before the window was touched.
    assert!(flash.mem[..48 * 1024].iter().all(|&b| b == 0x5A));
}

#[test]
fn an_image_copied_into_the_tail_of_a_slot_leaves_its_head_alone() {
    // The write proof: an image at the slot's start, copied into the slot's last part.
    let bytes = good_image(10_000);
    let mut flash = FakeFlash::new(64 * 1024);
    flash.mem[..bytes.len()].copy_from_slice(&bytes);
    let tail = 32 * 1024;
    let mut writer = SlotWriter::new(32 * 1024, Sha::default());
    let mut source = [0u8; 4096];
    let mut at = 0;
    while at < bytes.len() {
        let n = source.len().min(bytes.len() - at);
        flash.read(at as u32, &mut source[..n]).unwrap();
        let mut window = Window::new(&mut flash, tail, 32 * 1024).unwrap();
        writer.write(&mut window, &source[..n]).unwrap();
        at += n;
    }
    let mut window = Window::new(&mut flash, tail, 32 * 1024).unwrap();
    let written = writer.finish(&mut window).unwrap();
    assert_eq!(&written.digest[..], &bytes[bytes.len() - DIGEST..]);
    assert_eq!(&flash.mem[..bytes.len()], &bytes[..]);
    let tail = tail as usize;
    assert_eq!(&flash.mem[tail..tail + bytes.len()], &bytes[..]);
}

#[test]
fn the_windows_last_bytes_and_sector_are_inside_it() {
    let mut flash = FakeFlash::new(64 * 1024);
    {
        let mut w = Window::new(&mut flash, 48 * 1024, 16 * 1024).unwrap();
        w.erase(12 * 1024, 16 * 1024).unwrap();
        w.write(16 * 1024 - 4, &[9, 8, 7, 6]).unwrap();
        let mut back = [0u8; 4];
        w.read(16 * 1024 - 4, &mut back).unwrap();
        assert_eq!(back, [9, 8, 7, 6]);
    }
    assert_eq!(flash.erases, [(60 * 1024, 64 * 1024)]);
    assert_eq!(&flash.mem[64 * 1024 - 4..], &[9, 8, 7, 6]);
}
