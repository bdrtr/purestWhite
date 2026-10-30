//! The device's bootloader, read only, and a test of whether it checks an image before it boots it
//! (rust/firmware/DESIGN.md, the SD updater): the SD updater's repair after a write cut short rests
//! on that check, which only the C++ build's bootloader has shown, in the emulator; the device
//! keeps the bootloader it left the factory with.
//!
//! First it reads the bootloader at the flash's start: its length, whether its appended digest
//! holds, the start of that digest, the ESP-IDF version and build time its descriptor names, and
//! whether it is the C++ build's (`CPP_BUILD_BOOTLOADER`). Then the test, only when it is safe:
//! the bootloader started this image by selection, its digest is not a C++ release's, and the C++
//! firmware's slot holds a known release. It clears its own appended digest (32 bytes written to
//! 0x00 at its image's end, read back), selects its own slot for the next boot, valid
//! (`test_image::select_this_image`), and resets. A bootloader that checks images refuses it and
//! boots the C++ firmware, which saves this report as it is. One that does not boots this image
//! again: it hands the next boot back first, as every image, finds its digest no longer holds and
//! notes `ran again: ...`, then resets into the C++ firmware. Either way nothing but this image's
//! own slot and otadata is written.
//!
//! The reset is a software one: a bootloader set to skip the check only at power-on passes it too.
//! The entry stays selected, valid (a rollback bootloader leaves it), until the next update writes
//! this slot, for a later test with a boot known to be a power-on. Runs in the emulator too
//! (`emu/baremetal-boot.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "the ESP32-S3's usize is 32 bits, and a slot is at most 16 MB"
)]
#![no_main]

use aligned::{A4, Aligned};
use embedded_storage::nor_flash::NorFlash;
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::{Error as FlashError, FlashStorage, PartitionEntry};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::sha::Sha;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::images::identify_partition;
use purestwhite_firmware::sha::HwSha;
use purestwhite_firmware::slots::{OtaSlots, ota_slots};
use purestwhite_firmware::test_image::select_this_image;
use purestwhite_ota::digest::{HeldBack, Identified};
use purestwhite_ota::image::{DIGEST, ImageCheck};
use purestwhite_ota::slot::SECTOR;
use purestwhite_updater::images::SlotError;
use purestwhite_updater::releases::{Named, X4_PRO, release};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset. The next boot is the C++ firmware's: otadata
/// selects it, or, once the test has selected this image, the bootloader refuses this image (or,
/// if it does not check, boots it again, and it hands back at once).
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
    esp_hal::system::software_reset()
}

const TITLE: &str = "purestWhite boot check";
/// Where the bootloader may lie: from the flash's start to the partition table.
const BOOTLOADER_ROOM: usize = 0x8000;
/// The bootloader's descriptor (ESP-IDF's `esp_bootloader_desc_t`, at its first segment's start):
/// its magic byte, then the ESP-IDF version at 8 and the build time at 40.
const DESC_AT: u32 = 0x20;
const DESC_MAGIC: u8 = 0x50;
/// The appended digest of the C++ build's bootloader (`.pio/build/x4pro/bootloader.bin`, ESP-IDF
/// v5.5.5), which the emulator runs.
const CPP_BUILD_BOOTLOADER: [u8; DIGEST] = [
    0x8e, 0x15, 0xb5, 0x9a, 0x3c, 0x97, 0xe1, 0x9e, 0x31, 0xee, 0x3e, 0xe3, 0xf5, 0xea, 0x89, 0x5a,
    0x9a, 0x68, 0x39, 0x29, 0x64, 0xa3, 0x34, 0x3a, 0x6b, 0xfb, 0xf1, 0x0c, 0x3a, 0x2e, 0x7c, 0xb9,
];

type Buf = Aligned<A4, [u8; SECTOR]>;

/// A digest's first four bytes, for the report.
struct Start<'a>(&'a [u8; DIGEST]);

impl core::fmt::Display for Start<'_> {
    fn fmt(&self, out: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0
            .iter()
            .take(4)
            .try_for_each(|byte| write!(out, "{byte:02x}"))
    }
}

/// The bootloader at the flash's start, checked and hashed as it is read: its length, and its
/// digest when the one appended holds (`None` when not).
fn measure_bootloader(
    storage: &mut FlashStorage<'_>,
    sha: &mut Sha<'_>,
) -> Result<(usize, Option<[u8; DIGEST]>), &'static str> {
    let mut check = ImageCheck::new(BOOTLOADER_ROOM);
    let mut held = HeldBack::new(HwSha::new(sha));
    let mut buf: Buf = Aligned([0; SECTOR]);
    let mut at = 0;
    while !check.is_done() && at < BOOTLOADER_ROOM {
        storage.read(at as u32, &mut *buf).map_err(|_| "read")?;
        let used = check.feed_some(&*buf).map_err(|_| "image")?;
        held.update(buf.get(..used).unwrap_or_default());
        at += SECTOR;
    }
    let len = check.len_once_done().ok_or("truncated")?;
    Ok((len, held.finish().ok()))
}

/// The text of a NUL-padded field, or `?`.
fn text(field: &[u8]) -> &str {
    let end = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    core::str::from_utf8(field.get(..end).unwrap_or_default()).unwrap_or("?")
}

