//! The X4 Pro's SD card from the bare-metal firmware, read only: hands the next boot back to the
//! C++ firmware, mounts the card as the SDK does (see `purestwhite_firmware::sd`), opens its FAT
//! volume, lists the root directory with long names, and reads a small file the C++ firmware
//! writes into `.crosspoint`, then reports and resets. Every write is refused below the file
//! system, so the card is left as it was. Without esp-rtos, so it runs in the emulator too.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

use alloc::string::String;
use core::fmt::Write as _;
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController, SlotConfig};
use purestwhite_fat::{Error, Kind, Name, Volume};
use purestwhite_firmware::ready;
use purestwhite_firmware::sd::{self, BlockingBus, Card, CardBlocks, SpinDelay};
use purestwhite_firmware::{Report, arm_watchdog, hand_back, hold_power_latch};

esp_bootloader_esp_idf::esp_app_desc!();

/// After a panic: note it in the report and reset, so the next boot is the C++ firmware.
#[allow(
    clippy::no_mangle_with_rust_abi,
    reason = "esp-backtrace declares it extern \"Rust\""
)]
#[unsafe(no_mangle)]
pub fn custom_halt() -> ! {
    Report::append_saved("PANIK\n");
    esp_hal::system::software_reset()
}

/// The directory the C++ firmware writes into, and the files in it read, the first found.
const DIR: &str = ".crosspoint";
const READ: [&str; 2] = ["onyukleyici.txt", "state.json"];
/// Bytes of the root's first names that go into the report.
const NAMES: usize = 40;

/// Saves the report with its last line and resets into the C++ firmware.
fn finish(report: &mut Report) -> ! {
    let _ = writeln!(report, "bitti");
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
            None => f.write_str("hazir degil"),
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
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let _rtc = arm_watchdog(peripherals.RTC_TIMER);
    let _latch = hold_power_latch(peripherals.GPIO1);
    let mut report = Report::default();
    let _ = writeln!(report, "purestWhite bare-metal SD denemesi");
    report.save();
    match hand_back(peripherals.FLASH, peripherals.CPU_CTRL) {
        Ok(_) => {
            let _ = writeln!(report, "sonraki acilis: C++ yazilimi");
        }
        Err(e) => {
            // The way back rests on the bootloader alone now: touch nothing more.
            let _ = writeln!(report, "sonraki acilis: SECILEMEDI ({e})");
            report.save();
            esp_hal::system::software_reset()
        }
    }
    report.save();
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // The card off until its first try.
    let mut enable = Output::new(peripherals.GPIO5, Level::High, OutputConfig::default());
    let Ok(controller) = SdHostController::new(peripherals.SDHOST, SdConfig::default()) else {
        let _ = writeln!(report, "sd: denetleyici kurulamadi");
        finish(&mut report)
    };
    let Ok(slot) = controller.slot::<1>(SlotConfig::default()) else {
        let _ = writeln!(report, "sd: yuva kurulamadi");
        finish(&mut report)
    };
    let slot = slot
        .with_clk(peripherals.GPIO41)
        .with_cmd(peripherals.GPIO42)
        .with_data0(peripherals.GPIO40);
    let mut card: Card<'_> = Card::new_uninit(BlockingBus(slot), SpinDelay(Delay::new()));
    // A word per try, saved before it starts: a try the watchdog cuts short still shows.
    let _ = write!(report, "sd: deneme");
    let mounted = sd::mount(&mut card, &mut enable, |attempt| {
        let _ = write!(report, " {attempt}");
        report.save();
    });
    if mounted.is_none() {
        let _ = writeln!(report, ", baglanamadi");
        finish(&mut report)
    }
    let card = CardBlocks(card);
    let _ = writeln!(report, ", tamam, {} MB", card.blocks() / 2048);
    report.save();

    browse(card, &mut report)
}

/// Opens the FAT volume, lists the root and reads a file in `DIR`, noting each in the report;
/// then finishes.
fn browse(card: CardBlocks<'_>, report: &mut Report) -> ! {
    let mut volume = match ready(Volume::mount(card)) {
        Some(Ok(v)) => v,
        e => {
            let _ = writeln!(report, "fat: acilamadi ({})", Why(&e));
            finish(report)
        }
    };
    let kind = match volume.kind() {
        Kind::Fat16 => "FAT16",
        Kind::Fat32 => "FAT32",
    };
    let _ = writeln!(report, "fat: {kind}, kume {} B", volume.cluster_bytes());
    report.save();

    // The root: how many entries, and the first names that fit.
    let root = volume.root();
    let mut cursor = volume.cursor(root);
    let mut name = Name::new();
    let (mut total, mut names) = (0u32, String::new());
    loop {
        match ready(volume.next(&mut cursor, &mut name)) {
            Some(Ok(Some(_))) => {
                total += 1;
                let name = name.as_str();
                if names.len() + name.len() + 2 <= NAMES {
                    if !names.is_empty() {
                        names.push_str(", ");
                    }
                    names.push_str(name);
                }
            }
            Some(Ok(None)) => break,
            e => {
                let _ = writeln!(report, "kok: {total} ogeden sonra okunamadi ({})", Why(&e));
                finish(report)
            }
        }
    }
    let _ = writeln!(report, "kok: {total} oge: {}", cut(&names, NAMES));
    report.save();

    // A file in a long-named directory, found by its long names: the first there; any error but
    // a missing file ends the test.
    let dir = match ready(volume.open_dir(DIR)) {
        Some(Ok(dir)) => dir,
        e => {
            let _ = writeln!(report, "{DIR} acilamadi ({})", Why(&e));
            finish(report)
        }
    };
    for path in READ {
        let entry = match ready(volume.find_from(dir, path)) {
            Some(Ok(entry)) => entry,
            Some(Err(Error::NotFound)) => continue,
            e => {
                let _ = writeln!(report, "bulunamadi: {path} ({})", Why(&e));
                finish(report)
            }
        };
        let mut file = match volume.open(&entry) {
            Ok(file) => file,
            Err(e) => {
                let _ = writeln!(report, "acilamadi: {path} ({e:?})");
                finish(report)
            }
        };
        let mut text = [0u8; 64];
        match ready(volume.read(&mut file, &mut text)) {
            Some(Ok(n)) => {
                let read = core::str::from_utf8(text.get(..n).unwrap_or_default()).unwrap_or("?");
                let _ = writeln!(report, "okundu: {}", cut(read.trim_end(), 60));
            }
            e => {
                let _ = writeln!(report, "okunamadi: {path} ({})", Why(&e));
            }
        }
        finish(report)
    }
    let _ = writeln!(report, "{DIR} icinde okunacak dosya yok");
    finish(report)
}
