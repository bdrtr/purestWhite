//! BLE through esp-radio's controller and `TrouBLE`'s host (the `ble` feature), for gate 3
//! (ROADMAP.md): for now a scan for page-turn remotes (`scan`), counted by `purestwhite-ble`. The
//! radio needs the scheduler started first (`esp_rtos`) and takes its memory from the global heap
//! (internal RAM). No name or address is kept: the report goes on the card.

use bt_hci::controller::ExternalController;
use core::cell::RefCell;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::BT;
use esp_radio::ble::Config;
use esp_radio::ble::controller::BleConnector;
use purestwhite_ble::Seen;
use trouble_host::prelude::{
    Address, DefaultPacketPool, EventHandler, HostResources, ScanConfig, Scanner,
};

/// How long the runner is given to turn the scan off.
const STOP_GRACE: Duration = Duration::from_millis(100);
/// The HCI commands the host may have in flight at once.
const SLOTS: usize = 20;

/// Why a scan could not be had.
#[derive(Clone, Copy, Debug)]
pub enum ScanError {
    /// The BLE controller could not start.
    Start,
    /// The host stack failed (what it heard so far is lost with it).
    Host,
    /// The scan could not start.
    Scan,
}

/// The tally, as the host's runner hands it the advertising reports.
struct Tally(RefCell<Seen>);

impl EventHandler for Tally {
    fn on_adv_reports(&self, reports: bt_hci::param::LeAdvReportsIter) {
        let mut seen = self.0.borrow_mut();
        for report in reports.flatten() {
            let addr = (report.addr_kind.into_inner(), report.addr.into_inner());
            seen.report(addr, report.data, report.rssi);
        }
    }
}

/// The device's own address for the scan: a random static one, from its eFuse MAC address with
/// the two top bits set, as random static addresses have them.
fn own_address() -> Address {
    let mut addr = [0u8; 6];
    let mac = esp_hal::efuse::base_mac_address();
    for (to, from) in addr.iter_mut().zip(mac.as_bytes()) {
        *to = *from;
    }
    if let Some(top) = addr.last_mut() {
        *top |= 0xC0;
    }
    Address::random(addr)
}

/// Starts BLE on `bt`, scans actively for `how_long`, and stops the scan: what it heard. The
/// controller itself stays on until the next reset (the host keeps it, and does not hand it back
/// to be dropped); it no longer reports, so nothing piles up.
///
/// # Errors
/// The controller, the host or the scan could not start.
pub async fn scan(bt: BT<'static>, how_long: Duration) -> Result<Seen, ScanError> {
    let connector = BleConnector::new(bt, Config::default()).map_err(|_| ScanError::Start)?;
    let controller: ExternalController<_, SLOTS> = ExternalController::new(connector);
    let mut resources: HostResources<_, DefaultPacketPool, 1, 1> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(own_address())
        .build();
    let mut runner = stack.runner();
    let tally = Tally(RefCell::new(Seen::new()));
    let mut scanner = Scanner::new(stack.central());
    let scanning = async {
        let session = scanner
            .scan(&ScanConfig::default())
            .await
            .map_err(|_| ScanError::Scan)?;
        Timer::after(how_long).await;
        // The session's end asks the runner, still polled here, to turn the scan off.
        drop(session);
        Timer::after(STOP_GRACE).await;
        Ok::<(), ScanError>(())
    };
    match select(runner.run_with_handler(&tally), scanning).await {
        Either::First(_) => return Err(ScanError::Host),
        Either::Second(result) => result?,
    }
    Ok(tally.0.into_inner())
}
