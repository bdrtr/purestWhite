//! Joining a network the C++ firmware saved (`purestwhite-wifistore`): station mode, WPA2
//! Personal with the saved password (or open without one). esp-radio 1.0.0-beta.1 has no WPA3: a
//! WPA2/WPA3 network is joined over WPA2, a WPA3-only one not at all. Its fast scan joins the first
//! access point of the name it hears, not the strongest.

use core::fmt::{self, Display, Formatter};
use esp_radio::wifi::sta::StationConfig;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, ConnectionError, DisconnectReason, Password, Ssid,
    WifiController, WifiError,
};
use purestwhite_wifistore::Network;

/// The network joined: its channel and the signal, in dBm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Joined {
    pub channel: u8,
    pub dbm: Option<i32>,
}

/// `wifi: joined on channel 6, -52 dBm`.
impl Display for Joined {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        write!(out, "wifi: joined on channel {}", self.channel)?;
        if let Some(dbm) = self.dbm {
            write!(out, ", {dbm} dBm")?;
        }
        Ok(())
    }
}

/// Why the network could not be joined.
#[derive(Clone, Copy, Debug)]
pub enum JoinError {
    /// The saved name or password does not fit Wi-Fi's limits.
    Saved,
    /// The station could not be set up.
    Config(WifiError),
    /// The access point refused or never answered, and why (`FourWayHandshakeTimeout`: most
    /// likely a wrong password; `NoAccessPointFound`: no such network around;
    /// `NoAccessPointFoundWithCompatibleSecurity`: a WPA3-only network, say).
    Refused(DisconnectReason),
    /// The radio failed.
    Radio(WifiError),
    /// A failure esp-radio added after this was written.
    Other,
}

/// `wifi: not joined (FourWayHandshakeTimeout)`, and the like; never the network's name.
impl Display for JoinError {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        match self {
            JoinError::Saved => out.write_str("wifi: saved network does not fit"),
            JoinError::Config(e) => write!(out, "wifi: not set up ({e:?})"),
            JoinError::Refused(reason) => write!(out, "wifi: not joined ({reason:?})"),
            JoinError::Radio(e) => write!(out, "wifi: radio error ({e:?})"),
            JoinError::Other => out.write_str("wifi: not joined"),
        }
    }
}

/// The station configuration for `network`: WPA2 Personal with its password, open without one.
fn station(network: &Network) -> Result<Config, JoinError> {
    let ssid = Ssid::try_from(network.ssid().as_bytes()).map_err(|_| JoinError::Saved)?;
    let password = network.password().as_bytes();
    let authentication = if password.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        let password = Password::try_from(password).map_err(|_| JoinError::Saved)?;
        AuthenticationMethodConfig::Wpa2Personal(password)
    };
    Ok(Config::Station(
        StationConfig::default()
            .with_ssid(ssid)
            .with_authentication(authentication),
    ))
}

/// Joins `network` with `controller`.
///
/// # Errors
/// The saved network does not fit, the station cannot be set up, or the network was not joined.
pub async fn join(
    controller: &mut WifiController<'_>,
    network: &Network,
) -> Result<Joined, JoinError> {
    controller
        .set_config(&station(network)?)
        .map_err(JoinError::Config)?;
    let connected = controller.connect_async().await.map_err(|e| match e {
        ConnectionError::Failed(info) => JoinError::Refused(info.reason),
        ConnectionError::WifiError(e) => JoinError::Radio(e),
        _ => JoinError::Other,
    })?;
    Ok(Joined {
        channel: connected.channel,
        dbm: controller.rssi().ok(),
    })
}
