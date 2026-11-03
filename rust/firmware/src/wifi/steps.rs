//! The steps the Wi-Fi test images share, each noting its line in the report: joining the saved
//! network (`wifi: joining` stands while it joins, so a run cut short says where it was) and an
//! address from DHCP, the RTC watchdog fed between them.

use crate::boot::Report;
use crate::wifi::{join, net};
use core::fmt::Write as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_net::{Runner, Stack};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::rtc_cntl::Rtc;
use esp_radio::wifi::{Interface, WifiController};
use purestwhite_wifistore::Network;

/// How long joining may take: the access point answers within seconds.
pub const JOIN_MOST: Duration = Duration::from_secs(30);
/// How long DHCP may take to give an address.
pub const DHCP_MOST: Duration = Duration::from_secs(20);
/// How long leaving may take: esp-radio waits for the driver's event without a limit.
pub const LEAVE_MOST: Duration = Duration::from_secs(5);

/// Joins `network`: whether it did; its line in `report`.
pub async fn join_step(
    report: &mut Report,
    rtc: &mut Rtc<'_>,
    controller: &mut WifiController<'_>,
    network: &Network,
) -> bool {
    let before = report.len();
    let _ = writeln!(report, "wifi: joining");
    report.save();
    rtc.rwdt.feed();
    let began = Instant::now();
    let joined = select(join::join(controller, network), Timer::after(JOIN_MOST)).await;
    let ms = began.elapsed().as_millis();
    report.truncate(before);
    let _ = match &joined {
        Either::First(Ok(joined)) => writeln!(report, "{joined}, {ms} ms"),
        Either::First(Err(error)) => writeln!(report, "{error}"),
        Either::Second(()) => writeln!(report, "wifi: no answer in {} s", JOIN_MOST.as_secs()),
    };
    report.save();
    rtc.rwdt.feed();
    matches!(joined, Either::First(Ok(_)))
}

#[embassy_executor::task]
async fn run_net(mut runner: Runner<'static, Interface>) -> ! {
    runner.run().await
}

/// The station's stack with an address from DHCP (its runner spawned); its line in `report`.
pub async fn dhcp_step(
    report: &mut Report,
    rtc: &mut Rtc<'_>,
    spawner: Spawner,
) -> Option<Stack<'static>> {
    let Some((stack, runner)) = net::station_stack() else {
        let _ = writeln!(report, "dhcp: no interface");
        return None;
    };
    spawner.spawn(run_net(runner).ok()?);
    let began = Instant::now();
    let up = select(stack.wait_config_up(), Timer::after(DHCP_MOST)).await;
    let ms = began.elapsed().as_millis();
    let _ = match up {
        Either::First(()) => writeln!(report, "dhcp: address in {ms} ms"),
        Either::Second(()) => writeln!(report, "dhcp: no address in {} s", DHCP_MOST.as_secs()),
    };
    report.save();
    rtc.rwdt.feed();
    matches!(up, Either::First(())).then_some(stack)
}

/// Leaves the network, for `LEAVE_MOST` at most.
pub async fn leave(controller: &mut WifiController<'_>) {
    let _ = select(controller.disconnect_async(), Timer::after(LEAVE_MOST)).await;
}
