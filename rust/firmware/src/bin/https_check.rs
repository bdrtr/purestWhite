//! https-check: gate 2's download (ROADMAP.md): a file fetched over HTTPS on the bare-metal
//! firmware. It starts as every test image does (the next boot handed back to the C++ firmware
//! first), reads the battery-backed clock (`clock`, the time TLS checks certificates' dates
//! against), reads the network the C++ firmware saved (`wifi::saved`), joins it and gets an address
//! (`wifi::steps`), fetches `PATH` from `HOST` over TLS with wolfSSL (`wifi::https`, the chain
//! checked against the C++ firmware's roots, ESP-IDF's certificate bundle), notes its status,
//! size, SHA-256 and the time each part took, leaves the network and resets into the C++
//! firmware. Nothing is written
//! to the card; the report holds no network name, password or address. On the device only. Built
//! with `--features wifi`.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{SHA, WIFI};
use esp_hal::rng::Rng;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::sha::Sha;
use esp_hal::timer::systimer::SystemTimer;
use esp_radio::wifi::{ControllerConfig, WifiController};
use purestwhite_firmware::board::{Board, I2cParts};
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::sha::HwSha;
use purestwhite_firmware::wifi::{https, saved, steps};
use purestwhite_firmware::{clock, i2c};
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

/// A file that never changes: this repository's roadmap at a pushed commit (10,246 bytes, SHA-256
/// c4e5578c...), behind a Let's Encrypt chain (YR1, Root YR) to ISRG Root X1.
const HOST: &str = "raw.githubusercontent.com";
const PATH: &str =
    "/bdrtr/purestWhite/6afcf3085effd63c2831f43e964f70f8829a1b06/rust/firmware/ROADMAP.md";
/// A second region of the global heap, beside the 73,744 bytes of reclaimed RAM: Wi-Fi, the
/// network stack and wolfSSL take their memory from the global heap (internal RAM).
const RADIO_HEAP: usize = 96 * 1024;
/// How long the fetch may take, within the watchdog's 60 s.
const FETCH_MOST: Duration = Duration::from_secs(40);

/// Random bytes for TLS from the chip's generator, true random while the radio is on.
fn random(bytes: &mut [u8]) {
    Rng::new().read(bytes);
}

/// The clock's line, and its time given to TLS; the touch panel's supply, on for the bus, kept.
fn note_clock(report: &mut Report, parts: I2cParts) -> Output<'static> {
    let supply = Output::new(parts.supply, Level::Low, OutputConfig::default());
    esp_hal::delay::Delay::new().delay_millis(u32::try_from(i2c::SUPPLY_MS).unwrap_or(50));
    let read = i2c::open_bus(parts.i2c, parts.sda, parts.scl)
        .as_mut()
        .and_then(clock::read);
    let _ = match &read {
        Some(read) => writeln!(report, "{read}"),
        None => writeln!(report, "clock: not set"),
    };
    if let Some(seconds) = read.and_then(|r| r.unix_seconds()) {
        purestwhite_tls::set_time(seconds);
    }
    report.save();
    supply
}

/// Fetches the file, its line in the report.
async fn note_fetch(report: &mut Report, rtc: &mut Rtc<'_>, spawner: Spawner, sha: SHA<'static>) {
    let Some(stack) = steps::dhcp_step(report, rtc, spawner).await else {
        return;
    };
    let before = report.len();
    let _ = writeln!(report, "https: fetching");
    report.save();
    rtc.rwdt.feed();
    let mut sha = Sha::new(sha);
    let fetch = https::get(stack, HOST, PATH, HwSha::new(&mut sha));
    let fetched = select(fetch, Timer::after(FETCH_MOST)).await;
    report.truncate(before);
    let _ = match fetched {
        Either::First(Ok(fetched)) => writeln!(report, "{fetched}"),
        Either::First(Err(error)) => writeln!(report, "{error}"),
        Either::Second(()) => writeln!(report, "https: no end in {} s", FETCH_MOST.as_secs()),
    };
    report.save();
    rtc.rwdt.feed();
}

/// Joins `network`, fetches the file, and leaves.
async fn join_and_fetch(
    report: &mut Report,
    rtc: &mut Rtc<'_>,
    spawner: Spawner,
    radio: (WIFI<'static>, SHA<'static>),
    network: &Network,
) {
    let Ok(mut controller) = WifiController::new(radio.0, ControllerConfig::default()) else {
        let _ = writeln!(report, "wifi: not started");
        return;
    };
    if steps::join_step(report, rtc, &mut controller, network).await {
        note_fetch(report, rtc, spawner, radio.1).await;
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
    } = start("purestWhite HTTPS check", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::heap_allocator!(size: RADIO_HEAP);
    purestwhite_tls::set_random(random);

    let _touch_supply = note_clock(&mut report, board.i2c);
    let network = saved::saved_network(&mut report, board.card, || rtc.rwdt.feed());
    let systimer = SystemTimer::new(board.rtos.systimer);
    // The radio needs the scheduler running before it starts.
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);
    if let Some(network) = network {
        let radio = (board.wifi, board.sha);
        join_and_fetch(&mut report, &mut rtc, spawner, radio, &network).await;
    }
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
