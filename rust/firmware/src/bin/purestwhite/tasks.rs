//! The image's tasks: thin wrappers that run the services' loops (`purestwhite_services`) and the
//! key loop on the device's parts, and what they hand back when they rest. `#[task]` functions
//! cannot be generic, so each one names its types here.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use esp_hal::gpio::Output;
use purestwhite_app::App;
use purestwhite_firmware::input::ButtonPins;
use purestwhite_firmware::keys::run_keys;
use purestwhite_firmware::screen::{DeviceScreen, ScreenStats};
use purestwhite_services::{Links, run_app, run_draw, run_panel};

/// The channels' mutex: a plain `static` needs one that is `Sync`.
pub type Mutex = CriticalSectionRawMutex;

/// The channels between the loops.
pub static LINKS: Links<Mutex> = Links::new();

/// What the panel hands the power loop once it rests.
pub struct PanelRested {
    /// Whether the panel went into its deep sleep.
    pub slept: bool,
    pub stats: ScreenStats,
    /// The panel's reset line, for the sleep to hold high.
    pub reset: Output<'static>,
}

pub static PANEL_RESTED: Signal<Mutex, PanelRested> = Signal::new();
/// Once the keys rest: their pins, Power's to arm the wake.
pub static KEYS_RESTED: Signal<Mutex, ButtonPins<'static>> = Signal::new();

/// How long the panel may take over one call before the loop gives it up: a full refresh takes
/// a few seconds, and after a failure the panel is begun first. Two such calls (a frame under
/// way, then the sleep screen) fit in the backstop's `GRACE` (`crate::backstop`).
pub const PANEL_LIMIT: Duration = Duration::from_secs(20);

#[embassy_executor::task]
pub async fn app() -> ! {
    let mut app = App::new();
    run_app(&mut app, LINKS.app_links()).await
}

/// Without a font for now: the screens' shapes only.
#[embassy_executor::task]
pub async fn draw() -> ! {
    run_draw(LINKS.draw_links(), None).await
}

#[embassy_executor::task]
pub async fn panel(mut screen: DeviceScreen) {
    let rested = run_panel(&mut screen, LINKS.panel_links(), PANEL_LIMIT).await;
    PANEL_RESTED.signal(PanelRested {
        slept: rested.is_ok(),
        stats: screen.stats,
        reset: screen.into_reset(),
    });
}

#[embassy_executor::task]
pub async fn keys(pins: ButtonPins<'static>) {
    let pins = run_keys(pins, LINKS.events.dyn_sender(), &LINKS.keys_rest).await;
    KEYS_RESTED.signal(pins);
}
