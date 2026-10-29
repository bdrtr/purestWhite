//! The scratch partition, read only. Before the update gate's write path is proven on the flash
//! (rust/firmware/DESIGN.md, the SD updater), it must be shown that nothing uses the partition it
//! is proven on: `spiffs` in partitions.csv (0xC90000, 3456 KB), which the C++ firmware's sources
//! never open. Hands the next boot back to the C++ firmware first, finds the partition in the
//! partition table, reads it whole sector by sector, and reports how many 4 KB sectors are erased
//! (all 0xFF), the first sector that is not and its first bytes (a file system's signature, if
//! one was ever made there), and a FNV-1a hash of it all; then resets. Writes nothing. Runs in
//! the emulator too (`emu/baremetal-scratch.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::{
    FlashStorage, PARTITION_TABLE_MAX_LEN, read_partition_table,
};
use esp_hal::delay::Delay;
use esp_hal::rtc_cntl::Rtc;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};

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

/// A flash sector.
const SECTOR: usize = 4096;
/// The partition's type and subtype: data, SPIFFS.
const DATA: u8 = 1;
const SPIFFS: u8 = 0x82;
/// FNV-1a's 32-bit offset basis and prime.
const FNV_BASIS: u32 = 0x811C_9DC5;
const FNV_PRIME: u32 = 0x0100_0193;

/// What the partition holds.
struct Survey {
    sectors: u32,
    erased: u32,
    /// The first sector that is not erased: where, and its first bytes.
    first_used: Option<(u32, [u8; 8])>,
    fnv: u32,
}

impl Survey {
    fn new() -> Self {
        Survey {
            sectors: 0,
            erased: 0,
            first_used: None,
            fnv: FNV_BASIS,
        }
    }

    fn add(&mut self, at: u32, sector: &[u8]) {
        self.sectors += 1;
        if sector.iter().all(|&b| b == 0xFF) {
            self.erased += 1;
        } else if self.first_used.is_none() {
            let mut head = [0u8; 8];
            if let Some(bytes) = sector.get(..8) {
                head.copy_from_slice(bytes);
            }
            self.first_used = Some((at, head));
        }
        for &b in sector {
            self.fnv = (self.fnv ^ u32::from(b)).wrapping_mul(FNV_PRIME);
        }
    }
}

/// Where the partition table is.
const TABLE_AT: u32 = 0x8000;

/// Where the scratch partition is and how long, from the partition table; when the table cannot
/// be read, why, in the report (on the device the first image failed here, the emulator not).
fn find(storage: &mut FlashStorage<'_>, report: &mut Report) -> Option<(u32, u32)> {
    let mut table = [0u8; PARTITION_TABLE_MAX_LEN];
    if let Err(e) = read_partition_table(storage, &mut table) {
        // Once more, a moment later: whether the failure lasts.
        let _ = writeln!(report, "table first: {e:?}");
        Delay::new().delay_millis(10);
    }
    let found = match read_partition_table(storage, &mut table) {
        Ok(table) => table
            .iter()
            .find(|p| p.raw_type() == DATA && p.raw_subtype() == SPIFFS)
            .map(|p| (p.offset(), p.len())),
        Err(e) => {
            let _ = writeln!(report, "table: {e:?}");
            note_raw_read(storage, report);
            return None;
        }
    };
    if found.is_none() {
        let _ = writeln!(report, "scratch: no spiffs partition");
    }
    found
}

/// The flash's own answer for the table's first bytes, and its size as the bootloader set it.
fn note_raw_read(storage: &mut FlashStorage<'_>, report: &mut Report) {
    let mut head = [0u8; 8];
    let read = storage.read(TABLE_AT, &mut head);
    let _ = writeln!(
        report,
        "raw {read:?} {head:02x?}, flash {} KB",
        storage.capacity() / 1024
    );
}

/// Reads `len` bytes at `offset` sector by sector, feeding the watchdog between them.
fn survey(
    storage: &mut FlashStorage<'_>,
    rtc: &mut Rtc<'_>,
    offset: u32,
    len: u32,
) -> Result<Survey, &'static str> {
    let mut survey = Survey::new();
    let mut sector = [0u8; SECTOR];
    for at in (offset..offset + len).step_by(SECTOR) {
        storage.read(at, &mut sector).map_err(|_| "read failed")?;
        survey.add(at, &sector);
        rtc.rwdt.feed();
    }
    Ok(survey)
}

/// The report's lines on the partition.
fn note(report: &mut Report, offset: u32, len: u32, s: &Survey) {
    let _ = writeln!(report, "scratch {offset:#x}, {} KB", len / 1024);
    let _ = writeln!(report, "erased {} of {} sectors", s.erased, s.sectors);
    match s.first_used {
        None => {
            let _ = writeln!(report, "first used: none");
        }
        Some((at, head)) => {
            let _ = writeln!(report, "first used {at:#x}: {head:02x?}");
        }
    }
    let _ = writeln!(report, "fnv {:08x}", s.fnv);
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
    } = start("purestWhite scratch check", board.boot);
    let _ = writeln!(report, "flash {} KB", storage.capacity() / 1024);
    if let Some((offset, len)) = find(&mut storage, &mut report) {
        match survey(&mut storage, &mut rtc, offset, len) {
            Ok(s) => note(&mut report, offset, len, &s),
            Err(e) => {
                let _ = writeln!(report, "scratch: {e}");
            }
        }
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
