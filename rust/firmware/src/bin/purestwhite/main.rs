//! purestWhite: the bare-metal firmware's Embassy main (rust/firmware/DESIGN.md, step 2), the
//! image that grows into the real firmware. During phase 2 it first hands the next boot back to
//! the C++ firmware (`boot::start`), like every test image, so that any reset, panic or wake lands
//! there.
//!
//! It runs the application (`purestwhite-app`) through the services' loops: the home screen and,
//! when Power is held, the sleep screen, drawn into two frames in PSRAM and shown on the panel
//! while the other tasks run; the buttons on their interrupts. Then the sleep handshake
//! (`power`), and a deep sleep until Power, whose wake boots the C++ firmware; and during phase 2
//! a backstop that brings the device back there without the owner (`backstop`). Without a font,
//! the battery or the clock for now: the screens' shapes only. On the device only: esp-rtos does
//! not run in eink-qemu 0.9.2.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

extern crate alloc;

mod backstop;
mod power;
mod tasks;

use core::fmt::Write as _;
use embassy_executor::Spawner;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::psram::Psram;
use esp_hal::timer::systimer::SystemTimer;
use power::Power;
use purestwhite_firmware::board::{Board, PanelParts, psram_config};
use purestwhite_firmware::boot::{self, Report, Started, start};
use purestwhite_firmware::input::ButtonPins;
use purestwhite_firmware::memory::{add_psram, psram_frame};
use purestwhite_firmware::panel;
use purestwhite_firmware::screen::DeviceScreen;
use tasks::LINKS;

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

/// The PSRAM as its own heap, and the two frames in it; a reset, after noting it, when there is
/// no room for them.
fn give_frames(report: &mut Report, psram: &Psram) {
    let kb = add_psram(psram) / 1024;
    let given = match [psram_frame(), psram_frame()] {
        [Some(a), Some(b)] => LINKS.give_frames([a, b]),
        _ => false,
    };
    let _ = writeln!(
        report,
        "psram {kb} KB, frames {}",
        if given { "ok" } else { "NONE" }
    );
    report.save();
    if !given {
        esp_hal::system::software_reset();
    }
}

/// The panel (`panel::open`), not yet begun; a reset, after noting it, when it cannot be used.
fn open_screen(report: &mut Report, parts: PanelParts) -> DeviceScreen {
    match panel::open(parts, |_| None) {
        Ok(opened) => {
            let screen = DeviceScreen::new(opened);
            let _ = writeln!(report, "panel {}", screen.name);
            report.save();
            screen
        }
        Err(error) => {
            let name = error.probe().map_or("none", |p| p.controller().name());
            let _ = writeln!(report, "panel {name}: unusable");
            report.save();
            esp_hal::system::software_reset()
        }
    }
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let board = Board::take(esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    ));
    // Keep the title short: the report holds 248 bytes.
    let Started {
        rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite", board.boot);
    // The boot counter: during phase 2 it is only noted (there is no recovery mode yet), and the
    // hand-back has already made the next boot the C++ firmware's.
    let _ = writeln!(report, "{}", boot::begin_run());
    report.save();
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    // Before the scheduler, as psram-check does: mapping the PSRAM suspends the cache.
    let psram = Psram::new(board.psram, psram_config());
    give_frames(&mut report, &psram);
    let systimer = SystemTimer::new(board.rtos.systimer);
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);

    let screen = open_screen(&mut report, board.panel);
    spawner.spawn(tasks::app().unwrap());
    spawner.spawn(tasks::draw().unwrap());
    spawner.spawn(tasks::panel(screen).unwrap());
    spawner.spawn(tasks::keys(ButtonPins::new(board.buttons)).unwrap());
    spawner.spawn(backstop::backstop().unwrap());
    esp_println::println!("purestwhite: started");

    let power = Power::new(
        rtc,
        board.low_power,
        board.i2c.supply,
        board.card.supply,
        report,
    );
    power.run().await
}
