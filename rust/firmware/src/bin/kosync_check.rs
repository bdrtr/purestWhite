//! kosync-check: gate 2's sync step (ROADMAP.md): the `KOReader` sync account the C++ firmware
//! saved, signed in at its server from the bare-metal firmware. It starts as every test image does
//! (the next boot handed back to the C++ firmware first), reads the battery-backed clock (the time
//! TLS checks certificates' dates against), reads the saved network and the account from the card
//! in one session (`card`, `wifi::saved`, `wifi::kosync`), joins the network and gets an address
//! (`wifi::steps`), sends `KOReaderSyncClient::authenticate`'s request over TLS (an `https` server
//! only), notes the status and the time each part took, leaves the network and resets into the C++
//! firmware. Nothing is written to the card or the server; the report holds no account name,
//! password, server, network name or address. On the device only. Built with `--features wifi`.
#![no_std]
#![warn(clippy::pedantic)]
#![no_main]

use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::WIFI;
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::systimer::SystemTimer;
use esp_radio::wifi::{ControllerConfig, WifiController};
use purestwhite_firmware::board::{Board, CardParts};
use purestwhite_firmware::boot::{Report, Started, start};
use purestwhite_firmware::card::read_files;
use purestwhite_firmware::test_image::tls;
use purestwhite_firmware::wifi::kosync::{self, Checkable};
use purestwhite_firmware::wifi::{saved, steps};
use purestwhite_kosync::{JSON_MOST as ACCOUNT_MOST, SETTINGS_PATH};
use purestwhite_wifistore::{JSON_MOST, Network};

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

/// A second region of the global heap, beside the 73,744 bytes of reclaimed RAM: Wi-Fi, the
/// network stack and wolfSSL take their memory from the global heap (internal RAM).
const RADIO_HEAP: usize = 96 * 1024;
/// How long the sign-in may take, within the watchdog's 60 s.
const SIGN_IN_MOST: Duration = Duration::from_secs(40);

/// The saved network and the account, read from the card in one session, each line in the report.
fn read_card(
    report: &mut Report,
    card: CardParts,
    feed: impl FnMut(),
) -> Option<(Network, Checkable)> {
    let files = [(saved::SAVED, JSON_MOST), (SETTINGS_PATH, ACCOUNT_MOST)];
    let Ok([wifi, account]) = read_files(card, files, feed) else {
        saved::note_saved(report, Err("no card"));
        return None;
    };
    let network = saved::note_network(report, wifi.as_deref().map_err(|e| *e));
    let checkable = kosync::note_account(report, account.as_deref().map_err(|e| *e));
    Some((network?, checkable?))
}

/// Signs the account in, `kosync: signing in` standing in the report while it runs.
async fn sign_in(
    report: &mut Report,
    rtc: &mut Rtc<'_>,
    stack: embassy_net::Stack<'static>,
    checkable: &Checkable,
) {
    let before = report.len();
    let _ = writeln!(report, "kosync: signing in");
    report.save();
    rtc.rwdt.feed();
    let checked = select(kosync::check(stack, checkable), Timer::after(SIGN_IN_MOST)).await;
    report.truncate(before);
    let _ = match checked {
        Either::First(checked) => writeln!(report, "{checked}"),
        Either::Second(()) => writeln!(report, "kosync: no end in {} s", SIGN_IN_MOST.as_secs()),
    };
    report.save();
    rtc.rwdt.feed();
}

/// Joins `network`, signs the account in, and leaves.
async fn join_and_check(
    report: &mut Report,
    rtc: &mut Rtc<'_>,
    spawner: Spawner,
    radio: WIFI<'static>,
    (network, checkable): (&Network, &Checkable),
) {
    let Ok(mut controller) = WifiController::new(radio, ControllerConfig::default()) else {
        let _ = writeln!(report, "wifi: not started");
        return;
    };
    if steps::join_step(report, rtc, &mut controller, network).await
        && let Some(stack) = steps::dhcp_step(report, rtc, spawner).await
    {
        sign_in(report, rtc, stack, checkable).await;
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
    } = start("purestWhite KOSync check", board.boot);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::heap_allocator!(size: RADIO_HEAP);
    purestwhite_tls::set_random(tls::random);

    let _touch_supply = tls::note_clock(&mut report, board.i2c);
    let found = read_card(&mut report, board.card, || rtc.rwdt.feed());
    let systimer = SystemTimer::new(board.rtos.systimer);
    // The radio needs the scheduler running before it starts.
    esp_rtos::start(systimer.alarm0, board.rtos.interrupt);
    if let Some((network, checkable)) = &found {
        join_and_check(
            &mut report,
            &mut rtc,
            spawner,
            board.wifi,
            (network, checkable),
        )
        .await;
    }
    drop(found); // the account's secrets wiped before the reset
    let _ = writeln!(report, "done");
    esp_println::println!("{}", report.as_str());
    report.save();
    Timer::after_millis(100).await;
    esp_hal::system::software_reset()
}
