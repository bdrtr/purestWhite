//! An address from DHCP over embassy-net, on the station interface. The stack's runner moves the
//! frames between embassy-net and the radio: the image runs it in a task of its own.

use alloc::boxed::Box;
use embassy_net::{Config, DhcpConfig, Runner, Stack, StackResources};
use esp_hal::rng::Rng;
use esp_radio::wifi::Interface;

/// The sockets the stack has room for: DHCP's, and two for what comes next (DNS, a download).
pub const SOCKETS: usize = 3;

/// The stack on the station interface, asking DHCP for its address, and its runner; `None` when
/// the station interface is already taken. Its resources stay on the heap for the run.
#[must_use]
pub fn station_stack() -> Option<(Stack<'static>, Runner<'static, Interface>)> {
    let interface = Interface::try_station()?;
    let resources = Box::leak(Box::new(StackResources::<SOCKETS>::new()));
    let rng = Rng::new();
    let seed = u64::from(rng.random()) << 32 | u64::from(rng.random());
    Some(embassy_net::new(
        interface,
        Config::dhcpv4(DhcpConfig::default()),
        resources,
        seed,
    ))
}
