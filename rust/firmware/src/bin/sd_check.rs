//! The X4 Pro's SD card from the bare-metal firmware, read only: hands the next boot back to the
//! C++ firmware, mounts the card as the SDK does (see `purestwhite_firmware::sd`), opens its FAT
//! volume, counts the root directory's entries, reads a small file the C++ firmware writes into
//! `.crosspoint`, and reads this image's own file twice, one block per card command and then many, timing
//! both and hashing what came; then reports and resets. Every write is refused below the file
//! system, so the card is left as it was. Without esp-rtos, so it runs in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController, SlotConfig};
use esp_hal::time::Instant;
use purestwhite_fat::{Error, File, Kind, Name, Volume};
use purestwhite_firmware::board::Board;
use purestwhite_firmware::ready;
use purestwhite_firmware::sd::{self, BlockingBus, Card, CardBlocks, SpinDelay};
use purestwhite_firmware::test_image::{Report, Started, start};

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

/// The card as the file system reads it here.
type Blocks<'d> = CardBlocks<BlockingBus<'d>, SpinDelay>;

/// The directory the C++ firmware writes into, and the files in it read, the first found.
const DIR: &str = ".crosspoint";
const READ: [&str; 2] = ["bootloader.txt", "state.json"];
/// This image's file, read whole: its name on the owner's card, and in the emulator's.
const IMAGE: [&str; 2] = ["purestwhite-baremetal-sd.bin", "sd-check.bin"];
/// FNV-1a's 32-bit offset basis and prime.
const FNV_BASIS: u32 = 0x811C_9DC5;
const FNV_PRIME: u32 = 0x0100_0193;

/// Saves the report with its last line and resets into the C++ firmware.
fn finish(report: &mut Report) -> ! {
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
}

/// Why a call gave no value: its error, or that its future was not ready at once.
struct Why<'a, T, E>(&'a Option<Result<T, E>>);

impl<T, E: core::fmt::Debug> core::fmt::Display for Why<'_, T, E> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(Err(e)) => write!(f, "{e:?}"),
            Some(Ok(_)) => f.write_str("?"),
            None => f.write_str("not ready"),
        }
    }
}

/// `text` cut to at most `max` bytes, at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

