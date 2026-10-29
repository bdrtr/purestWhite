//! The update gate's write path, proven on the device where a mistake harms only this image's own
//! slot (rust/firmware/DESIGN.md, the SD updater). The image copies itself, from the start of the
//! slot it runs from into the last megabyte of that same slot, through the slot's own flash region
//! (which cannot write outside the slot) and the SD updater's own parts (`purestwhite-ota`). In
//! order: its length measured where it lies; its digest worked out without writing (`Identify`);
//! the sectors the copy will use written to 0x00, so that the copy can pass only if the erase of
//! each sector holding a non-zero byte really brings it back; the copy, each 4 KB sector erased,
//! written and read back
//! (`SlotWriter`); the copy read and hashed once more. The digests are the chip's SHA unit's.
//!
//! It hands the next boot back to the C++ firmware first, so any failure ends in the C++ firmware.
//! Reports the slot, the image's length, each pass's result and how long the copy took; then
//! resets. Runs in the emulator too (`emu/baremetal-write.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![allow(
    clippy::cast_possible_truncation,
    reason = "the ESP32-S3's usize is 32 bits, and a slot is at most 16 MB"
)]
#![no_main]

use aligned::{A4, Aligned};
use core::fmt::{Debug, Write as _};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_backtrace as _;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::sha::Sha;
use esp_hal::system::{Cpu, is_running};
use esp_hal::time::Instant;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::sha::HwSha;
use purestwhite_firmware::slots::ota_slots;
use purestwhite_ota::digest::{Identified, Identify};
use purestwhite_ota::image::ImageCheck;
use purestwhite_ota::slot::{SECTOR, SlotWriter};
use purestwhite_ota::window::Window;

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

/// The part of the slot the copy goes to: its last megabyte.
const WINDOW: u32 = 1024 * 1024;
/// How much is read at a time: a sector, word aligned (esp-storage copies an unaligned buffer
/// through one of its own).
const CHUNK: usize = SECTOR;
type Buf = Aligned<A4, [u8; CHUNK]>;

/// A zeroed buffer.
fn buf() -> Buf {
    Aligned([0; CHUNK])
}

/// Notes on the report that `what` failed, and why.
fn failed(report: &mut Report, what: &str, why: impl Debug) {
    let _ = writeln!(report, "{what}: {why:?}");
}

/// Notes a flash failure at `at`, and whether the second core runs (esp-storage refuses to
/// write beside it).
fn failed_at(report: &mut Report, what: &str, at: usize, why: impl Debug) {
    let core = if is_running(Cpu::AppCpu) { "on" } else { "off" };
    let _ = writeln!(report, "{what} at {at:x}: {why:?}, core 1 {core}");
}

/// The window on the slot's last megabyte, noted when there is none (a slot under 1 MB).
fn window<'f, F: NorFlash>(flash: &'f mut F, report: &mut Report) -> Option<Window<'f, F>> {
    let tail = (flash.capacity() as u32).checked_sub(WINDOW);
    let window = tail.and_then(|tail| Window::new(flash, tail, WINDOW));
    if window.is_none() {
        failed(report, "window", "none");
    }
    window
}

/// The length of the image at the start of `flash`, which must be at most `limit`.
fn measure<F: ReadNorFlash>(flash: &mut F, limit: usize, report: &mut Report) -> Option<usize> {
    let mut check = ImageCheck::new(limit);
    let mut buf = buf();
    let mut at = 0;
    while !check.is_done() {
        if let Err(e) = flash.read(at as u32, &mut *buf) {
            failed_at(report, "measure", at, e);
            return None;
        }
        if let Err(e) = check.feed_some(&*buf) {
            failed(report, "measure", e);
            return None;
        }
        at += CHUNK;
    }
    check
        .finish()
        .map_err(|e| failed(report, "measure", e))
        .ok()
        .map(|c| c.len)
}

/// The first `len` bytes of `flash` checked and hashed, nothing written.
fn identify<F: ReadNorFlash>(
    flash: &mut F,
    len: usize,
    sha: &mut Sha<'_>,
    report: &mut Report,
) -> Option<Identified> {
    let mut identify = Identify::new(len, HwSha::new(sha));
    let mut buf = buf();
    for at in (0..len).step_by(CHUNK) {
        let piece = buf.get_mut(..CHUNK.min(len - at)).unwrap_or_default();
        if let Err(e) = flash.read(at as u32, piece) {
            failed_at(report, "identify", at, e);
            return None;
        }
        if let Err(e) = identify.feed(piece) {
            failed(report, "identify", e);
            return None;
        }
    }
    identify
        .finish()
        .map_err(|e| failed(report, "identify", e))
        .ok()
}

