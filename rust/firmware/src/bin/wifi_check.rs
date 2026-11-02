//! wifi-check: the first step of gate 2 (ROADMAP.md): esp-radio's Wi-Fi on the device. It starts as
//! every test image does (the next boot handed back to the C++ firmware first), starts the
//! scheduler, starts Wi-Fi in station mode, scans once and notes what it found (`crate::wifi`:
//! how many networks, the strongest signal and its channel, no names), then resets into the C++
//! firmware. It joins no network and sends nothing. On the device only: esp-rtos does not run in
//! eink-qemu 0.9.2. Built with `--features wifi`.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::WIFI;
use esp_hal::timer::systimer::SystemTimer;
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::wifi;

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

/// A second region of the global heap, beside the 73,744 bytes of reclaimed RAM: Wi-Fi's buffers
/// and its driver's tasks take their memory from the global heap (internal RAM).
const RADIO_HEAP: usize = 64 * 1024;

/// How long the scan may take: a few seconds at most, but esp-radio waits for the driver's
/// scan-done event without a limit.
const SCAN_MOST: Duration = Duration::from_secs(20);

/// The scan's line, and how long it took, in the report; `wifi: scanning` stands until it ends,
/// so a run the watchdog cut short says where it was.
async fn note_scan(report: &mut Report, radio: WIFI<'static>) {
    let before = report.len();
    let _ = writeln!(report, "wifi: scanning");
    report.save();
    let began = Instant::now();
    let scanned = select(wifi::scan(radio), Timer::after(SCAN_MOST)).await;
    let ms = began.elapsed().as_millis();
    report.truncate(before);
    let _ = match scanned {
        Either::First(Ok(scan)) => writeln!(report, "{scan}, {ms} ms"),
        Either::First(Err(error)) => writeln!(report, "wifi: {error:?}"),
        Either::Second(()) => writeln!(report, "wifi: no answer in {} s", SCAN_MOST.as_secs()),
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
    } = start("purestWhite bare-metal Wi-Fi check", board.boot);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::heap_allocator!(size: RADIO_HEAP);
    let systimer = SystemTimer::new(board.rtos.systimer);
    // The radio needs the scheduler running before it starts.
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);
    esp_println::println!("wifi: started");

    note_scan(&mut report, board.wifi).await;
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
