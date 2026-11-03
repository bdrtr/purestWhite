//! A remote linked as the C++ firmware links it: the strongest HID remote a scan heard,
//! connected, paired, its HID service set up and its reports read as page turns for a while.
//! What happened goes in an `Outcome`.

use super::hid::{self, Client};
use super::link::{self, Link, Server};
use super::outcome::{Failure, HidNote, LinkNote, Outcome, Plan};
use super::reports::{self, Turns};
use super::scan::listen;
use super::{Hci, Tally, own_address};
use core::cell::RefCell;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Timer, with_timeout};
use esp_hal::peripherals::BT;
use esp_radio::ble::Config;
use esp_radio::ble::controller::BleConnector;
use purestwhite_ble::Seen;
use purestwhite_ble::binding::{PageKeys, Remotes, Router, address_text};
use purestwhite_ble::profiles::profile_for;
use trouble_host::prelude::{
    BleHostError, DefaultPacketPool, Error, ExternalController, GattClient, HostResources,
    IoCapabilities, Stack,
};

/// How long the GATT client's start (the MTU exchange), and then the HID service's setup with
/// the device name, may each take.
const SETUP_MOST: Duration = Duration::from_secs(15);
/// How long the link is given to close.
const CLOSE_MOST: Duration = Duration::from_secs(1);
/// The watchdog's 60 s: the longest stretch with no feed is the GATT client's start and the
/// setup after it.
const _: () = assert!(2 * SETUP_MOST.as_secs() < 60);
const _: () = assert!(link::PAIR_MOST.as_secs() < 60 && link::CONNECT_MOST.as_secs() < 60);

/// Starts BLE on `bt`, scans for `plan.scan`, links to the strongest HID remote and reads its
/// reports for `plan.listen`, then closes the link. `feed` runs between the steps and while the
/// reports are read (a watchdog).
///
/// The host is never dropped: dropping it would shut the controller down (esp-radio's
/// `ble_deinit`) before the caller saves the outcome, a link perhaps still up. It stays on until
/// the reset that follows; its resources hold nothing that needs dropping.
pub async fn remote(bt: BT<'static>, plan: Plan, feed: &mut impl FnMut()) -> Outcome {
    let mut outcome = Outcome::default();
    let Ok(connector) = BleConnector::new(bt, Config::default()) else {
        outcome.failed = Some(Failure::Start);
        return outcome;
    };
    let server = link::server();
    let controller: Hci = ExternalController::new(connector);
    let mut resources: HostResources<_, DefaultPacketPool, 1, 1, 1, 1> = HostResources::new();
    // A display only, as the SDK's host says it is: a remote with no keys pairs "just works".
    let stack = trouble_host::new(controller, &mut resources)
        .set_random_address(own_address())
        .set_io_capabilities(IoCapabilities::DisplayOnly)
        .build();
    let mut runner = stack.runner();
    let tally = Tally(RefCell::new(Seen::new()));
    let session = session(&stack, &server, &tally, plan, &mut outcome, feed);
    let result = match select(runner.run_with_handler(&tally), session).await {
        Either::First(_) => Err(Failure::Host),
        Either::Second(result) => result,
    };
    core::mem::forget(stack);
    outcome.seen = tally.0.into_inner();
    outcome.failed = result.err();
    outcome
}

/// The scan, then the link to the remote it picked, closed after.
async fn session(
    stack: &Stack<'_, Hci, DefaultPacketPool>,
    server: &Server,
    tally: &Tally,
    plan: Plan,
    out: &mut Outcome,
    feed: &mut impl FnMut(),
) -> Result<(), Failure> {
    listen(stack.central(), plan.scan)
        .await
        .map_err(|_| Failure::Scan)?;
    feed();
    let target = tally.0.borrow().best_remote_addr();
    let target = target.ok_or(Failure::NoRemote)?;
    let mut central = stack.central();
    let (conn, connect_ms) = link::connect(&mut central, target).await?;
    let link = conn
        .with_attribute_server(server)
        .map_err(|_| Failure::Gatt)?;
    feed();
    let linked = async {
        let (level, pair_ms) = link::pair(stack, &link).await?;
        out.link = Some(LinkNote {
            connect_ms,
            pair_ms,
            level,
        });
        feed();
        let using = use_hid(stack, &link, target.1, plan.listen, out, feed);
        match select(link::serve(stack, &link), using).await {
            Either::First(()) => Err(Failure::Dropped),
            Either::Second(result) => result,
        }
    };
    let linked = linked.await;
    link.raw().disconnect();
    let _ = select(link::serve(stack, &link), Timer::after(CLOSE_MOST)).await;
    linked
}

/// The GATT client over the link, its task run beside the HID service's use.
async fn use_hid(
    stack: &Stack<'_, Hci, DefaultPacketPool>,
    link: &Link<'_, '_>,
    addr: [u8; 6],
    listen_for: Duration,
    out: &mut Outcome,
    feed: &mut impl FnMut(),
) -> Result<(), Failure> {
    let client: Client = with_timeout(SETUP_MOST, GattClient::new(stack, link.raw()))
        .await
        .map_err(|_| Failure::SetupTimeout)?
        .map_err(|_| Failure::Gatt)?;
    // Listening from the start: a frame that comes during the setup is not lost.
    let listener = client.listen_all().map_err(|_| Failure::Gatt)?;
    let using = async {
        let setup = async {
            let hid = hid::set_up(&client).await?;
            Ok::<_, Failure>((hid, hid::device_name(&client).await))
        };
        let (hid, name) = with_timeout(SETUP_MOST, setup)
            .await
            .map_err(|_| Failure::SetupTimeout)??;
        let remotes = Remotes::default();
        let table = remotes.table_for(&address_text(addr), name.as_bytes());
        out.hid = Some(HidNote {
            inputs: hid.inputs.len(),
            boot: hid.boot,
            map_len: hid.map_len,
            hints: hid.hints,
            profile: profile_for(name.as_bytes()).map(|p| p.name_prefix),
        });
        feed();
        let keys = PageKeys {
            enabled: true,
            ..PageKeys::default()
        };
        let router = Router::new(table, keys);
        let turns = out.turns.insert(Turns::default());
        reports::read(listener, &hid, router, listen_for, turns, feed).await;
        Ok(())
    };
    match select(client.task(), using).await {
        Either::First(Err(BleHostError::BleHost(Error::Disconnected))) => Err(Failure::Dropped),
        Either::First(_) => Err(Failure::Gatt),
        Either::Second(result) => result,
    }
}
