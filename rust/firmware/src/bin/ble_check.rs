//! ble-check: the first step of gate 3 (ROADMAP.md): esp-radio's BLE controller and `TrouBLE`'s
//! host on the device. It starts as every test image does (the next boot handed back to the C++
//! firmware first), starts the scheduler and BLE, scans actively for `SCAN` and notes what it
//! heard (`crate::ble`, `purestwhite-ble`: how many advertisers, how many are HID remotes as the
//! C++ firmware's SDK judges them, the strongest remote's signal; no names or addresses), then
//! resets into the C++ firmware. It connects to nothing. On the device only. Built with
//! `--features ble`.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::BT;
use esp_hal::timer::systimer::SystemTimer;
use purestwhite_firmware::ble;
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
/// How long the scan listens: a remote advertises every 20 ms to a few hundred.
const SCAN: Duration = Duration::from_secs(10);
/// How long the whole scan may take, the controller's start included, within the watchdog's 60 s.
const SCAN_MOST: Duration = Duration::from_secs(25);

/// The scan's line, and how long it took, in the report; `ble: scanning` stands until it ends.
async fn note_scan(report: &mut Report, radio: BT<'static>) {
    let before = report.len();
    let _ = writeln!(report, "ble: scanning");
    report.save();
    let began = Instant::now();
    let scanned = select(ble::scan(radio, SCAN), Timer::after(SCAN_MOST)).await;
    let ms = began.elapsed().as_millis();
    report.truncate(before);
    let _ = match scanned {
        Either::First(Ok(seen)) => writeln!(report, "{seen}, {ms} ms"),
        Either::First(Err(error)) => writeln!(report, "ble: {error:?}"),
        Either::Second(()) => writeln!(report, "ble: no end in {} s", SCAN_MOST.as_secs()),
    };
    report.save();
}

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
        rtc: _rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal BLE check", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::heap_allocator!(size: RADIO_HEAP);
    let systimer = SystemTimer::new(board.rtos.systimer);
    // The radio needs the scheduler running before it starts.
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);
    esp_println::println!("ble: started");

    note_scan(&mut report, board.bt).await;
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
