//! BLE through esp-radio's controller and `TrouBLE`'s host (the `ble` feature), for gate 3
//! (ROADMAP.md): a scan for page-turn remotes (`scan`), counted by `purestwhite-ble`, and a link
//! to the strongest one (`remote`): connected, paired, its HID reports subscribed to and read as
//! page turns. The radio needs the scheduler started first (`esp_rtos`) and takes its memory from
//! the global heap (internal RAM). No name or address goes in the report: it goes on the card.

mod hid;
mod link;
mod outcome;
mod remote;
mod reports;
mod scan;

pub use outcome::{Failure, HidNote, LinkNote, Outcome, Plan};
pub use remote::remote;
pub use reports::Turns;
pub use scan::{ScanError, scan};

use bt_hci::controller::ExternalController;
use core::cell::RefCell;
use embassy_time::Duration;
use esp_radio::ble::controller::BleConnector;
use purestwhite_ble::Seen;
use trouble_host::prelude::{Address, EventHandler};

/// How long the runner is given to turn the scan off.
const STOP_GRACE: Duration = Duration::from_millis(100);
/// The HCI commands the host may have in flight at once.
const SLOTS: usize = 20;

/// The controller as the host drives it.
type Hci = ExternalController<BleConnector<'static>, SLOTS>;

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

/// The device's own address: a random static one, from its eFuse MAC address with the two top
/// bits set, as random static addresses have them.
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
