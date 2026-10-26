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
use core::ops::ControlFlow;
use embedded_sdmmc::{
    LfnBuffer, Mode, ShortFileName, TimeSource, Timestamp, VolumeIdx, VolumeManager,
};
use esp_backtrace as _;
use esp_hal::delay::Delay;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::sdmmc::{Config as SdConfig, SdHostController, SlotConfig};
use purestwhite_firmware::sd::{self, BlockingBus, Card, ReadOnlyCard, SpinDelay};
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

/// The directory read, and the files in it read, the first found: what the C++ firmware writes.
const DIR: &str = ".crosspoint";
const READ: [&str; 2] = ["onyukleyici.txt", "state.json"];
/// Bytes of the root's first names that go into the report.
const NAMES: usize = 40;

/// No clock: nothing is written, so no time is needed.
struct NoClock;

impl TimeSource for NoClock {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp::from_calendar(2026, 1, 1, 0, 0, 0).unwrap_or(Timestamp {
            year_since_1970: 56,
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 0,
            minutes: 0,
            seconds: 0,
        })
    }
}

/// Saves the report with its last line and resets into the C++ firmware.
fn finish(report: &mut Report) -> ! {
    let _ = writeln!(report, "bitti");
    esp_println::println!("{}", report.as_str());
    report.save();
    esp_hal::system::software_reset()
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
    let card = ReadOnlyCard::new(card);
    let _ = writeln!(report, ", tamam, {} MB", card.blocks() / 2048);
    report.save();

    browse(card, &mut report)
}

/// Opens the FAT volume, lists the root and reads a file in `DIR`, noting each in the report;
/// then finishes.
fn browse(card: ReadOnlyCard<'_>, report: &mut Report) -> ! {
    let manager: VolumeManager<_, _> = VolumeManager::new(card, NoClock);
    let volume = match manager.open_volume(VolumeIdx(0)) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(report, "fat: acilamadi ({e:?})");
            finish(report)
        }
    };
    let Ok(root) = volume.open_root_dir() else {
        let _ = writeln!(report, "fat: kok acilamadi");
        finish(report)
    };

    // The root: how many entries, the first names that fit, and the short name of `DIR`.
    let mut lfn_storage = [0u8; 256];
    let mut lfn = LfnBuffer::new(&mut lfn_storage);
    let (mut total, mut names, mut dir) = (0u32, String::new(), None::<ShortFileName>);
    let listed = root.iterate_dir_lfn(&mut lfn, |entry, long| {
        if entry.attributes.is_volume() {
            return ControlFlow::Continue(());
        }
        total += 1;
        let mut short = String::new();
        let _ = write!(short, "{}", entry.name);
        // A name too long for the buffer comes empty: the short one then.
        let name = long.filter(|l| !l.is_empty()).unwrap_or(&short);
        if name == DIR {
            dir = Some(entry.name);
        }
        if names.len() + name.len() + 2 <= NAMES {
            if !names.is_empty() {
                names.push_str(", ");
            }
            names.push_str(name);
        }
        ControlFlow::Continue(())
    });
    if let Err(e) = listed {
        let _ = writeln!(report, "kok: {total} ogeden sonra okunamadi ({e:?})");
        finish(report)
    }
    let _ = writeln!(report, "kok: {total} oge: {}", cut(&names, NAMES));
    report.save();

    // A file in a long-named directory, opened by its long name.
    let Some(dir) = dir.and_then(|name| root.open_dir(name).ok()) else {
        let _ = writeln!(report, "{DIR} yok");
        finish(report)
    };
    for name in READ {
        let Ok(file) = dir.open_long_name_file_in_dir(name, Mode::ReadOnly) else {
            continue;
        };
        let mut text = [0u8; 64];
        match file.read(&mut text) {
            Ok(n) => {
                let read = core::str::from_utf8(text.get(..n).unwrap_or_default()).unwrap_or("?");
                let _ = writeln!(report, "okundu: {}", cut(read.trim_end(), 60));
            }
            Err(e) => {
                let _ = writeln!(report, "okunamadi: {name} ({e:?})");
            }
        }
        finish(report)
    }
    let _ = writeln!(report, "{DIR} icinde okunacak dosya yok");
    finish(report)
}
