//! ble-remote: gate 3's link (ROADMAP.md): a page-turn remote linked as the C++ firmware links
//! it. It starts as every test image does (the next boot handed back to the C++ firmware first),
//! starts the scheduler and BLE, scans for `SCAN`, connects to the strongest HID remote it heard,
//! pairs with it (asking the remote to bond, legacy pairing allowed, as the SDK pairs), sets its
//! HID service up as the SDK does, reads its reports for `LISTEN` as page turns
//! (`purestwhite-ble`'s decoder and the C++ firmware's built-in button tables), notes each step
//! and the turns counted (no name or address), closes the link and resets into the C++ firmware.
//! This image keeps no bond itself, so the remote must be in pairing mode. On the device only.
//! Built with `--features ble`.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::timer::systimer::SystemTimer;
use purestwhite_firmware::ble::{self, Plan};
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

/// A second region of the global heap, beside the 73,744 bytes of reclaimed RAM: the BLE
/// controller and host take their memory from the global heap.
const RADIO_HEAP: usize = 64 * 1024;
/// How long the scan listens: a remote in pairing mode advertises every 20 ms to a few hundred.
const SCAN: Duration = Duration::from_secs(8);
/// How long the remote's reports are read: the time to press each of its buttons a few times.
const LISTEN: Duration = Duration::from_secs(30);

#[esp_rtos::main]
#[allow(
    clippy::used_underscore_binding,
    reason = "esp-rtos's main takes the spawner; this image spawns no task"
)]
async fn main(_spawner: Spawner) -> ! {
    let board = Board::take(esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    ));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite BLE remote", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::heap_allocator!(size: RADIO_HEAP);
    let systimer = SystemTimer::new(board.rtos.systimer);
    // The radio needs the scheduler running before it starts.
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);

    let before = report.len();
    let _ = writeln!(report, "ble: linking");
    report.save();
    let plan = Plan {
        scan: SCAN,
        listen: LISTEN,
    };
    let outcome = ble::remote(board.bt, plan, &mut || rtc.rwdt.feed()).await;
    report.truncate(before);
    let _ = write!(report, "{outcome}");
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
