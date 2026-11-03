//! A scan for page-turn remotes, counted by `purestwhite-ble` (ble-check).

use super::{Hci, STOP_GRACE, Tally, own_address};
use core::cell::RefCell;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer};
use esp_hal::peripherals::BT;
use esp_radio::ble::Config;
use esp_radio::ble::controller::BleConnector;
use purestwhite_ble::Seen;
use trouble_host::prelude::{
    Central, DefaultPacketPool, ExternalController, HostResources, ScanConfig, Scanner,
};

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

/// Scans actively for `how_long` with `central`, then stops the scan; the runner must be polled
/// meanwhile (it hands the reports to the tally).
pub(super) async fn listen(
    central: Central<'_, Hci, DefaultPacketPool>,
    how_long: Duration,
) -> Result<(), ScanError> {
    let mut scanner = Scanner::new(central);
    let session = scanner
        .scan(&ScanConfig::default())
        .await
        .map_err(|_| ScanError::Scan)?;
    Timer::after(how_long).await;
    // The session's end asks the runner, still polled, to turn the scan off.
    drop(session);
    Timer::after(STOP_GRACE).await;
    Ok(())
}

/// Starts BLE on `bt`, scans actively for `how_long`, and stops the scan: what it heard. The
/// host is never dropped (dropping it would shut the controller down, esp-radio's `ble_deinit`,
/// before the caller saves what it heard): the controller stays on until the reset that follows.
/// It no longer reports, so nothing piles up.
///
/// # Errors
/// The controller, the host or the scan could not start.
pub async fn scan(bt: BT<'static>, how_long: Duration) -> Result<Seen, ScanError> {
    let connector = BleConnector::new(bt, Config::default()).map_err(|_| ScanError::Start)?;
    let controller: Hci = ExternalController::new(connector);
    // One link, one channel, one advertising set, one bond, as `remote` keeps.
    let mut resources: HostResources<_, DefaultPacketPool, 1, 1, 1, 1> = HostResources::new();
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(own_address())
        .build();
    let mut runner = stack.runner();
    let tally = Tally(RefCell::new(Seen::new()));
    let result = match select(
        runner.run_with_handler(&tally),
        listen(stack.central(), how_long),
    )
    .await
    {
        Either::First(_) => Err(ScanError::Host),
        Either::Second(result) => result,
    };
    core::mem::forget(stack);
    result.map(|()| tally.0.into_inner())
}
