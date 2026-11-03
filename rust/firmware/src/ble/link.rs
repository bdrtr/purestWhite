//! The link to a remote: connected with the SDK's parameters, served by a small GATT server of
//! its own (as the SDK's `NimBLE` host answers a remote's requests), paired as the SDK pairs
//! (bonding, the host a display only), and its link events answered while it lasts.

use super::Hci;
use super::outcome::Failure;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::{Duration, Instant, Timer};
use trouble_host::prelude::{
    AddrKind, Address, AttributeServer, AttributeTable, BdAddr, Central, ConnectConfig, Connection,
    DefaultPacketPool, GattConnection, GattConnectionEvent, RequestedConnParams, ScanConfig,
    SecurityLevel, Service, Stack, appearance, characteristic, service,
};

/// How long the connection may take to come up (the SDK's `kConnectTimeoutMs`).
pub(super) const CONNECT_MOST: Duration = Duration::from_secs(8);
/// How long pairing may take (a remote asks nothing of the user).
pub(super) const PAIR_MOST: Duration = Duration::from_secs(20);
/// The host's name in its GAP service, for a remote that reads it.
const NAME: &str = "purestWhite";
/// The server's attributes: GAP (its service, Device Name and Appearance, each a declaration and
/// a value) and GATT (its service). Exactly as many as `server` adds: `TrouBLE` panics on one more.
const ATTRIBUTES: usize = 6;
/// The passkey the SDK gives a remote that asks for one.
const SDK_PASSKEY: u32 = 123_456;

/// The host's GATT server: GAP and GATT only, so a remote's requests are answered.
pub(super) type Server = AttributeServer<'static, NoopRawMutex, DefaultPacketPool, ATTRIBUTES, 1>;
/// The link, with the server answering the remote's requests.
pub(super) type Link<'s, 'v> = GattConnection<'s, 'v, DefaultPacketPool>;

/// The host's GATT server.
pub(super) fn server() -> Server {
    let mut table = AttributeTable::new();
    let mut gap = table.add_service(Service::new(service::GAP));
    gap.add_characteristic_ro(characteristic::DEVICE_NAME, NAME);
    gap.add_characteristic_ro(characteristic::APPEARANCE, &appearance::UNKNOWN);
    gap.build();
    table.add_service(Service::new(service::GATT));
    AttributeServer::new(table)
}

/// The parameters the SDK asks for (`setConnectionParams(12, 24, 0, 800)`): a 15-30 ms interval
/// for quick turns, no latency, and an 8 s supervision timeout, so a slow page does not drop the
/// link.
fn conn_params() -> RequestedConnParams {
    RequestedConnParams {
        min_connection_interval: Duration::from_millis(15),
        max_connection_interval: Duration::from_millis(30),
        max_latency: 0,
        min_event_length: Duration::from_secs(0),
        max_event_length: Duration::from_secs(0),
        supervision_timeout: Duration::from_secs(8),
    }
}

/// The remote at `addr` (its address kind and bytes, as a scan heard them), connected; how long
/// it took, in ms.
pub(super) async fn connect<'s>(
    central: &mut Central<'s, Hci, DefaultPacketPool>,
    addr: (u8, [u8; 6]),
) -> Result<(Connection<'s, DefaultPacketPool>, u64), Failure> {
    let target = [Address {
        kind: AddrKind::new(addr.0),
        addr: BdAddr::new(addr.1),
    }];
    let config = ConnectConfig {
        scan_config: ScanConfig {
            filter_accept_list: &target,
            ..ScanConfig::default()
        },
        connect_params: conn_params(),
    };
    let began = Instant::now();
    match select(central.connect(&config), Timer::after(CONNECT_MOST)).await {
        Either::First(Ok(conn)) => Ok((conn, began.elapsed().as_millis())),
        Either::First(Err(_)) => Err(Failure::Connect),
        Either::Second(()) => Err(Failure::ConnectTimeout),
    }
}

/// Pairs with the remote, bonding, to its end (the keys handed over, as the SDK's
/// `secureConnection` waits for): its security level and how long it took, in ms. The link's
/// events are answered meanwhile.
pub(super) async fn pair(
    stack: &Stack<'_, Hci, DefaultPacketPool>,
    link: &Link<'_, '_>,
) -> Result<(SecurityLevel, u64), Failure> {
    let conn = link.raw();
    conn.set_bondable(true).map_err(|_| Failure::Pair)?;
    conn.request_security().map_err(|_| Failure::Pair)?;
    let began = Instant::now();
    let paired = async {
        loop {
            match link.next().await {
                GattConnectionEvent::PairingComplete { security_level, .. } => {
                    return Ok(security_level);
                }
                GattConnectionEvent::PairingFailed(_) => return Err(Failure::Pair),
                GattConnectionEvent::Disconnected { .. } => return Err(Failure::Dropped),
                other => answer(stack, link, other).await,
            }
        }
    };
    match select(paired, Timer::after(PAIR_MOST)).await {
        Either::First(level) => Ok((level?, began.elapsed().as_millis())),
        Either::Second(()) => Err(Failure::PairTimeout),
    }
}

/// Answers the link's events until it drops.
pub(super) async fn serve(stack: &Stack<'_, Hci, DefaultPacketPool>, link: &Link<'_, '_>) {
    loop {
        match link.next().await {
            GattConnectionEvent::Disconnected { .. } => return,
            other => answer(stack, link, other).await,
        }
    }
}

/// One link event answered: the remote's GATT request by the server, a parameter update refused
/// (some keyboards drop the link if one is taken on their first key, so the SDK refuses them), a
/// passkey to confirm confirmed, one to enter given as the SDK's fixed 123456; the rest need
/// nothing.
async fn answer(
    stack: &Stack<'_, Hci, DefaultPacketPool>,
    link: &Link<'_, '_>,
    event: GattConnectionEvent<'_, '_, DefaultPacketPool>,
) {
    match event {
        GattConnectionEvent::Gatt { event } => {
            if let Ok(reply) = event.accept() {
                reply.send().await;
            }
        }
        GattConnectionEvent::RequestConnectionParams(request) => {
            let _ = request.reject(stack).await;
        }
        GattConnectionEvent::PassKeyConfirm(_) => {
            let _ = link.raw().pass_key_confirm();
        }
        GattConnectionEvent::PassKeyInput => {
            let _ = link.raw().pass_key_input(SDK_PASSKEY);
        }
        _ => {}
    }
}
