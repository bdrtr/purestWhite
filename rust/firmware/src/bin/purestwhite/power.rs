//! The image's power side, run by `main` once the tasks are started: the watchdog fed while the
//! executor runs, then, once the application asks to sleep, the sleep handshake (DESIGN.md,
//! Sleep): the panel rests and hands back its reset line, the keys rest and hand back their pins,
//! Power is let go, and the chip sleeps deeply until Power. During phase 2 the wake boots the
//! C++ firmware, which `boot::start` selected, and the sleep is bounded (`boot::BoundedSleep`):
//! it ends after `SLEEP.timer` at most, and the watchdog counts through it.

use crate::tasks::{KEYS_RESTED, LINKS, PANEL_LIMIT, PANEL_RESTED, PanelRested, SHOWN};
use core::fmt::Write as _;
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, Instant, Timer};
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::peripherals::{GPIO2, GPIO5, LPWR};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::rtc_cntl::sleep::LowPower;
use esp_hal::time::Duration as HalDuration;
use purestwhite_firmware::boot::{self, BoundedSleep, Report};
use purestwhite_firmware::sleep::Rails;

/// How often the watchdog (60 s) is fed while the executor runs.
pub const FEED: Duration = Duration::from_secs(10);
/// How long the image runs, a frame shown, before the run counts as healthy (the boot counter,
/// rust/firmware/DESIGN.md); with a frame shown, reaching the orderly sleep counts too.
const HEALTHY_AFTER: Duration = Duration::from_secs(30);
/// Set by the power loop once it begins the sleep.
pub static SLEEPING: AtomicBool = AtomicBool::new(false);
/// Set by the phase-2 backstop (`crate::backstop`) once it has asked for the sleep.
pub static TIME_UP: AtomicBool = AtomicBool::new(false);
/// How long a loop may take to rest before a reset: the panel's within `PANEL_LIMIT`.
const REST_MOST: Duration = Duration::from_secs(2 * PANEL_LIMIT.as_secs() + FEED.as_secs());
/// The deep sleep: until Power or for 10 minutes at most; if the chip has not woken 2 minutes after
/// that, the watchdog resets its core, and the whole system 30 s after that. Power may stay down
/// 10 s before it starts anyway (a wake pin at its wake level would end it at once).
const SLEEP: BoundedSleep = BoundedSleep {
    timer: HalDuration::from_secs(10 * 60),
    core: HalDuration::from_secs(12 * 60),
    system: HalDuration::from_secs(30),
    release: HalDuration::from_secs(10),
};

/// What the power side owns: the watchdog, the deep sleep, the supplies nothing else uses yet
/// (touch and SD, off), and the report.
pub struct Power {
    rtc: Rtc<'static>,
    lpwr: LPWR<'static>,
    touch_supply: Output<'static>,
    sd_supply: Output<'static>,
    report: Report,
}

impl Power {
    /// The touch panel's and the card's supplies (active low) off, as the C++ firmware's sleep
    /// leaves them.
    pub fn new(
        rtc: Rtc<'static>,
        lpwr: LPWR<'static>,
        touch: GPIO2<'static>,
        sd: GPIO5<'static>,
        report: Report,
    ) -> Self {
        Power {
            rtc,
            lpwr,
            touch_supply: Output::new(touch, Level::High, OutputConfig::default()),
            sd_supply: Output::new(sd, Level::High, OutputConfig::default()),
            report,
        }
    }

    /// Feeds the watchdog until the application asks to sleep, marking the run healthy on the
    /// way (`HEALTHY_AFTER`), then sleeps.
    pub async fn run(mut self) -> ! {
        let started = Instant::now();
        let mut healthy = false;
        while let Either::Second(()) = select(LINKS.sleep.wait(), Timer::after(FEED)).await {
            self.rtc.rwdt.feed();
            if !healthy && SHOWN.load(Ordering::Acquire) && started.elapsed() >= HEALTHY_AFTER {
                self.mark_healthy("30 s");
                healthy = true;
            }
        }
        SLEEPING.store(true, Ordering::Release);
        let why = if TIME_UP.load(Ordering::Acquire) {
            "time up"
        } else {
            "power"
        };
        LINKS.panel_rest.signal(());
        let panel = self.within(PANEL_RESTED.wait(), "panel did not rest").await;
        self.note_panel(&panel);
        LINKS.keys_rest.signal(());
        let pins = self.within(KEYS_RESTED.wait(), "keys did not rest").await;
        // The orderly sleep is reached, both loops at rest: healthy, once a frame was shown.
        if !healthy && SHOWN.load(Ordering::Acquire) {
            self.mark_healthy("sleep");
        }
        let rails = Rails {
            touch_power: self.touch_supply,
            sd_power: self.sd_supply,
            panel_reset: panel.reset,
        };
        let low_power = LowPower::new(self.lpwr);
        let (report, rtc) = (&mut self.report, &mut self.rtc);
        // Blocking from here (the wait for Power, then the sleep): only as the last step, with
        // every other task at rest.
        boot::sleep_when_released(report, why, rtc, pins.power, rails, low_power, SLEEP)
    }

    /// Marks the run healthy for the boot counter and notes `healthy: <why>` in the report.
    fn mark_healthy(&mut self, why: &str) {
        boot::finish_run();
        let _ = writeln!(self.report, "healthy: {why}");
        self.report.save();
    }

    /// `wait`'s result, feeding the watchdog meanwhile; after `REST_MOST`, `what` noted and a
    /// reset.
    async fn within<T>(&mut self, wait: impl Future<Output = T>, what: &str) -> T {
        let until = Instant::now() + REST_MOST;
        let mut wait = core::pin::pin!(wait);
        while Instant::now() < until {
            match select(wait.as_mut(), Timer::after(FEED)).await {
                Either::First(value) => return value,
                Either::Second(()) => self.rtc.rwdt.feed(),
            }
        }
        let _ = writeln!(self.report, "{what}");
        self.report.save();
        esp_hal::system::software_reset()
    }

    /// The panel's line: its calls, and whether it went to sleep.
    fn note_panel(&mut self, panel: &PanelRested) {
        let s = panel.stats;
        let _ = writeln!(
            self.report,
            "panel: {} ok, {} failed, {} given up, longest {} ms, {}",
            s.ok,
            s.failed,
            s.given_up(),
            s.longest_ms,
            if panel.slept { "asleep" } else { "NOT asleep" },
        );
        self.report.save();
    }
}
