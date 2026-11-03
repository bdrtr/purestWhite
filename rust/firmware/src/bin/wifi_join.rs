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
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::WIFI;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::systimer::SystemTimer;
use esp_radio::wifi::{ControllerConfig, Interface, WifiController};
use purestwhite_firmware::board::Board;
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::wifi::{self, saved};
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
/// How long joining may take: the access point answers within seconds.
const JOIN_MOST: Duration = Duration::from_secs(30);
/// How long DHCP may take to give an address.
const DHCP_MOST: Duration = Duration::from_secs(20);
/// How long leaving may take: esp-radio waits for the driver's event without a limit.
const LEAVE_MOST: Duration = Duration::from_secs(5);

#[embassy_executor::task]
async fn net(mut runner: embassy_net::Runner<'static, Interface>) -> ! {
    runner.run().await
}

/// Joins `network` and asks DHCP for an address, each step's line in the report (`wifi: joining`
/// stands until the join ends, so a run cut short says where it was); then leaves it. The
/// watchdog (60 s) is fed between the steps, which take 30 and 20 s at most.
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
    let before = report.len();
    let _ = writeln!(report, "wifi: joining");
    report.save();
    rtc.rwdt.feed();
    let began = Instant::now();
    let joined = select(
        wifi::join::join(&mut controller, network),
        Timer::after(JOIN_MOST),
    )
    .await;
    let ms = began.elapsed().as_millis();
    report.truncate(before);
    let _ = match joined {
        Either::First(Ok(joined)) => writeln!(report, "{joined}, {ms} ms"),
        Either::First(Err(error)) => writeln!(report, "{error}"),
        Either::Second(()) => writeln!(report, "wifi: no answer in {} s", JOIN_MOST.as_secs()),
    };
    report.save();
    rtc.rwdt.feed();
    if let Either::First(Ok(_)) = joined {
        note_dhcp(report, spawner).await;
        rtc.rwdt.feed();
    }
    let _ = select(controller.disconnect_async(), Timer::after(LEAVE_MOST)).await;
}

/// DHCP's line: how long it took to give an address, or that it gave none.
async fn note_dhcp(report: &mut Report, spawner: Spawner) {
    let Some((stack, runner)) = wifi::net::station_stack() else {
        let _ = writeln!(report, "dhcp: no interface");
        return;
    };
    spawner.spawn(net(runner).unwrap());
    let began = Instant::now();
    let up = select(stack.wait_config_up(), Timer::after(DHCP_MOST)).await;
    let ms = began.elapsed().as_millis();
    let _ = match up {
        Either::First(()) => writeln!(report, "dhcp: address in {ms} ms"),
        Either::Second(()) => writeln!(report, "dhcp: no address in {} s", DHCP_MOST.as_secs()),
    };
    report.save();
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