/// Writes 0x00 over the first `sectors` sectors of `window` and reads them back: a NOR write can
/// always clear bits, and only an erase sets them again.
fn clear<F: NorFlash>(window: &mut F, sectors: usize, report: &mut Report) -> Option<()> {
    let zeros = buf();
    let mut back = buf();
    for at in (0..sectors * SECTOR).step_by(SECTOR) {
        if let Err(e) = window.write(at as u32, &*zeros) {
            failed_at(report, "clear", at, e);
            return None;
        }
        if let Err(e) = window.read(at as u32, &mut *back) {
            failed_at(report, "clear", at, e);
            return None;
        }
        if back.iter().any(|&b| b != 0) {
            failed_at(report, "clear", at, "not 0x00");
            return None;
        }
    }
    Some(())
}

/// The first `len` bytes of `flash` copied into the window on the slot's last megabyte.
fn copy<F: NorFlash>(
    flash: &mut F,
    len: usize,
    sha: &mut Sha<'_>,
    rtc: &mut Rtc<'_>,
    report: &mut Report,
) -> Option<Identified> {
    let mut writer = SlotWriter::new(WINDOW as usize, HwSha::new(sha));
    let mut buf = buf();
    for at in (0..len).step_by(CHUNK) {
        let piece = buf.get_mut(..CHUNK.min(len - at)).unwrap_or_default();
        if let Err(e) = flash.read(at as u32, piece) {
            failed_at(report, "copy read", at, e);
            return None;
        }
        if let Err(e) = writer.write(&mut window(flash, report)?, piece) {
            failed_at(report, "copy", at, e);
            return None;
        }
        rtc.rwdt.feed();
    }
    let finished = writer.finish(&mut window(flash, report)?);
    finished.map_err(|e| failed(report, "copy", e)).ok()
}

/// The passes over the slot's flash, each noted: measure, identify, clear, copy, identify the
/// copy.
fn prove<F: NorFlash>(flash: &mut F, sha: &mut Sha<'_>, rtc: &mut Rtc<'_>, report: &mut Report) {
    // The image must fit the window, and end before it whatever the slot's size.
    let tail = (flash.capacity() as u32).saturating_sub(WINDOW);
    let Some(len) = measure(flash, tail.min(WINDOW) as usize, report) else {
        return;
    };
    let first = identify(flash, len, sha, report);
    let Some(first) = first else {
        return;
    };
    let _ = writeln!(report, "image {len} bytes, digest ok");
    report.save();
    let sectors = len.div_ceil(SECTOR);
    let Some(mut tail) = window(flash, report) else {
        return;
    };
    if clear(&mut tail, sectors, report).is_none() {
        return;
    }
    let _ = writeln!(report, "cleared {sectors} sectors");
    report.save();
    let start = Instant::now();
    let Some(second) = copy(flash, len, sha, rtc, report) else {
        return;
    };
    let ms = start.elapsed().as_millis();
    let _ = writeln!(report, "copy: {sectors} sectors, {ms} ms");
    report.save();
    let Some(third) = window(flash, report).and_then(|mut w| identify(&mut w, len, sha, report))
    else {
        return;
    };
    let same = first.digest == second.digest && second.digest == third.digest;
    let _ = writeln!(
        report,
        "read back: {}",
        if same {
            "same digest"
        } else {
            "DIGESTS DIFFER"
        }
    );
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
    } = start("purestWhite write check", board.boot);
    let mut sha = Sha::new(board.sha);
    match ota_slots(&mut storage) {
        Ok(slots) => {
            let slot = slots.booted;
            let _ = writeln!(report, "slot {:x}+{:x}", slot.offset(), slot.len());
            let mut region = slot.as_flash_region(&mut storage);
            match region.as_nor_flash() {
                Ok(mut flash) => prove(&mut flash, &mut sha, &mut rtc, &mut report),
                Err(e) => failed(&mut report, "region", e),
            }
        }
        Err(e) => failed(&mut report, "slot", e),
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