#[esp_hal::main]
fn main() -> ! {
    let board = Board::take(esp_hal::init(esp_hal::Config::default()));
    let Started {
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal SD check", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // The card off until its first try.
    let mut enable = Output::new(board.card.supply, Level::High, OutputConfig::default());
    let Ok(controller) = SdHostController::new(board.card.sdhost, SdConfig::default()) else {
        let _ = writeln!(report, "sd: no controller");
        finish(&mut report)
    };
    let Ok(slot) = controller.slot::<1>(SlotConfig::default()) else {
        let _ = writeln!(report, "sd: no slot");
        finish(&mut report)
    };
    let slot = slot
        .with_clk(board.card.clk)
        .with_cmd(board.card.cmd)
        .with_data0(board.card.dat0);
    let mut card: Card<'_> = Card::new_uninit(BlockingBus::new(slot), SpinDelay(Delay::new()));
    // A word per try, saved before it starts: a try the watchdog cuts short still shows.
    let _ = write!(report, "sd: try");
    let mounted = sd::mount(&mut card, &mut enable, |attempt| {
        let _ = write!(report, " {attempt}");
        report.save();
    });
    if mounted.is_none() {
        let _ = writeln!(report, ", not mounted");
        finish(&mut report)
    }
    let card = CardBlocks::new(card);
    let cmd23 = if card.takes_cmd23() { "yes" } else { "no" };
    let _ = writeln!(report, ", ok, {} MB, cmd23 {cmd23}", card.blocks() / 2048);
    report.save();

    browse(card, &mut report)
}

/// Opens the FAT volume, counts the root's entries, reads a file in `DIR` and times reading this
/// image, noting each in the report; then finishes.
fn browse(card: Blocks<'_>, report: &mut Report) -> ! {
    let mut volume = match ready(Volume::mount(card)) {
        Some(Ok(v)) => v,
        e => {
            let _ = writeln!(report, "fat: failed ({})", Why(&e));
            finish(report)
        }
    };
    let kind = match volume.kind() {
        Kind::Fat16 => "FAT16",
        Kind::Fat32 => "FAT32",
    };
    let _ = writeln!(report, "fat: {kind}, cluster {} B", volume.cluster_bytes());
    report.save();

    let root = volume.root();
    let mut cursor = volume.cursor(root);
    let mut name = Name::new();
    let mut total = 0u32;
    loop {
        match ready(volume.next(&mut cursor, &mut name)) {
            Some(Ok(Some(_))) => total += 1,
            Some(Ok(None)) => break,
            e => {
                let _ = writeln!(
                    report,
                    "root: read failed after {total} entries ({})",
                    Why(&e)
                );
                finish(report)
            }
        }
    }
    let _ = writeln!(report, "root: {total} entries");
    report.save();

    read_state(&mut volume, report);
    report.save();
    time_image(&mut volume, report);
    finish(report)
}

/// Reads the start of the first of `READ` in `DIR`, a long-named directory, found by long names;
/// any error but a missing directory or file ends the test.
fn read_state(volume: &mut Volume<Blocks<'_>>, report: &mut Report) {
    let dir = match ready(volume.open_dir(DIR)) {
        Some(Ok(dir)) => dir,
        Some(Err(Error::NotFound)) => {
            let _ = writeln!(report, "{DIR} missing");
            return;
        }
        e => {
            let _ = writeln!(report, "{DIR} not opened ({})", Why(&e));
            finish(report)
        }
    };
    for path in READ {
        let entry = match ready(volume.find_from(dir, path)) {
            Some(Ok(entry)) => entry,
            Some(Err(Error::NotFound)) => continue,
            e => {
                let _ = writeln!(report, "not found: {path} ({})", Why(&e));
                finish(report)
            }
        };
        let mut file = match volume.open(&entry) {
            Ok(file) => file,
            Err(e) => {
                let _ = writeln!(report, "not opened: {path} ({e:?})");
                finish(report)
            }
        };
        let mut text = [0u8; 64];
        match ready(volume.read(&mut file, &mut text)) {
            Some(Ok(n)) => {
                let read = core::str::from_utf8(text.get(..n).unwrap_or_default()).unwrap_or("?");
                // Its first line up to any detail in brackets: "bootloader rollback: YES", for one.
                let line = read.lines().next().unwrap_or_default();
                let line = line.split(" (").next().unwrap_or_default();
                let _ = writeln!(report, "read: {}", cut(line, 28));
            }
            e => {
                let _ = writeln!(report, "read failed: {path} ({})", Why(&e));
                finish(report)
            }
        }
        return;
    }
    let _ = writeln!(report, "nothing to read in {DIR}");
}

/// Reads this image's file whole twice, one block per card command and then up to `sd::RUN`, and notes
/// its size, both times and the hash of what came (or that the two differ).
fn time_image(volume: &mut Volume<Blocks<'_>>, report: &mut Report) {
    let mut found = None;
    for path in IMAGE {
        match ready(volume.find(path)) {
            Some(Ok(entry)) => {
                found = Some(entry);
                break;
            }
            Some(Err(Error::NotFound)) => {}
            e => {
                let _ = writeln!(report, "image: not found ({})", Why(&e));
                finish(report)
            }
        }
    }
    let Some(entry) = found else {
        let _ = writeln!(report, "image: none");
        return;
    };
    let mut file = match volume.open(&entry) {
        Ok(file) => file,
        Err(e) => {
            let _ = writeln!(report, "image: not opened ({e:?})");
            finish(report)
        }
    };
    let mut buf = [0u8; sd::RUN * 512];
    let (one_ms, one) = read_whole(volume, &mut file, &mut buf, 1, "one", report);
    let (many_ms, many) = read_whole(volume, &mut file, &mut buf, sd::RUN, "many", report);
    if one == many {
        let _ = writeln!(
            report,
            "image: {} B, one {one_ms} ms, many {many_ms} ms, fnv {one:08x}",
            file.size()
        );
    } else {
        let _ = writeln!(report, "image: DIFFERENT, one {one:08x}, many {many:08x}");
    }
}

/// Reads `file` whole from its start through `buf`, at most `run` blocks a card read: the time it
/// took in milliseconds and the FNV-1a hash of its bytes. A failed read ends the test.
fn read_whole(
    volume: &mut Volume<Blocks<'_>>,
    file: &mut File,
    buf: &mut [u8],
    run: usize,
    pass: &str,
    report: &mut Report,
) -> (u64, u32) {
    volume.device_mut().set_run(run);
    file.seek(0);
    let start = Instant::now();
    let mut hash = FNV_BASIS;
    loop {
        match ready(volume.read(file, buf)) {
            Some(Ok(0)) => break,
            Some(Ok(n)) => {
                for &b in buf.get(..n).unwrap_or_default() {
                    hash = (hash ^ u32::from(b)).wrapping_mul(FNV_PRIME);
                }
            }
            e => {
                let _ = writeln!(report, "image: {pass} read failed ({})", Why(&e));
                finish(report)
            }
        }
    }
    (start.elapsed().as_millis(), hash)
}
