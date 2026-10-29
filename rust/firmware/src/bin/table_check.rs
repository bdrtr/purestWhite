//! The device's partition table, read only. The device keeps the table it left the factory with:
//! the C++ firmware's SD and OTA updates write app slots and otadata, never the table, so it need
//! not be `partitions.csv` (scratch-check found `spiffs` at 0xFD0000, 80 KB, in use, where
//! `partitions.csv` puts 3456 KB at 0xC90000). Hands the next boot back to the C++ firmware
//! first, then reports each partition (its label, type/subtype, offset and size, in hex) and the
//! slot it was booted from, and resets. Writes nothing. Runs in the emulator too
//! (`emu/baremetal-table.txt`).
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_bootloader_esp_idf::partitions::{
    FlashStorage, PARTITION_TABLE_MAX_LEN, PartitionEntry, read_partition_table,
};
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

/// The most of a label the report shows: it has 248 bytes for every line.
const LABEL: usize = 10;

/// `p`'s label, its printable ASCII only (`?` for anything else), cut to `LABEL`.
fn write_label(report: &mut Report, p: &PartitionEntry) {
    for &b in p
        .label()
        .iter()
        .take_while(|&&b| b != 0 && b != 0xFF)
        .take(LABEL)
    {
        let c = if b.is_ascii_graphic() {
            char::from(b)
        } else {
            '?'
        };
        let _ = report.write_char(c);
    }
}

/// One line per partition, then the slot booted from.
fn note_table(storage: &mut FlashStorage<'_>, report: &mut Report) {
    let mut buf = [0u8; PARTITION_TABLE_MAX_LEN];
    let table = match read_partition_table(storage, &mut buf) {
        Ok(table) => table,
        Err(e) => {
            let _ = writeln!(report, "table: {e:?}");
            return;
        }
    };
    for p in table.iter() {
        write_label(report, &p);
        let _ = writeln!(
            report,
            " {}/{:02x} {:x} {:x}",
            p.raw_type(),
            p.raw_subtype(),
            p.offset(),
            p.len()
        );
    }
    let _ = write!(report, "run ");
    match table.booted_partition() {
        Ok(Some(p)) => write_label(report, &p),
        _ => {
            let _ = write!(report, "?");
        }
    }
    let _ = writeln!(report);
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    // Short: the report has 248 bytes, and the table needs most of them.
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        flash: mut storage,
        ..
    } = start("purestWhite table", board.boot);
    let _ = writeln!(report, "flash {} KB", storage.capacity() / 1024);
    note_table(&mut storage, &mut report);
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}
