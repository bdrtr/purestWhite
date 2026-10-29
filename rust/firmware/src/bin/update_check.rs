//! The SD updater's reading side, a dry run on the device (rust/firmware/DESIGN.md, the SD
//! updater): nothing is written but the hand-back's otadata entry. It reads the C++ firmware's
//! release image the recovery mode would install from the card (`RECOVERY_FILE`), checks and hashes
//! it as it streams in, and names it among the releases the updater knows (`releases`). Then it
//! hashes the image in the C++ firmware's slot the same way and compares the two. Reports both
//! (their length, the start of their digest, their name), the time each took, whether they are the
//! same image, and how many sectors an install would write; then resets. Hands the next boot back
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
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::sdmmc::{Config as SdConfig, SdHostController};
use esp_hal::sha::Sha;
use esp_hal::time::Instant;
use purestwhite_firmware::board::{Board, CardParts};
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::images::{identify_file, identify_partition};
use purestwhite_firmware::ready;
use purestwhite_firmware::releases::{Named, RECOVERY_FILE};
use purestwhite_firmware::sd;
use purestwhite_firmware::sha::HwSha;
use purestwhite_firmware::slots::ota_slots;
use purestwhite_ota::digest::Identified;
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

/// Notes on the report that `what` failed, and why.
fn failed(report: &mut Report, what: &str, why: impl Debug) {
    let _ = writeln!(report, "{what}: {why:?}");
}

/// Notes an identified image: where it is, its length, its digest's start and its name, and the
/// time it took.
fn note(report: &mut Report, what: &str, image: &Identified, ms: u64) {
    let (len, named) = (image.checked.len, Named(&image.digest));
    let _ = writeln!(report, "{what} {len} B {ms} ms {named}");
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
    let pins = (card.clk, card.cmd, card.dat0);
    let mut volume = match sd::open_volume(&controller, pins, &mut supply, |_| rtc.rwdt.feed()) {
        Ok(volume) => volume,
        Err(e) => {
            failed(report, "card", e);
            return None;
        }
    };
    let feed = || rtc.rwdt.feed();
    match ready(identify_file(
        &mut volume,
        RECOVERY_FILE,
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
    identify_partition(storage, target, HwSha::new(sha), || rtc.rwdt.feed())
        .map_err(|e| failed(report, "slot", e))
        .ok()
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