/// The bootloader noted: its length and digest, its descriptor, and whether it is the C++ build's.
fn bootloader(storage: &mut FlashStorage<'_>, sha: &mut Sha<'_>, report: &mut Report) {
    match measure_bootloader(storage, sha) {
        Ok((len, Some(digest))) => {
            report.note(format_args!("bootloader {len} B {}", Start(&digest)));
            let same = if digest == CPP_BUILD_BOOTLOADER {
                "same"
            } else {
                "other"
            };
            report.note(format_args!("C++ build's: {same}"));
        }
        Ok((len, None)) => report.note(format_args!("bootloader {len} B, digest BAD")),
        Err(e) => report.note(format_args!("bootloader: {e}")),
    }
    let mut desc = [0u8; 64];
    if storage.read(DESC_AT, &mut desc).is_err() || desc.first() != Some(&DESC_MAGIC) {
        return report.note(format_args!("no descriptor"));
    }
    let (idf, built) = (
        text(desc.get(8..40).unwrap_or_default()),
        text(desc.get(40..64).unwrap_or_default()),
    );
    report.note(format_args!("built {idf} {built}"));
}

/// Writes 0x00 over this image's appended digest at `at` in `flash` and reads it back: whether it
/// took.
fn clear_digest<F: NorFlash>(flash: &mut F, at: u32) -> bool {
    let zeros: Aligned<A4, [u8; DIGEST]> = Aligned([0; DIGEST]);
    let mut back: Aligned<A4, [u8; DIGEST]> = Aligned([0xFF; DIGEST]);
    flash.write(at, &*zeros).is_ok()
        && flash.read(at, &mut *back).is_ok()
        && back.iter().all(|&b| b == 0)
}

/// What the run lends each step.
struct Run<'a, 'd> {
    storage: &'a mut FlashStorage<'d>,
    sha: &'a mut Sha<'d>,
    rtc: &'a mut Rtc<'d>,
    report: &'a mut Report,
}

impl Run<'_, '_> {
    /// The image in `slot`, identified where it lies.
    fn identify(&mut self, slot: PartitionEntry) -> Result<Identified, SlotError<FlashError>> {
        let rtc = &mut *self.rtc;
        identify_partition(self.storage, slot, HwSha::new(self.sha), || {
            rtc.rwdt.feed();
        })
    }

    /// Whether the test may break this image (`own`): the bootloader started it by selection, its
    /// digest is not a C++ release's (so `slots.booted` cannot be the C++ firmware's slot), and
    /// the C++ firmware's slot holds a known release, which the bootloader then boots. Noted.
    fn safe(&mut self, slots: &OtaSlots, own: &Identified, fell_back: bool) -> bool {
        if fell_back {
            self.report
                .note(format_args!("skipped: the bootloader fell back"));
            return false;
        }
        if release(&X4_PRO, &own.digest).is_some() {
            self.report
                .note(format_args!("skipped: own image is a release"));
            return false;
        }
        match self.identify(slots.other) {
            Ok(cpp) if release(&X4_PRO, &cpp.digest).is_some() => {
                let (len, named) = (cpp.checked.len, Named(&cpp.digest, &X4_PRO));
                self.report.note(format_args!("C++ slot {len} B {named}"));
                true
            }
            Ok(cpp) => {
                let named = Named(&cpp.digest, &X4_PRO);
                self.report.note(format_args!("skipped: C++ slot {named}"));
                false
            }
            Err(e) => {
                self.report.note(format_args!("skipped: C++ slot {e:?}"));
                false
            }
        }
    }

    /// Clears this image's appended digest (at `at` in `slots.booted`) and selects its slot, noted:
    /// whether both took.
    fn set(&mut self, slots: &OtaSlots, at: u32) -> bool {
        let mut region = slots.booted.as_flash_region(self.storage);
        let cleared = region
            .as_nor_flash()
            .is_ok_and(|mut flash| clear_digest(&mut flash, at));
        if !cleared {
            self.report.note(format_args!("own digest: not cleared"));
            return false;
        }
        match select_this_image(self.storage) {
            Ok(()) => {
                self.report
                    .note(format_args!("digest cleared at {at:x}, selected: reset"));
                true
            }
            Err(e) => {
                self.report.note(format_args!("select: {e}"));
                false
            }
        }
    }

    /// The test, noted: set when it is safe (`safe`); or, when this image's digest no longer
    /// holds, the bootloader started it so (`ran again`). Whether the test is set: the reset that
    /// follows is the test, and no `done` is noted then.
    fn test_own_image(&mut self, slots: &OtaSlots, fell_back: bool) -> bool {
        match self.identify(slots.booted) {
            Ok(own) => {
                self.safe(slots, &own, fell_back) && self.set(slots, own.checked.hash_at as u32)
            }
            Err(SlotError::Digest) => {
                self.report
                    .note(format_args!("ran again: booted with its digest cleared"));
                false
            }
            Err(e) => {
                self.report.note(format_args!("own image: {e:?}"));
                false
            }
        }
    }
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        found,
        flash: mut storage,
        ..
    } = start(TITLE, board.boot);
    // `note` prints each line it adds; these are `start`'s.
    esp_println::println!("{}", report.as_str().trim_end());
    let mut sha = Sha::new(board.sha);
    bootloader(&mut storage, &mut sha, &mut report);
    let tested = match ota_slots(&mut storage) {
        Ok(slots) => Run {
            storage: &mut storage,
            sha: &mut sha,
            rtc: &mut rtc,
            report: &mut report,
        }
        .test_own_image(&slots, found.fell_back()),
        Err(e) => {
            report.note(format_args!("slot: {e}"));
            false
        }
    };
    if !tested {
        report.note(format_args!("done"));
    }
    esp_hal::system::software_reset()
}
