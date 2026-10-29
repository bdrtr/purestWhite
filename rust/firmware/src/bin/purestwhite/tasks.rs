//! The image's tasks: thin wrappers that run the services' loops (`purestwhite_services`) and the
//! key loop on the device's parts, and what they hand back when they rest. `#[task]` functions
//! cannot be generic, so each one names its types here.

use core::sync::atomic::{AtomicBool, Ordering};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Duration;
use esp_hal::gpio::Output;
use purestwhite_app::{App, Refresh};
use purestwhite_firmware::input::ButtonPins;
use purestwhite_firmware::keys::run_keys;
use purestwhite_firmware::screen::{DeviceScreen, ScreenStats};
use purestwhite_render::FRAME_BYTES;
use purestwhite_services::{Links, Screen, ScreenError, run_app, run_draw, run_panel};

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

/// Set once the panel has shown a frame: one of the two conditions for a healthy run
/// (`crate::power`).
pub static SHOWN: AtomicBool = AtomicBool::new(false);

/// The panel, setting `SHOWN` once it has shown a frame.
struct Watched(DeviceScreen);

impl Screen for Watched {
    async fn show(
        &mut self,
        frame: &[u8; FRAME_BYTES],
        refresh: Refresh,
    ) -> Result<(), ScreenError> {
        let shown = self.0.show(frame, refresh).await;
        if shown.is_ok() {
            SHOWN.store(true, Ordering::Release);
        }
        shown
    }

    async fn sleep(&mut self) -> Result<(), ScreenError> {
        self.0.sleep().await
    }
}

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
pub async fn panel(screen: DeviceScreen) {
    let mut screen = Watched(screen);
    let rested = run_panel(&mut screen, LINKS.panel_links(), PANEL_LIMIT).await;
    let Watched(screen) = screen;
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
