//! wifi-join: gate 2's second step (ROADMAP.md): joining the network the C++ firmware joined last,
//! and an address from DHCP. It starts as every test image does (the next boot handed back to the
//! C++ firmware first), reads the saved networks from the card (`/.crosspoint/wifi.json`, the
//! passwords decoded with the device's MAC address: `purestwhite-wifistore`), starts the
//! scheduler and Wi-Fi, joins the network (`wifi::join`), asks DHCP for an address over
//! embassy-net (`wifi::net`), notes how each step went and how long it took, leaves the network
//! and resets into the C++ firmware. The report holds no network name, password or address; the
//! image sends nothing but what joining and DHCP need. On the device only. Built with
//! `--features wifi`.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::WIFI;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::systimer::SystemTimer;
use esp_radio::wifi::{ControllerConfig, WifiController};
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::wifi::{saved, steps};
use purestwhite_wifistore::Network;

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

/// A second region of the global heap, beside the 73,744 bytes of reclaimed RAM: Wi-Fi's buffers,
/// its driver's tasks and the network stack take their memory from the global heap.
const RADIO_HEAP: usize = 64 * 1024;

/// Joins `network` and asks DHCP for an address, each step's line in the report; then leaves it.
async fn join_and_dhcp(
    report: &mut Report,
    spawner: Spawner,
    radio: WIFI<'static>,
    rtc: &mut Rtc<'_>,
    network: &Network,
) {
    let Ok(mut controller) = WifiController::new(radio, ControllerConfig::default()) else {
        let _ = writeln!(report, "wifi: not started");
        return;
    };
    if steps::join_step(report, rtc, &mut controller, network).await {
        let _ = steps::dhcp_step(report, rtc, spawner).await;
    }
    steps::leave(&mut controller).await;
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    let board = Board::take(esp_hal::init(
        esp_hal::Config::default().with_cpu_clock(CpuClock::max()),
    ));
    let Started {
        mut rtc,
        latch: _latch,
        mut report,
        ..
    } = start("purestWhite bare-metal Wi-Fi join", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::heap_allocator!(size: RADIO_HEAP);

    let network = saved::saved_network(&mut report, board.card, || rtc.rwdt.feed());
    let systimer = SystemTimer::new(board.rtos.systimer);
    // The radio needs the scheduler running before it starts.
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);
    if let Some(network) = network {
        join_and_dhcp(&mut report, spawner, board.wifi, &mut rtc, &network).await;
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
