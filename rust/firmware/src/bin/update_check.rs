//! The SD updater's reading side, a dry run on the device (rust/firmware/DESIGN.md, the SD
//! updater): nothing is written but the hand-back's otadata entry. It reads the C++ firmware's
//! release image the recovery mode would install from the card (`FILE`), checks and hashes it as
//! it streams in, and names it among the releases this image knows (`KNOWN`). Then it hashes the
//! image in the C++ firmware's slot the same way and compares the two. Reports both (their
//! length, the start of their digest, their name), the time each took, whether they are the same
//! image, and how many sectors an install would write; then resets. Hands the next boot back
//! first, as every test image. Runs in the emulator too (`emu/baremetal-update.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "the ESP32-S3's usize is 32 bits, and a slot is at most 16 MB"
)]
#![no_main]

use core::fmt::{Debug, Write as _};
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::{FlashStorage, PartitionEntry};
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::sdmmc::{Config as SdConfig, SdHostController, SlotConfig};
use esp_hal::sha::Sha;
use esp_hal::time::Instant;
use purestwhite_fat::Volume;
use purestwhite_firmware::board::{Board, CardParts};
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::images::{identify_file, identify_slot};
use purestwhite_firmware::ready;
use purestwhite_firmware::sd::{self, BlockingBus, Card, CardBlocks, SpinDelay};
use purestwhite_firmware::sha::HwSha;
use purestwhite_firmware::slots::ota_slots;
use purestwhite_ota::digest::Identified;
use purestwhite_ota::image::DIGEST;
use purestwhite_ota::slot::SECTOR;

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIC\n");
    esp_hal::system::software_reset()
}

/// The release image the recovery mode installs: one fixed name in the card's root.
const FILE: &str = "purestwhite-recovery.bin";

/// The X4 Pro's C++ releases this image knows, by the SHA-256 appended to each (the archive's
/// `purestWhite-next-*.bin`). During phase 2 the recovery mode installs no other file.
const KNOWN: [(&str, [u8; DIGEST]); KNOWN_6] = [
    (
        "next 0.0.1",
        [
            0x8d, 0xa9, 0x01, 0x55, 0xbe, 0xa4, 0x0a, 0xfa, 0x11, 0xaa, 0x83, 0x9a, 0x14, 0xb1,
            0xd5, 0x19, 0x2f, 0x43, 0x88, 0xd1, 0x6b, 0x9e, 0x61, 0x14, 0x49, 0x2a, 0x1f, 0xed,
            0x1c, 0x0f, 0x05, 0x54,
        ],
    ),
    (
        "next 0.0.3",
        [
            0x74, 0x34, 0x2d, 0x77, 0x99, 0x1e, 0xa2, 0x01, 0x24, 0x8a, 0x60, 0x0f, 0x8f, 0x7d,
            0x54, 0x22, 0x2e, 0x89, 0xa0, 0x21, 0xc5, 0x5d, 0xca, 0x16, 0x4a, 0xac, 0x2c, 0x25,
            0x9a, 0x0d, 0x49, 0xf8,
        ],
    ),
    (
        "next rapor",
        [
            0x71, 0x86, 0x97, 0xdb, 0xcc, 0xb7, 0x59, 0x7e, 0xfd, 0x07, 0x2a, 0x29, 0xad, 0xb3,
            0x2d, 0x0c, 0xaf, 0xde, 0x39, 0x8f, 0x5f, 0xf7, 0xc8, 0xae, 0xd8, 0x4c, 0xc5, 0xd1,
            0xfb, 0x65, 0xfb, 0x92,
        ],
    ),
    (
        "next a8764dbb",
        [
            0xc7, 0x46, 0xbe, 0xbf, 0x3f, 0x92, 0x9f, 0x51, 0x56, 0x60, 0x17, 0x53, 0xaf, 0xaf,
            0xc8, 0x47, 0x0c, 0xa6, 0xd0, 0x51, 0xc6, 0x58, 0x5e, 0x21, 0x40, 0xae, 0xc4, 0x12,
            0x05, 0x3d, 0x6d, 0x9f,
        ],
    ),
    (
        "next 8a55083e",
        [
            0xba, 0x33, 0xb1, 0x57, 0x3b, 0x25, 0x05, 0x39, 0x41, 0x61, 0xaf, 0x00, 0x27, 0x84,
            0xb3, 0x97, 0xba, 0xcb, 0x2b, 0x0c, 0x67, 0x0e, 0x39, 0xef, 0x05, 0xce, 0xc9, 0xee,
            0xf0, 0xe4, 0x6a, 0xcc,
        ],
    ),
    (
        "next uyanma",
        [
            0xaa, 0xf4, 0xfa, 0xac, 0x0b, 0x13, 0xa3, 0x8e, 0xd8, 0x9e, 0xb1, 0x46, 0x81, 0xee,
            0x69, 0xaf, 0x28, 0x70, 0x96, 0x6c, 0x2e, 0x18, 0xae, 0x83, 0x03, 0x1c, 0x06, 0xc8,
            0x0e, 0xa4, 0x02, 0xbb,
        ],
    ),
];
const KNOWN_6: usize = 6;

