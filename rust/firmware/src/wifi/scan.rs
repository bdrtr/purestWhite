//! A scan of the networks around, for the report.

use core::fmt::{self, Display, Formatter};
use esp_hal::peripherals::WIFI;
use esp_radio::wifi::scan::ScanConfig;
use esp_radio::wifi::{ControllerConfig, WifiController, WifiError};

/// How many networks a scan copies out of the driver at most; a scan that reaches it noted `64+`.
pub const MOST: usize = 64;

/// What a scan found: how many networks (each access point counted), and the strongest one's
/// signal and channel. No names: the report goes on the card, and the networks around are not the
/// owner's to note.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scan {
    pub networks: usize,
    /// The strongest signal, in dBm, and its channel.
    pub best: Option<(i8, u8)>,
}

/// `wifi: 7 networks, best -48 dBm on channel 6` (`64+ networks` at `MOST`), or
/// `wifi: no networks`.
impl Display for Scan {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let Some((dbm, channel)) = self.best else {
            return out.write_str("wifi: no networks");
        };
        let more = if self.networks >= MOST { "+" } else { "" };
        write!(
            out,
            "wifi: {}{more} networks, best {dbm} dBm on channel {channel}",
            self.networks
        )
    }
}

/// Why a scan could not be had.
#[derive(Debug)]
pub enum ScanError {
    /// The Wi-Fi controller could not start.
    Start(WifiError),
    /// The scan itself failed.
    Scan(WifiError),
}

/// Starts Wi-Fi in station mode on `wifi`, scans once and stops it again (the controller is
/// dropped): what it found.
///
/// # Errors
/// The controller could not start, or the scan failed.
pub async fn scan(wifi: WIFI<'static>) -> Result<Scan, ScanError> {
    let mut controller =
        WifiController::new(wifi, ControllerConfig::default()).map_err(ScanError::Start)?;
    let found = controller
        .scan_async(&ScanConfig::default().with_max(MOST))
        .await
        .map_err(ScanError::Scan)?;
    let best = found
        .iter()
        .max_by_key(|ap| ap.signal_strength)
        .map(|ap| (ap.signal_strength, ap.channel));
    Ok(Scan {
        networks: found.len(),
        best,
    })
}