/// The release `digest` is, or `unknown`.
fn name(digest: &[u8; DIGEST]) -> &'static str {
    KNOWN
        .iter()
        .find(|(_, known)| known == digest)
        .map_or("unknown", |(name, _)| name)
}

/// Notes on the report that `what` failed, and why.
fn failed(report: &mut Report, what: &str, why: impl Debug) {
    let _ = writeln!(report, "{what}: {why:?}");
}

/// Notes an identified image: where it is, its length, its digest's start and its name, and the
/// time it took.
fn note(report: &mut Report, what: &str, image: &Identified, ms: u64) {
    let [a, b, c, d, ..] = image.digest;
    let _ = writeln!(
        report,
        "{what} {} B {ms} ms {a:02x}{b:02x}{c:02x}{d:02x} {}",
        image.checked.len,
        name(&image.digest)
    );
    report.save();
}

/// The release image on the card, at most `limit` bytes (the slot it would go to). The card is
/// mounted as the C++ firmware mounts it, through the blocking bus.
fn card_image(
    card: CardParts,
    limit: usize,
    sha: &mut Sha<'_>,
    rtc: &mut Rtc<'_>,
    report: &mut Report,
) -> Option<Identified> {
    // Off until its first try (`sd::mount`).
    let mut supply = Output::new(card.supply, Level::High, OutputConfig::default());
    let controller = SdHostController::new(card.sdhost, SdConfig::default()).ok()?;
    let slot = controller
        .slot::<1>(SlotConfig::default())
        .ok()?
        .with_clk(card.clk)
        .with_cmd(card.cmd)
        .with_data0(card.dat0);
    let mut sd: Card<'_> = Card::new_uninit(BlockingBus::new(slot), SpinDelay(Delay::new()));
    let Some(_) = sd::mount(&mut sd, &mut supply, |_| rtc.rwdt.feed()) else {
        failed(report, "card", "not mounted");
        return None;
    };
    let mut volume = match ready(Volume::mount(CardBlocks::new(sd))) {
        Some(Ok(volume)) => volume,
        other => {
            failed(report, "volume", other.map(Result::err));
            return None;
        }
    };
    let feed = || rtc.rwdt.feed();
    match ready(identify_file(
        &mut volume,
        FILE,
        limit,
        HwSha::new(sha),
        feed,
    )) {
        Some(Ok(image)) => Some(image),
        Some(Err(e)) => {
            failed(report, "card", e);
            None
        }
        None => {
            failed(report, "card", "not ready");
            None
        }
    }
}

/// The image in `target` (the C++ firmware's slot), measured and hashed where it lies.
fn slot_image(
    storage: &mut FlashStorage<'_>,
    target: PartitionEntry,
    sha: &mut Sha<'_>,
    rtc: &mut Rtc<'_>,
    report: &mut Report,
) -> Option<Identified> {
    let mut region = target.as_flash_region(storage);
    match region.as_nor_flash() {
        Ok(mut flash) => identify_slot(&mut flash, HwSha::new(sha), || rtc.rwdt.feed())
            .map_err(|e| failed(report, "slot", e))
            .ok(),
        Err(e) => {
            failed(report, "region", e);
            None
        }
    }
}

/// The card's release image and the C++ firmware's slot, each identified, then compared.
fn compare(
    storage: &mut FlashStorage<'_>,
    card: CardParts,
    sha: &mut Sha<'_>,
    rtc: &mut Rtc<'_>,
    report: &mut Report,
) {
    let target = match ota_slots(storage) {
        Ok(slots) => slots.other,
        Err(e) => return failed(report, "slot", e),
    };
    let start = Instant::now();
    let Some(file) = card_image(card, target.len() as usize, sha, rtc, report) else {
        return;
    };
    note(report, "card", &file, start.elapsed().as_millis());
    let start = Instant::now();
    let Some(slot) = slot_image(storage, target, sha, rtc, report) else {
        return;
    };
    note(report, "slot", &slot, start.elapsed().as_millis());
    let same = if slot.digest == file.digest {
        "yes"
    } else {
        "no"
    };
    let sectors = file.checked.len.div_ceil(SECTOR);
    let _ = writeln!(report, "same: {same}, would write {sectors} sectors");
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        flash: mut storage,
        ..
    } = start("purestWhite update check", board.boot);
    let mut sha = Sha::new(board.sha);
    compare(&mut storage, board.card, &mut sha, &mut rtc, &mut report);
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
